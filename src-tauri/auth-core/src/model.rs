use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

// Secrets deliberately have neither Debug nor Serialize implementations.
pub struct Secret(pub Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub name: String,
    pub id: String,
}
impl Profile {
    pub fn validate(self) -> Result<Self, AuthError> {
        if self.id.len() != 32
            || !self.id.bytes().all(|b| b.is_ascii_hexdigit())
            || !(1..=16).contains(&self.name.len())
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(AuthError::new(
                "profile",
                "Minecraft returned an invalid player profile. Try again later.",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountView {
    pub status: &'static str,
    pub configured: bool,
    pub profile: Option<Profile>,
    pub message: String,
}
impl AccountView {
    pub fn signed_out(configured: bool) -> Self {
        Self {
            status: "signed_out",
            configured,
            profile: None,
            message: if configured {
                "Sign in to verify your Minecraft Java Edition account.".into()
            } else {
                "Microsoft sign-in needs your application client ID. See README setup instructions."
                    .into()
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthError {
    pub code: &'static str,
    pub message: String,
}
impl AuthError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn cancelled() -> Self {
        Self::new("cancelled", "Sign-in cancelled. You can try again.")
    }
    pub fn network(stage: &str) -> Self {
        Self::new("network", format!("Could not reach {stage}. Check your connection and try again. Your saved sign-in has been kept."))
    }
}

pub fn needs_refresh(expires_at: u64, now: u64) -> bool {
    expires_at <= now.saturating_add(120)
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn service_error(
    stage: &str,
    status: u16,
    oauth: Option<&str>,
    xerr: Option<u64>,
) -> AuthError {
    if oauth == Some("invalid_grant") {
        return AuthError::new(
            "reauthenticate",
            "Your Microsoft session expired or was revoked. Sign in again.",
        );
    }
    if matches!(
        oauth,
        Some("invalid_client" | "unauthorized_client" | "invalid_scope")
    ) {
        return AuthError::new("configuration", "Microsoft rejected this application configuration. Check the client ID, personal-account support, native redirect URI, and Xbox permissions in the README.");
    }
    let message = match xerr {
        Some(2148916233) => Some("This Microsoft account needs an Xbox profile. Sign in at xbox.com to create one, then retry."),
        Some(2148916235) => Some("Xbox Live is not available for this account's country or region."),
        Some(2148916236 | 2148916237) => Some("Xbox requires age verification. Complete it through your Microsoft/Xbox account, then retry."),
        Some(2148916238) => Some("This child account needs Microsoft family setup and permission from its adult organizer before using Xbox Live."),
        _ => None,
    };
    if let Some(message) = message {
        return AuthError::new("xbox_account", message);
    }
    if stage == "Minecraft profile" && status == 404 {
        return AuthError::new("profile_missing", "No Minecraft Java profile was found. Set up your Java Edition username on minecraft.net, then retry.");
    }
    if stage == "Minecraft login" && matches!(status, 401 | 403) {
        return AuthError::new("minecraft_access", "Minecraft rejected sign-in. Your application may need Minecraft API approval. Check the registration instructions; do not use another launcher's client ID.");
    }
    if status == 429 {
        return AuthError::new(
            "rate_limit",
            format!("{stage} is receiving too many requests. Wait a few minutes before retrying."),
        );
    }
    AuthError::new("service", format!("{stage} could not complete sign-in (HTTP {status}). Try again later; if this persists, check your Xbox account and application registration."))
}
