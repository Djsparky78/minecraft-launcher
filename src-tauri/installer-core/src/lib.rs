pub mod model;
pub mod storage;
use fs2::FileExt;
use model::{
    asset_tasks, plan, safe_relative, task, valid_id, AssetIndex, Download, Metadata, Platform,
    Task,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use storage::{atomic_json, fingerprint, safe_join, Downloader, VerifiedFile};

#[derive(Debug)]
pub enum Error {
    Cancelled,
    Message(String),
}
impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(
                f,
                "Installation cancelled. Verified files were kept; install again to resume."
            ),
            Self::Message(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::message(format!(
            "File operation failed (check permissions and free disk space): {e}"
        ))
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::message(format!("Invalid Minecraft metadata: {e}"))
    }
}
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::message(format!(
            "Download failed. Check your connection and retry: {e}"
        ))
    }
}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::message(format!("Cannot extract native library: {e}"))
    }
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
#[derive(Clone, Serialize)]
pub struct Progress {
    pub version: String,
    pub stage: String,
    pub file: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub percent: f64,
    pub file_bytes: u64,
    pub file_total: Option<u64>,
}
#[derive(Deserialize, Serialize)]
struct Receipt {
    schema: u32,
    version: String,
    status: String,
    error: Option<String>,
    files: Vec<VerifiedFile>,
    platform: Platform,
}
#[derive(Clone, Serialize)]
pub struct Installation {
    pub version: String,
    pub status: String,
    pub location: String,
    pub error: Option<String>,
}
pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
    }
}
pub fn acquire_lock(root: &Path) -> Result<File> {
    fs::create_dir_all(root)?;
    let path = safe_join(root, "launcher/install.lock")?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| Error::message("Missing lock parent"))?,
    )?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive().map_err(|_| {
        Error::message("Another installation or repair is already running. Wait for it to finish.")
    })?;
    Ok(file)
}
fn receipt_path(id: &str) -> String {
    format!("launcher/installations/{id}.json")
}
pub fn list(root: &Path) -> Result<Vec<Installation>> {
    let directory = safe_join(root, "launcher/installations")?;
    let entries = match fs::read_dir(directory) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let cancel = AtomicBool::new(false);
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.path().extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let id = entry
            .path()
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if valid_id(&id).is_err() {
            continue;
        }
        let location = display_path(&root.join("versions").join(&id));
        let parsed = fs::read(safe_join(root, &receipt_path(&id))?)
            .ok()
            .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
        let Some(receipt) = parsed else {
            result.push(Installation {
                version: id,
                location,
                status: "needs_repair".into(),
                error: Some("Installation record is unreadable. Repair this version.".into()),
            });
            continue;
        };
        let mut status = receipt.status;
        let mut error = receipt.error;
        if status == "installing" {
            status = "incomplete".into();
            error = Some("Interrupted installation. Install again to resume.".into());
        }
        if status == "installed" {
            if receipt.schema != 1 || receipt.version != id || receipt.files.is_empty() {
                status = "needs_repair".into();
            } else {
                // Validate all files, including extracted natives. No network is needed.
                for file in &receipt.files {
                    let task = Task {
                        path: file.path.clone(),
                        stage: String::new(),
                        download: Download {
                            url: String::new(),
                            sha1: Some(file.sha1.clone()),
                            size: Some(file.size),
                            path: String::new(),
                        },
                    };
                    match storage::verify(root, &task, &cancel) {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            status = "needs_repair".into();
                            error = Some(format!("Missing or damaged file: {}", file.path));
                            break;
                        }
                        Err(e) => {
                            status = "needs_repair".into();
                            error = Some(e.to_string());
                            break;
                        }
                    }
                }
            }
        }
        result.push(Installation {
            version: id,
            status,
            location,
            error,
        });
    }
    result.sort_by(|a, b| a.version.cmp(&b.version));
    Ok(result)
}
pub fn install(
    root: &Path,
    id: &str,
    metadata_download: Download,
    platform: Platform,
    cancel: &AtomicBool,
    mut notify: impl FnMut(Progress),
) -> Result<()> {
    valid_id(id)?;
    let _lock = acquire_lock(root)?;
    let staging_path = safe_join(root, "launcher/partial")?;
    // This directory is owned exclusively by the installer lock; stale partial files
    // from a killed process are never reused or treated as complete downloads.
    if staging_path.exists() {
        fs::remove_dir_all(&staging_path)?;
    }
    fs::create_dir_all(&staging_path)?;
    let staging = tempfile::tempdir_in(&staging_path)?;
    let mut receipt = Receipt {
        schema: 1,
        version: id.into(),
        status: "installing".into(),
        error: None,
        files: Vec::new(),
        platform: platform.clone(),
    };
    atomic_json(root, &receipt_path(id), &receipt)?;
    let result = install_inner(
        root,
        id,
        metadata_download,
        &platform,
        staging.path(),
        cancel,
        &mut notify,
    );
    match result {
        Ok(files) => {
            receipt.files = files;
            receipt.status = "installed".into();
            atomic_json(root, &receipt_path(id), &receipt)?;
            notify(Progress {
                version: id.into(),
                stage: "Installed".into(),
                file: String::new(),
                completed_files: receipt.files.len(),
                total_files: receipt.files.len(),
                percent: 100.0,
                file_bytes: 0,
                file_total: None,
            });
            Ok(())
        }
        Err(error) => {
            receipt.status = if matches!(error, Error::Cancelled) {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            receipt.error = Some(error.to_string());
            if let Err(save_error) = atomic_json(root, &receipt_path(id), &receipt) {
                return Err(Error::message(format!(
                    "{error}; could not save install status: {save_error}"
                )));
            }
            Err(error)
        }
    }
}
fn install_inner(
    root: &Path,
    id: &str,
    metadata_download: Download,
    platform: &Platform,
    staging: &Path,
    cancel: &AtomicBool,
    notify: &mut impl FnMut(Progress),
) -> Result<Vec<VerifiedFile>> {
    let downloader = Downloader::new()?;
    let emit = |stage: &str, file: &str, done, total, percent, bytes, size| Progress {
        version: id.into(),
        stage: stage.into(),
        file: file.into(),
        completed_files: done,
        total_files: total,
        percent,
        file_bytes: bytes,
        file_total: size,
    };
    notify(emit("Version metadata", id, 0, 0, 0.0, 0, None));
    let version_task = task(
        format!("versions/{id}/{id}.json"),
        "Version metadata",
        metadata_download,
    )?;
    let mut files = vec![downloader.ensure(root, &version_task, staging, cancel, |_| {})?];
    let metadata: Metadata =
        serde_json::from_reader(File::open(safe_join(root, &version_task.path)?)?)?;
    if metadata.id != id {
        return Err(Error::message(
            "Downloaded version ID does not match selection",
        ));
    }
    let mut plan = plan(&metadata, platform)?;
    let index_task = task(
        format!("assets/indexes/{}.json", metadata.asset_index.id),
        "Asset index",
        metadata.asset_index.download.clone(),
    )?;
    notify(emit("Asset index", &index_task.path, 0, 0, 0.0, 0, None));
    files.push(downloader.ensure(root, &index_task, staging, cancel, |_| {})?);
    let index: AssetIndex =
        serde_json::from_reader(File::open(safe_join(root, &index_task.path)?)?)?;
    plan.tasks.extend(asset_tasks(&index)?);
    let total = plan.tasks.len();
    for (done, task) in plan.tasks.iter().enumerate() {
        check_cancel(cancel)?;
        notify(emit(
            &format!("Verify / {}", task.stage),
            &task.path,
            done,
            total,
            done as f64 / total.max(1) as f64 * 90.0,
            0,
            task.download.size,
        ));
        let mut last = Instant::now();
        let file = downloader
            .ensure(root, task, staging, cancel, |bytes| {
                if last.elapsed().as_millis() >= 100 {
                    let part = task
                        .download
                        .size
                        .filter(|s| *s > 0)
                        .map(|s| (bytes as f64 / s as f64).min(1.0))
                        .unwrap_or(0.0);
                    notify(emit(
                        &task.stage,
                        &task.path,
                        done,
                        total,
                        (done as f64 + part) / total.max(1) as f64 * 90.0,
                        bytes,
                        task.download.size,
                    ));
                    last = Instant::now();
                }
            })
            .map_err(|e| match e {
                Error::Cancelled => Error::Cancelled,
                _ => Error::message(format!("{}: {e}", task.path)),
            })?;
        files.push(file);
    }
    notify(emit("Legacy asset layout", "", total, total, 91.0, 0, None));
    let objects: BTreeMap<_, _> = files
        .iter()
        .filter(|f| f.path.starts_with("assets/objects/"))
        .map(|f| (f.sha1.clone(), f.clone()))
        .collect();
    for (name, asset) in &index.objects {
        check_cancel(cancel)?;
        if !index.virtual_assets && !index.map_to_resources {
            break;
        }
        safe_relative(name)?;
        let source = objects
            .get(&asset.hash)
            .ok_or_else(|| Error::message("Missing asset object"))?;
        if index.virtual_assets {
            files.push(storage::materialize(
                root,
                source,
                &format!("assets/virtual/{}/{name}", metadata.asset_index.id),
                staging,
                cancel,
            )?);
        }
        if index.map_to_resources {
            files.push(storage::materialize(
                root,
                source,
                &format!("versions/{id}/resources/{name}"),
                staging,
                cancel,
            )?);
        }
    }
    notify(emit("Extracting natives", "", total, total, 95.0, 0, None));
    let native_dir = tempfile::tempdir_in(staging)?;
    for native in &plan.natives {
        notify(emit(
            "Extracting natives",
            &native.archive,
            total,
            total,
            95.0,
            0,
            None,
        ));
        extract_native(
            &safe_join(root, &native.archive)?,
            native_dir.path(),
            &native.exclude,
            cancel,
        )?;
    }
    let destination = safe_join(root, &format!("natives/{id}"))?;
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| Error::message("Missing native parent"))?,
    )?;
    // Receipt remains incomplete until the directory and all extracted hashes are saved.
    check_cancel(cancel)?;
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(native_dir.path(), &destination)?;
    collect_native_files(root, &format!("natives/{id}"), cancel, &mut files)?;
    check_cancel(cancel)?;
    Ok(files)
}
pub fn extract_native(
    archive: &Path,
    destination: &Path,
    excludes: &[String],
    cancel: &AtomicBool,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    for i in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if entry.is_dir()
            || name.starts_with("META-INF/")
            || excludes.iter().any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        if entry.enclosed_name().is_none()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::message("Unsafe native archive entry"));
        }
        safe_relative(&name)?;
        let dest = safe_join(destination, &name)?;
        fs::create_dir_all(
            dest.parent()
                .ok_or_else(|| Error::message("Missing native parent"))?,
        )?;
        let mut file = File::create(dest)?;
        let mut buffer = [0; 65536];
        loop {
            check_cancel(cancel)?;
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])?;
        }
        file.sync_all()?;
    }
    Ok(())
}
fn collect_native_files(
    root: &Path,
    relative: &str,
    cancel: &AtomicBool,
    files: &mut Vec<VerifiedFile>,
) -> Result<()> {
    for entry in fs::read_dir(safe_join(root, relative)?)? {
        check_cancel(cancel)?;
        let entry = entry?;
        let path = format!("{relative}/{}", entry.file_name().to_string_lossy());
        let full = safe_join(root, &path)?;
        if full.is_dir() {
            collect_native_files(root, &path, cancel, files)?;
        } else {
            let (sha1, size) = fingerprint(&full, cancel)?;
            files.push(VerifiedFile { path, sha1, size });
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
