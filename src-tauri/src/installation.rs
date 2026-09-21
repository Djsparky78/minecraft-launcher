use installer_core::{
    model::{Download, Platform},
    Installation, Progress,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{ipc::Channel, Manager};

type ActiveJob = Option<(String, Arc<AtomicBool>)>;
#[derive(Default, Clone)]
pub struct Jobs(Arc<Mutex<ActiveJob>>);
struct JobGuard(Jobs);
impl Drop for JobGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0 .0.lock() {
            *active = None;
        }
    }
}
fn root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("game"))
}
fn platform() -> Result<Platform, String> {
    installer_core::platform::detect()
}
#[tauri::command]
pub async fn installed_versions(app: tauri::AppHandle) -> Result<Vec<Installation>, String> {
    let root = root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        installer_core::list(&root).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Installation check failed: {e}"))?
}
#[tauri::command]
pub async fn install_version(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, Jobs>,
    version: String,
    on_progress: Channel<Progress>,
) -> Result<(), String> {
    installer_core::model::valid_id(&version).map_err(|e| e.to_string())?;
    let platform = platform()?;
    let root = root(&app)?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = jobs
            .inner()
            .0
            .lock()
            .map_err(|_| "Installation state unavailable")?;
        if active.is_some() {
            return Err("An installation or repair is already running.".into());
        }
        *active = Some((version.clone(), cancel.clone()));
    }
    let guard = JobGuard(jobs.inner().clone());
    let entry = crate::versions::resolve(&cache, &version).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        installer_core::install(
            &root,
            &version,
            Download {
                url: entry.url,
                sha1: entry.sha1,
                size: None,
                path: String::new(),
            },
            platform,
            &cancel,
            |progress| {
                // UI reloads/disconnects do not invalidate a verified download.
                let _ = on_progress.send(progress);
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Installer worker failed: {e}"))?
}
#[tauri::command]
pub fn cancel_install(jobs: tauri::State<'_, Jobs>, version: String) -> Result<(), String> {
    let active = jobs
        .inner()
        .0
        .lock()
        .map_err(|_| "Installation state unavailable")?;
    if let Some((id, cancel)) = &*active {
        if id == &version {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}
