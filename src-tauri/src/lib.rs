mod auth;
mod installation;
mod java;
mod versions;
use tauri::Manager;

#[tauri::command]
async fn minecraft_versions(app: tauri::AppHandle) -> Result<versions::VersionList, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    versions::load(&dir).await
}

#[tauri::command]
async fn detect_java() -> Vec<java::JavaInstallation> {
    java::detect().await
}

#[tauri::command]
fn launcher_status() -> &'static str {
    "Launcher services ready"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(installation::Jobs::default())
        .setup(|app| {
            let service = auth_core::AuthService::new(auth::configuration(app.handle()))
                .map_err(|error| std::io::Error::other(error.message))?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth::auth_restore,
            auth::auth_sign_in,
            auth::auth_cancel,
            auth::auth_sign_out,
            launcher_status,
            minecraft_versions,
            detect_java,
            installation::installed_versions,
            installation::install_version,
            installation::cancel_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
