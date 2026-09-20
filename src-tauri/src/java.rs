use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
#[derive(Serialize)]
pub struct JavaInstallation {
    path: String,
    version: String,
    major_version: u32,
    vendor: Option<String>,
}
fn parse_version(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with("java version \"") || line.starts_with("openjdk version \"") {
            line.split('"')
                .nth(1)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        } else {
            None
        }
    })
}
fn major_version(version: &str) -> Option<u32> {
    let normalized = version.strip_prefix("1.").unwrap_or(version);
    normalized
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
        .filter(|v| *v > 0)
}
fn property(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        (key.trim() == name && !value.trim().is_empty()).then(|| value.trim().to_string())
    })
}
// Independent of discovery so a future manual path can use the same validation.
pub async fn inspect(path: &Path) -> Option<JavaInstallation> {
    let path = path.canonicalize().ok()?;
    if !path.is_file() {
        return None;
    }
    let mut command = tokio::process::Command::new(&path);
    command
        .args(["-XshowSettings:properties", "-version"])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(3), command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let version = property(&text, "java.version").or_else(|| parse_version(&text))?;
    Some(JavaInstallation {
        path: path.to_string_lossy().into_owned(),
        major_version: major_version(&version)?,
        vendor: property(&text, "java.vendor"),
        version,
    })
}
fn add_root(root: &Path, candidates: &mut Vec<PathBuf>) {
    candidates.push(root.join("bin/java.exe"));
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            candidates.push(entry.path().join("bin/java.exe"));
        }
    }
}
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        paths.push(PathBuf::from(home).join("bin").join(executable));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path).filter(|p| p.is_absolute()) {
            paths.push(dir.join(executable));
        }
    }
    if cfg!(windows) {
        for env in [
            "ProgramFiles",
            "ProgramW6432",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
        ] {
            if let Some(base) = std::env::var_os(env) {
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "AdoptOpenJDK",
                    "Microsoft",
                    "Amazon Corretto",
                    "Zulu",
                    "BellSoft",
                    "Programs/Eclipse Adoptium",
                ] {
                    add_root(&PathBuf::from(&base).join(vendor), &mut paths);
                }
            }
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            add_root(&PathBuf::from(home).join(".jdks"), &mut paths);
        }
    }
    paths
}
pub async fn detect() -> Vec<JavaInstallation> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for candidate in candidates() {
        let Ok(path) = candidate.canonicalize() else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        let key = if cfg!(windows) {
            key.to_lowercase()
        } else {
            key
        };
        if !seen.insert(key) {
            continue;
        }
        if let Some(installation) = inspect(&path).await {
            found.push(installation);
        }
    }
    found
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_legacy_and_modern_java() {
        assert_eq!(major_version("1.8.0_421"), Some(8));
        assert_eq!(major_version("21.0.4"), Some(21));
        assert_eq!(major_version("25-ea"), Some(25));
        assert_eq!(major_version("bad"), None);
        assert_eq!(
            property("    java.vendor = Eclipse Adoptium", "java.vendor"),
            Some("Eclipse Adoptium".into())
        );
        assert_eq!(
            parse_version("java version \"1.8.0_421\""),
            Some("1.8.0_421".into())
        );
        assert_eq!(
            parse_version("openjdk version \"21.0.4\" 2024-07-16"),
            Some("21.0.4".into())
        );
        assert_eq!(parse_version("not java"), None);
    }
}
