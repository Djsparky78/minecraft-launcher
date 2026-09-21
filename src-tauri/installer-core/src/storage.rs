use crate::{
    check_cancel,
    model::{safe_relative, Task},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerifiedFile {
    pub path: String,
    pub sha1: String,
    pub size: u64,
}
pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    safe_relative(relative)?;
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::message(format!(
                    "Refusing linked game path: {}",
                    path.display()
                )))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(Error::message(format!(
                    "Cannot inspect {}: {e}",
                    path.display()
                )))
            }
        }
    }
    Ok(path)
}
pub fn fingerprint(path: &Path, cancel: &AtomicBool) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut sha = Sha1::new();
    let mut size = 0;
    let mut buffer = [0; 65536];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((format!("{:x}", sha.finalize()), size))
}
pub fn verify(root: &Path, task: &Task, cancel: &AtomicBool) -> Result<Option<VerifiedFile>> {
    let path = safe_join(root, &task.path)?;
    match fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() {
                return Err(Error::message(format!(
                    "Expected a file at {}",
                    path.display()
                )));
            }
            if task.download.size.is_some_and(|size| size != meta.len()) {
                return Ok(None);
            }
        }
    }
    // A file with neither a trusted hash nor size cannot be considered reusable.
    if task.download.sha1.is_none() && task.download.size.is_none() {
        return Ok(None);
    }
    let (hash, size) = fingerprint(&path, cancel)?;
    if task
        .download
        .sha1
        .as_ref()
        .is_some_and(|expected| !hash.eq_ignore_ascii_case(expected))
    {
        return Ok(None);
    }
    Ok(Some(VerifiedFile {
        path: task.path.clone(),
        sha1: hash,
        size,
    }))
}
pub fn atomic_json<T: Serialize>(root: &Path, relative: &str, value: &T) -> Result<()> {
    let path = safe_join(root, relative)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("Missing parent directory"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, value)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| Error::from(e.error))?;
    Ok(())
}
pub struct Downloader {
    client: reqwest::blocking::Client,
}
impl Downloader {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("EmberLauncher/0.1 vanilla-installer")
                .build()?,
        })
    }
    pub fn ensure(
        &self,
        root: &Path,
        task: &Task,
        staging: &Path,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<VerifiedFile> {
        check_cancel(cancel)?;
        if let Some(file) = verify(root, task, cancel)? {
            progress(file.size);
            return Ok(file);
        }
        let url =
            reqwest::Url::parse(&task.download.url).map_err(|e| Error::message(e.to_string()))?;
        let trusted = url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some(
                    "piston-meta.mojang.com"
                        | "piston-data.mojang.com"
                        | "launcher.mojang.com"
                        | "launchermeta.mojang.com"
                        | "libraries.minecraft.net"
                        | "resources.download.minecraft.net"
                )
            );
        #[cfg(test)]
        let trusted = trusted || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
        if !trusted {
            return Err(Error::message(format!(
                "Untrusted download URL: {}",
                task.download.url
            )));
        }
        let path = safe_join(root, &task.path)?;
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| Error::message("Missing file parent"))?,
        )?;
        if let Some(size) = task.download.size {
            if fs2::available_space(root)? < size {
                return Err(Error::message(format!(
                    "Not enough disk space for {} (needs {size} bytes)",
                    task.path
                )));
            }
        }
        let mut response = self.client.get(url).send()?.error_for_status()?;
        let mut temp = tempfile::NamedTempFile::new_in(staging)?;
        let mut hash = Sha1::new();
        let mut size = 0;
        let mut buffer = [0; 65536];
        loop {
            check_cancel(cancel)?;
            let count = response.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size += count as u64;
            if task.download.size.is_some_and(|expected| size > expected) {
                return Err(Error::message(format!(
                    "Download is larger than expected: {}",
                    task.path
                )));
            }
            hash.update(&buffer[..count]);
            temp.write_all(&buffer[..count])?;
            progress(size);
        }
        check_cancel(cancel)?;
        let hash = format!("{:x}", hash.finalize());
        if task
            .download
            .sha1
            .as_ref()
            .is_some_and(|expected| !hash.eq_ignore_ascii_case(expected))
            || task.download.size.is_some_and(|expected| size != expected)
        {
            return Err(Error::message(format!(
                "Integrity check failed for {}. Retry to download it again.",
                task.path
            )));
        }
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| Error::from(e.error))?;
        Ok(VerifiedFile {
            path: task.path.clone(),
            sha1: hash,
            size,
        })
    }
}
pub fn materialize(
    root: &Path,
    source: &VerifiedFile,
    relative: &str,
    staging: &Path,
    cancel: &AtomicBool,
) -> Result<VerifiedFile> {
    let task = Task {
        path: relative.into(),
        stage: "Legacy assets".into(),
        download: crate::model::Download {
            url: String::new(),
            sha1: Some(source.sha1.clone()),
            size: Some(source.size),
            path: String::new(),
        },
    };
    if let Some(file) = verify(root, &task, cancel)? {
        return Ok(file);
    }
    let dest = safe_join(root, relative)?;
    fs::create_dir_all(
        dest.parent()
            .ok_or_else(|| Error::message("Missing parent"))?,
    )?;
    let mut input = File::open(safe_join(root, &source.path)?)?;
    let mut temp = tempfile::NamedTempFile::new_in(staging)?;
    let mut buffer = [0; 65536];
    loop {
        check_cancel(cancel)?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        temp.write_all(&buffer[..n])?;
    }
    temp.as_file().sync_all()?;
    temp.persist(dest).map_err(|e| Error::from(e.error))?;
    Ok(VerifiedFile {
        path: relative.into(),
        sha1: source.sha1.clone(),
        size: source.size,
    })
}
