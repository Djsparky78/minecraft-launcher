use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
#[derive(Deserialize, Serialize)]
struct Manifest {
    versions: Vec<Version>,
}
#[derive(Deserialize, Serialize)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    kind: String,
    pub url: String,
}
#[derive(Serialize)]
pub struct VersionList {
    versions: Vec<Version>,
    cached: bool,
    warning: Option<String>,
}
fn parse(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !manifest
        .versions
        .iter()
        .any(|v| v.kind == "release" && !v.id.is_empty())
    {
        return Err("Manifest contains no release versions".into());
    }
    Ok(manifest)
}
fn releases(manifest: Manifest, cached: bool, warning: Option<String>) -> VersionList {
    VersionList {
        versions: manifest
            .versions
            .into_iter()
            .filter(|v| v.kind == "release" && !v.id.is_empty())
            .collect(),
        cached,
        warning,
    }
}
pub async fn load(cache_dir: &Path) -> Result<VersionList, String> {
    load_from(cache_dir, MANIFEST_URL).await
}

async fn load_from(cache_dir: &Path, url: &str) -> Result<VersionList, String> {
    let cache = cache_dir.join("version-manifest.json");
    let fetched = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let text = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let manifest = parse(&text)?;
        Ok::<_, String>((manifest, text))
    }
    .await;
    match fetched {
        Ok((manifest, text)) => {
            // Validate before touching the last known good cache. Persist via a temporary file.
            let saved = (|| -> std::io::Result<()> {
                std::fs::create_dir_all(cache_dir)?;
                let mut temp = tempfile::NamedTempFile::new_in(cache_dir)?;
                std::io::Write::write_all(&mut temp, text.as_bytes())?;
                temp.persist(&cache).map_err(|e| e.error)?;
                Ok(())
            })();
            Ok(releases(
                manifest,
                false,
                saved
                    .err()
                    .map(|e| format!("Versions loaded, but could not save offline cache: {e}")),
            ))
        }
        Err(error) => {
            let manifest = std::fs::read_to_string(cache).map_err(|_| {
                format!("Could not contact Mojang and no readable cache is available: {error}")
            })?;
            let manifest = parse(&manifest).map_err(|_| {
                "Mojang is unavailable and the local cache is invalid. Reconnect and retry."
                    .to_string()
            })?;
            Ok(releases(
                manifest,
                true,
                Some("Mojang is unavailable. Showing previously saved versions.".into()),
            ))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_snapshots_and_rejects_invalid_data() {
        let text = r#"{"versions":[{"id":"1.21","type":"release","url":"https://example.com"},{"id":"snapshot","type":"snapshot","url":"https://example.com"}]}"#;
        let list = releases(parse(text).unwrap(), false, None);
        assert_eq!(list.versions.len(), 1);
        assert_eq!(list.versions[0].id, "1.21");
        assert!(parse(r#"{"versions":[]}"#).is_err());
        assert!(parse("invalid").is_err());
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[tokio::test]
    async fn unavailable_server_uses_cache_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("version-manifest.json");
        let offline = "http://127.0.0.1:0";
        assert!(load_from(dir.path(), offline).await.is_err());
        std::fs::write(
            &path,
            r#"{"versions":[{"id":"1.21","type":"release","url":"https://example.com"}]}"#,
        )
        .unwrap();
        let result = load_from(dir.path(), offline).await.unwrap();
        assert!(result.cached);
        assert_eq!(result.versions[0].id, "1.21");
        std::fs::write(path, "invalid").unwrap();
        assert!(load_from(dir.path(), offline).await.is_err());
    }
}
