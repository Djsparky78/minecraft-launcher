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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "x86" => "x86",
            "aarch64" => "aarch64",
            _ => return Err("Unsupported Windows architecture".into()),
        };
        // cmd's built-in ver reports the OS version used by Mojang's regex rules.
        let output = std::process::Command::new(
            PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot is unavailable")?)
                .join("System32/cmd.exe"),
        )
        .args(["/D", "/C", "ver"])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("Could not detect Windows version: {e}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        let version = text
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .find(|part| part.contains('.') && part.chars().any(|c| c.is_ascii_digit()))
            .ok_or("Could not parse Windows version")?;
        return Ok(Platform::windows(arch, version));
    }
    #[cfg(not(windows))]
    Err("Minecraft installation currently supports Windows only.".into())
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
