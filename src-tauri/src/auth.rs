use auth_core::{
    model::{AccountView, AuthError},
    AuthService,
};
use tauri::Manager;

// Only the public application ID belongs in ordinary configuration. Credentials
// go directly to Windows Credential Manager in auth-core.
pub fn configuration(app: &tauri::AppHandle) -> Result<Option<String>, AuthError> {
    if let Ok(value) = std::env::var("EMBER_MICROSOFT_CLIENT_ID") {
        return Ok(Some(value));
    }
    let path = app
        .path()
        .app_config_dir()
        .map_err(|_| {
            AuthError::new(
                "configuration",
                "Could not locate Ember's configuration directory.",
            )
        })?
        .join("auth.json");
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(AuthError::new(
                "configuration",
                "Could not read auth.json. Check its file permissions.",
            ))
        }
    };
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Config {
        microsoft_client_id: String,
    }
    let config: Config = serde_json::from_slice(&bytes).map_err(|_| AuthError::new("configuration", "auth.json must contain only microsoft_client_id. Do not put secrets or tokens in this file."))?;
    Ok(Some(config.microsoft_client_id))
}
#[tauri::command]
pub async fn auth_restore(auth: tauri::State<'_, AuthService>) -> Result<AccountView, AuthError> {
    auth.authenticate(false).await
}
#[tauri::command]
pub async fn auth_sign_in(auth: tauri::State<'_, AuthService>) -> Result<AccountView, AuthError> {
    auth.authenticate(true).await
}
#[tauri::command]
pub fn auth_cancel(auth: tauri::State<'_, AuthService>) {
    auth.cancel();
}
#[tauri::command]
pub async fn auth_sign_out(auth: tauri::State<'_, AuthService>) -> Result<AccountView, AuthError> {
    auth.sign_out().await
}
