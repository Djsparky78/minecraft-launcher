mod api;
pub mod model;
mod oauth;
pub mod store;
use api::{Api, MicrosoftTokens, MinecraftSession};
use model::{AccountView, AuthError, Secret};
use store::{CredentialStore, OsStore};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
trait Provider: Send + Sync {
    async fn login(&self, client_id: &str) -> Result<MicrosoftTokens, AuthError>;
    async fn refresh(
        &self,
        client_id: &str,
        refresh: &Secret,
    ) -> Result<MicrosoftTokens, AuthError>;
    async fn minecraft(&self, access: &Secret) -> Result<MinecraftSession, AuthError>;
}
#[async_trait::async_trait]
impl Provider for Api {
    async fn login(&self, client_id: &str) -> Result<MicrosoftTokens, AuthError> {
        let authorization = oauth::Authorization::bind(client_id).await?;
        webbrowser::open(authorization.url.as_str()).map_err(|_| AuthError::new("browser", "Could not open your default browser. Set a default browser in Windows Settings and retry."))?;
        let code = authorization.receive().await?;
        self.code(
            client_id,
            &code,
            &authorization.verifier,
            &authorization.redirect,
        )
        .await
    }
    async fn refresh(
        &self,
        client_id: &str,
        refresh: &Secret,
    ) -> Result<MicrosoftTokens, AuthError> {
        self.refresh(client_id, refresh).await
    }
    async fn minecraft(&self, access: &Secret) -> Result<MinecraftSession, AuthError> {
        self.minecraft(access).await
    }
}
struct State {
    restore_blocked: bool,
    session: Option<MinecraftSession>,
    view: AccountView,
}
pub struct AuthService {
    client_id: Option<String>,
    provider: Box<dyn Provider>,
    store: Box<dyn CredentialStore>,
    state: Mutex<State>,
    cancel: std::sync::Mutex<CancellationToken>,
}
pub fn validate_client_id(value: &str) -> Result<String, AuthError> {
    uuid::Uuid::parse_str(value.trim()).ok().filter(|id| !id.is_nil())
        .map(|id| id.hyphenated().to_string())
        .ok_or_else(|| AuthError::new("configuration", "Microsoft client ID must be your application's non-empty GUID. Check auth.json or EMBER_MICROSOFT_CLIENT_ID."))
}
impl AuthService {
    pub fn new(config: Result<Option<String>, AuthError>) -> Result<Self, AuthError> {
        let (client_id, error) = match config {
            Ok(Some(id)) => match validate_client_id(&id) {
                Ok(id) => (Some(id), None),
                Err(e) => (None, Some(e)),
            },
            Ok(None) => (None, None),
            Err(e) => (None, Some(e)),
        };
        let mut view = AccountView::signed_out(client_id.is_some());
        if let Some(e) = error {
            view.status = "error";
            view.message = e.message;
        }
        Ok(Self {
            client_id,
            provider: Box::new(Api::new()?),
            store: Box::new(OsStore),
            state: Mutex::new(State {
                restore_blocked: false,
                session: None,
                view,
            }),
            cancel: std::sync::Mutex::new(CancellationToken::new()),
        })
    }
    pub fn cancel(&self) {
        if let Ok(token) = self.cancel.lock() {
            token.cancel();
        }
    }
    // Both restoration and periodic refresh use this command. Only one auth
    // operation runs at a time. UI never receives an access or refresh token.
    pub async fn authenticate(&self, interactive: bool) -> Result<AccountView, AuthError> {
        let mut state = self.state.try_lock().map_err(|_| {
            AuthError::new(
                "busy",
                "Another account operation is running. Wait or cancel it first.",
            )
        })?;
        let Some(client_id) = &self.client_id else {
            return Ok(state.view.clone());
        };
        if !interactive && state.restore_blocked {
            return Ok(state.view.clone());
        }
        if interactive {
            state.restore_blocked = false;
        }
        if !interactive
            && state
                .session
                .as_ref()
                .is_some_and(|s| !model::needs_refresh(s.expires_at, model::now()))
        {
            return Ok(state.view.clone());
        }
        let cancel = CancellationToken::new();
        *self.cancel.lock().map_err(|_| {
            AuthError::new("internal", "Account state is unavailable. Restart Ember.")
        })? = cancel.clone();
        // Clear stale playable identity before attempting to refresh.
        state.session = None;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(AuthError::cancelled()),
            result = self.exchange(client_id, interactive) => result,
        };
        match result {
            Ok(Some(session)) => {
                state.view = AccountView { status: "signed_in", configured: true, profile: Some(session.profile.clone()), message: "Minecraft Java Edition access verified. Game launching comes in milestone 5.".into() };
                state.session = Some(session);
            }
            Ok(None) => {
                state.view = AccountView::signed_out(true);
            }
            Err(error) => {
                let mut error = error;
                if error.code == "reauthenticate" {
                    if let Err(storage) = self.store.delete(client_id) {
                        error = storage;
                    }
                }
                state.view = AccountView {
                    status: if error.code == "cancelled" {
                        "signed_out"
                    } else {
                        "error"
                    },
                    configured: true,
                    profile: None,
                    message: error.message,
                };
            }
        }
        Ok(state.view.clone())
    }
    async fn exchange(
        &self,
        client_id: &str,
        interactive: bool,
    ) -> Result<Option<MinecraftSession>, AuthError> {
        let tokens = if interactive {
            // Ensure the OS-backed store is accessible before starting browser auth.
            let _ = self.store.load(client_id)?;
            self.provider.login(client_id).await?
        } else {
            let Some(refresh) = self.store.load(client_id)? else {
                return Ok(None);
            };
            let tokens = self.provider.refresh(client_id, &refresh).await?;
            // Persist rotation immediately, even if the subsequent Xbox/Minecraft
            // network calls fail. Never discard a working saved token on a timeout.
            if let Some(refresh) = &tokens.refresh {
                self.store.save(client_id, refresh)?;
            }
            tokens
        };
        let session = self.provider.minecraft(&tokens.access).await?;
        if interactive {
            let refresh = tokens.refresh.as_ref().ok_or_else(|| AuthError::new("refresh_missing", "Microsoft did not provide a refresh credential. Check offline_access consent and sign in again."))?;
            self.store.save(client_id, refresh)?;
        }
        Ok(Some(session))
    }
    pub async fn sign_out(&self) -> Result<AccountView, AuthError> {
        self.cancel();
        let mut state = self.state.lock().await;
        state.session = None;
        state.restore_blocked = true;
        state.view = AccountView::signed_out(self.client_id.is_some());
        if let Some(id) = &self.client_id {
            if self.store.delete(id).is_err() {
                state.view.status = "error";
                state.view.message = "Signed out in memory, but Windows could not remove the saved credential. Retry Sign out, or remove Ember's entry in Windows Credential Manager before closing the app.".into();
                return Ok(state.view.clone());
            }
        }
        state.view.message =
            "Signed out of Ember. Your Microsoft browser session is unchanged.".into();
        Ok(state.view.clone())
    }
}

#[cfg(test)]
mod tests;
