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
        let mut command = tokio::process::Command::new(&path);
        command
            .arg("-version")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(3), command.output()).await
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        if let Some(version) = parse_version(&text) {
            found.push(JavaInstallation {
                path: path.to_string_lossy().into_owned(),
                version,
            });
        }
    }
    found
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_legacy_and_modern_java() {
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
