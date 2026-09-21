use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex as StdMutex,
};
const ID: &str = "12345678-1234-1234-1234-123456789abc";
fn secret(s: &str) -> Secret {
    Secret::new(s.into())
}
fn profile() -> model::Profile {
    model::Profile {
        name: "EmberTester".into(),
        id: "0123456789abcdef0123456789abcdef".into(),
    }
}
#[derive(Default)]
struct MemoryStore {
    token: StdMutex<Option<String>>,
    fail: bool,
}
impl CredentialStore for Arc<MemoryStore> {
    fn load(&self, _: &str) -> Result<Option<Secret>, AuthError> {
        Ok(self.token.lock().unwrap().as_deref().map(secret))
    }
    fn save(&self, _: &str, token: &Secret) -> Result<(), AuthError> {
        if self.fail {
            return Err(AuthError::new("secure_storage", "storage unavailable"));
        }
        *self.token.lock().unwrap() = Some(token.expose().into());
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), AuthError> {
        if self.fail {
            return Err(AuthError::new("secure_storage", "storage unavailable"));
        }
        *self.token.lock().unwrap() = None;
        Ok(())
    }
}
struct FakeProvider {
    refreshes: Arc<AtomicUsize>,
    failure: Option<&'static str>,
    pending: bool,
}
#[async_trait::async_trait]
impl Provider for FakeProvider {
    async fn login(&self, _: &str) -> Result<MicrosoftTokens, AuthError> {
        if self.pending {
            std::future::pending::<()>().await;
        }
        Ok(MicrosoftTokens {
            access: secret("access-canary"),
            refresh: Some(secret("rotated-canary")),
        })
    }
    async fn refresh(&self, _: &str, _: &Secret) -> Result<MicrosoftTokens, AuthError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        if self.failure == Some("reauthenticate") {
            return Err(AuthError::new("reauthenticate", "Sign in again"));
        }
        self.login(ID).await
    }
    async fn minecraft(&self, _: &Secret) -> Result<MinecraftSession, AuthError> {
        if let Some(code) = self.failure {
            return Err(AuthError::new(code, "Account verification failed"));
        }
        Ok(MinecraftSession {
            profile: profile(),
            _access: secret("minecraft-canary"),
            expires_at: model::now() + 3600,
        })
    }
}
fn service(
    store: Arc<MemoryStore>,
    failure: Option<&'static str>,
    pending: bool,
) -> (AuthService, Arc<AtomicUsize>) {
    let refreshes = Arc::new(AtomicUsize::new(0));
    let service = AuthService {
        client_id: Some(ID.into()),
        store: Box::new(store),
        provider: Box::new(FakeProvider {
            refreshes: refreshes.clone(),
            failure,
            pending,
        }),
        state: Mutex::new(State {
            restore_blocked: false,
            session: None,
            view: AccountView::signed_out(true),
        }),
        cancel: StdMutex::new(CancellationToken::new()),
    };
    (service, refreshes)
}
#[test]
fn pkce_matches_rfc7636_vector() {
    assert_eq!(
        oauth::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}
#[tokio::test]
async fn authorization_uses_pkce_and_public_client_only() {
    let a = oauth::Authorization::bind(ID).await.unwrap();
    let b = oauth::Authorization::bind(ID).await.unwrap();
    assert_ne!(a.state.expose(), b.state.expose());
    assert_ne!(a.verifier.expose(), b.verifier.expose());
    let query: std::collections::HashMap<_, _> = a.url.query_pairs().collect();
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["response_type"], "code");
    assert!(!query.contains_key("client_secret"));
    assert_eq!(query["scope"], "XboxLive.signin offline_access");
}
fn callback(query: &str) -> Option<Result<Secret, AuthError>> {
    oauth::callback(
        &format!("GET /?{query} HTTP/1.1\r\nHost: localhost:12345\r\n\r\n"),
        "http://localhost:12345/",
        "expected",
    )
}
#[test]
fn callback_validates_state_and_rejects_ambiguous_data() {
    assert_eq!(
        callback("code=abc&state=expected")
            .unwrap()
            .unwrap()
            .expose(),
        "abc"
    );
    for query in [
        "code=abc",
        "code=abc&state=wrong",
        "code=abc&state=expected&state=expected",
        "code=a&code=b&state=expected",
        "code=a&error=access_denied&state=expected",
        "code=&state=expected",
        "code=a&state=expected#fragment",
    ] {
        assert!(callback(query).is_none(), "{query}");
    }
    assert_eq!(
        callback("error=access_denied&state=expected")
            .unwrap()
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    for request in [
        "POST /?state=expected&code=a HTTP/1.1\r\nHost: localhost:12345\r\n\r\n",
        "GET /?state=expected&code=a HTTP/1.1\r\nHost: evil.example\r\n\r\n",
        "GET /wrong?state=expected&code=a HTTP/1.1\r\nHost: localhost:12345\r\n\r\n",
    ] {
        assert!(oauth::callback(request, "http://localhost:12345/", "expected").is_none());
    }
}
#[test]
fn expiry_uses_safety_window() {
    assert!(!model::needs_refresh(1121, 1000));
    assert!(model::needs_refresh(1120, 1000));
    assert!(model::needs_refresh(999, 1000));
    assert!(model::needs_refresh(u64::MAX, u64::MAX));
}
#[test]
fn profile_and_entitlements_are_both_required() {
    assert!(profile().validate().is_ok());
    let mut bad = profile();
    bad.id = "not-a-uuid".into();
    assert!(bad.validate().is_err());
    let mut bad = profile();
    bad.name = "<script>".into();
    assert!(bad.validate().is_err());
    assert!(api::verify_entitlements(&json!({"items":[{"name":"game_minecraft"}]})).is_ok());
    for v in [
        json!({"items":[]}),
        json!({"items":[{"name":"unrelated_product"}]}),
        json!({}),
    ] {
        assert!(api::verify_entitlements(&v).is_err());
    }
}
#[test]
fn maps_provider_errors_without_echoing_tokens() {
    assert_eq!(
        model::service_error("Microsoft", 400, Some("invalid_grant"), None).code,
        "reauthenticate"
    );
    assert_eq!(
        model::service_error("Minecraft profile", 404, None, None).code,
        "profile_missing"
    );
    assert_eq!(
        model::service_error("Minecraft login", 403, None, None).code,
        "minecraft_access"
    );
    assert!(
        model::service_error("Xbox XSTS", 401, None, Some(2148916238))
            .message
            .contains("child")
    );
    assert!(
        model::service_error("Xbox XSTS", 401, None, Some(2148916233))
            .message
            .contains("Xbox profile")
    );
    assert!(
        !model::service_error("Microsoft", 400, Some("secret-canary"), None)
            .message
            .contains("secret-canary")
    );
}
#[test]
fn validates_client_id() {
    assert_eq!(validate_client_id(ID).unwrap(), ID);
    assert!(validate_client_id("https://attacker.test").is_err());
    assert!(validate_client_id("00000000-0000-0000-0000-000000000000").is_err());
}
#[tokio::test]
async fn login_restore_refresh_and_sign_out_keep_secrets_backend_only() {
    let store = Arc::new(MemoryStore::default());
    let (s, count) = service(store.clone(), None, false);
    let view = s.authenticate(true).await.unwrap();
    assert_eq!(view.status, "signed_in");
    let serialized = serde_json::to_string(&view).unwrap();
    assert!(!serialized.contains("canary"));
    s.authenticate(false).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 0);
    s.state.lock().await.session.as_mut().unwrap().expires_at = 0;
    s.authenticate(false).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let (restarted, count) = service(store.clone(), None, false);
    assert_eq!(
        restarted
            .authenticate(false)
            .await
            .unwrap()
            .profile
            .unwrap(),
        profile()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(restarted.sign_out().await.unwrap().status, "signed_out");
    assert!(store.token.lock().unwrap().is_none());
    assert!(restarted.state.lock().await.session.is_none());
}
#[tokio::test]
async fn rotation_survives_downstream_network_failure_but_revocation_clears_store() {
    let store = Arc::new(MemoryStore::default());
    *store.token.lock().unwrap() = Some("old".into());
    let (s, _) = service(store.clone(), Some("network"), false);
    assert_eq!(s.authenticate(false).await.unwrap().status, "error");
    assert_eq!(
        store.token.lock().unwrap().as_deref(),
        Some("rotated-canary")
    );
    let (s, _) = service(store.clone(), Some("reauthenticate"), false);
    s.authenticate(false).await.unwrap();
    assert!(store.token.lock().unwrap().is_none());
}
#[tokio::test]
async fn storage_or_ownership_failure_never_marks_account_playable() {
    let store = Arc::new(MemoryStore {
        fail: true,
        ..Default::default()
    });
    let (s, _) = service(store, None, false);
    assert_eq!(s.authenticate(true).await.unwrap().status, "error");
    assert!(s.state.lock().await.session.is_none());
    let store = Arc::new(MemoryStore::default());
    let (s, _) = service(store.clone(), Some("no_ownership"), false);
    assert!(s.authenticate(true).await.unwrap().profile.is_none());
    assert!(store.token.lock().unwrap().is_none());
}
#[tokio::test]
async fn cancellation_and_signout_cannot_resurrect_a_session() {
    let (s, _) = service(Arc::new(MemoryStore::default()), None, true);
    let s = Arc::new(s);
    let other = s.clone();
    let task = tokio::spawn(async move { other.authenticate(true).await });
    tokio::task::yield_now().await;
    assert_eq!(s.authenticate(true).await.err().unwrap().code, "busy");
    let view = s.sign_out().await.unwrap();
    assert!(view.profile.is_none());
    assert_eq!(task.await.unwrap().unwrap().status, "signed_out");
    assert!(s.state.lock().await.session.is_none());
}
#[cfg(windows)]
#[test]
fn windows_credential_manager_round_trip() {
    // Isolated CI credential, no real account or user credential is touched.
    let id = format!("test-{}-{}", std::process::id(), model::now());
    let store = OsStore;
    store
        .save(&id, &secret("non-production-storage-test"))
        .unwrap();
    let read = store.load(&id);
    let deleted = store.delete(&id);
    assert_eq!(
        read.unwrap().unwrap().expose(),
        "non-production-storage-test"
    );
    deleted.unwrap();
    assert!(store.load(&id).unwrap().is_none());
}

#[tokio::test]
async fn failed_credential_removal_blocks_automatic_restoration() {
    let store = Arc::new(MemoryStore {
        fail: true,
        token: StdMutex::new(Some("saved".into())),
    });
    let (s, count) = service(store, None, false);
    assert_eq!(s.sign_out().await.unwrap().status, "error");
    s.authenticate(false).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn loopback_rejects_forged_callback_then_accepts_real_one_without_echoing_secrets() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let a = Arc::new(oauth::Authorization::bind(ID).await.unwrap());
    let port = url::Url::parse(&a.redirect).unwrap().port().unwrap();
    let other = a.clone();
    let receiving = tokio::spawn(async move { other.receive().await });
    for (state, expected_status) in [("wrong", "400 Bad Request"), (a.state.expose(), "200 OK")] {
        let mut socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        socket.write_all(format!("GET /?code=callback-secret&state={state} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n").as_bytes()).await.unwrap();
        let mut response = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            socket.read_to_string(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.contains(expected_status));
        assert!(!response.contains("callback-secret"));
        assert!(!response.contains(a.state.expose()));
    }
    assert_eq!(
        receiving.await.unwrap().unwrap().expose(),
        "callback-secret"
    );
}
