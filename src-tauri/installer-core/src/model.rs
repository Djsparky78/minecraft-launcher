use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Download {
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    #[serde(default)]
    pub path: String,
}
#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub downloads: HashMap<String, Download>,
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    #[serde(default)]
    pub logging: HashMap<String, Logging>,
}
#[derive(Debug, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(flatten)]
    pub download: Download,
}
#[derive(Debug, Deserialize)]
pub struct Logging {
    pub file: LoggingFile,
}
#[derive(Debug, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    #[serde(flatten)]
    pub download: Download,
}
#[derive(Debug, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: LibraryDownloads,
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub extract: Extract,
}
#[derive(Debug, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Download>,
    #[serde(default)]
    pub classifiers: HashMap<String, Download>,
}
#[derive(Default, Debug, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}
#[derive(Debug, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}
#[derive(Debug, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Platform {
    pub arch: String,
    pub version: String,
}
impl Platform {
    pub fn windows(arch: &str, version: &str) -> Self {
        Self {
            arch: arch.into(),
            version: version.into(),
        }
    }
    fn classifier(&self) -> &str {
        match self.arch.as_str() {
            "x86" => "natives-windows-x86",
            "aarch64" => "natives-windows-arm64",
            _ => "natives-windows",
        }
    }
}
fn matches(pattern: &str, value: &str) -> Result<bool> {
    Ok(regex::Regex::new(pattern)
        .map_err(|e| Error::message(format!("Invalid Mojang rule: {e}")))?
        .is_match(value))
}
pub fn allowed(rules: Option<&[Rule]>, platform: &Platform) -> Result<bool> {
    let Some(rules) = rules else { return Ok(true) };
    let mut result = false;
    for rule in rules {
        if rule.action != "allow" && rule.action != "disallow" {
            return Err(Error::message("Unknown library rule action"));
        }
        // Vanilla installation has all optional launcher features disabled.
        if rule.features.values().any(|expected| *expected) {
            continue;
        }
        if let Some(os) = &rule.os {
            if os.name.as_deref().is_some_and(|name| name != "windows") {
                continue;
            }
            if let Some(arch) = &os.arch {
                if !matches(arch, &platform.arch)? {
                    continue;
                }
            }
            if let Some(version) = &os.version {
                if !matches(version, &platform.version)? {
                    continue;
                }
            }
        }
        result = rule.action == "allow";
    }
    Ok(result)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
    pub path: String,
    pub stage: String,
    pub download: Download,
}
#[derive(Debug)]
pub struct Native {
    pub archive: String,
    pub exclude: Vec<String>,
}
#[derive(Debug)]
pub struct Plan {
    pub tasks: Vec<Task>,
    pub natives: Vec<Native>,
}

pub fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 120
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        || id == "."
        || id == ".."
    {
        return Err(Error::message("Invalid version or index ID"));
    }
    safe_relative(id)?;
    Ok(())
}
pub fn safe_relative(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || path.starts_with('/')
        || path.chars().any(|c| c.is_control())
    {
        return Err(Error::message(format!("Unsafe metadata path: {path}")));
    }
    for segment in path.split('/') {
        let stem = segment.split('.').next().unwrap_or("").to_ascii_uppercase();
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.ends_with(['.', ' '])
            || segment.contains(['<', '>', '"', '|', '?', '*'])
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err(Error::message(format!("Unsafe metadata path: {path}")));
        }
    }
    Ok(())
}
pub fn task(path: String, stage: &str, download: Download) -> Result<Task> {
    safe_relative(&path)?;
    if let Some(hash) = &download.sha1 {
        validate_hash(hash)?;
    }
    Ok(Task {
        path,
        stage: stage.into(),
        download,
    })
}
pub fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::message("Invalid SHA-1 in metadata"));
    }
    Ok(())
}
pub fn plan(metadata: &Metadata, platform: &Platform) -> Result<Plan> {
    valid_id(&metadata.id)?;
    valid_id(&metadata.asset_index.id)?;
    let mut tasks = vec![task(
        format!("versions/{0}/{0}.jar", metadata.id),
        "Client",
        metadata
            .downloads
            .get("client")
            .ok_or_else(|| Error::message("Version has no client download"))?
            .clone(),
    )?];
    let mut natives = Vec::new();
    for library in &metadata.libraries {
        if !allowed(library.rules.as_deref(), platform)? {
            continue;
        }
        let classifier = library.name.split(':').nth(3);
        if classifier.is_some_and(|c| c.starts_with("natives-") && c != platform.classifier()) {
            continue;
        }
        if let Some(artifact) = &library.downloads.artifact {
            let path = format!("libraries/{}", artifact.path);
            tasks.push(task(path.clone(), "Libraries", artifact.clone())?);
            if classifier.is_some_and(|c| c.starts_with("natives-windows")) {
                natives.push(Native {
                    archive: path,
                    exclude: library.extract.exclude.clone(),
                });
            }
        }
        if let Some(classifier) = library.natives.get("windows") {
            let classifier =
                classifier.replace("${arch}", if platform.arch == "x86" { "32" } else { "64" });
            if platform.arch == "aarch64" && !classifier.contains("arm64") {
                return Err(Error::message("This version has no declared Windows ARM64 natives; use the x64 launcher with an x64 Java runtime."));
            }
            let native = library
                .downloads
                .classifiers
                .get(&classifier)
                .ok_or_else(|| {
                    Error::message(format!(
                        "Missing native classifier {classifier} for {}",
                        library.name
                    ))
                })?;
            let path = format!("libraries/{}", native.path);
            tasks.push(task(path.clone(), "Natives", native.clone())?);
            natives.push(Native {
                archive: path,
                exclude: library.extract.exclude.clone(),
            });
        }
    }
    if let Some(logging) = metadata.logging.get("client") {
        valid_id(&logging.file.id)?;
        tasks.push(task(
            format!("assets/log_configs/{}", logging.file.id),
            "Logging",
            logging.file.download.clone(),
        )?);
    }
    let mut unique = BTreeMap::new();
    for task in tasks {
        if let Some(old) = unique.insert(task.path.clone(), task.clone()) {
            if old.download.sha1 != task.download.sha1 || old.download.size != task.download.size {
                return Err(Error::message("Conflicting library downloads"));
            }
        }
    }
    Ok(Plan {
        tasks: unique.into_values().collect(),
        natives,
    })
}
#[derive(Deserialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, Asset>,
    #[serde(default, rename = "virtual")]
    pub virtual_assets: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}
#[derive(Deserialize)]
pub struct Asset {
    pub hash: String,
    pub size: u64,
}
pub fn asset_tasks(index: &AssetIndex) -> Result<Vec<Task>> {
    let mut unique = BTreeMap::new();
    for asset in index.objects.values() {
        validate_hash(&asset.hash)?;
        let path = format!("assets/objects/{}/{}", &asset.hash[..2], asset.hash);
        unique.insert(
            path.clone(),
            task(
                path,
                "Assets",
                Download {
                    url: format!(
                        "https://resources.download.minecraft.net/{}/{}",
                        &asset.hash[..2],
                        asset.hash
                    ),
                    sha1: Some(asset.hash.clone()),
                    size: Some(asset.size),
                    path: String::new(),
                },
            )?,
        );
    }
    Ok(unique.into_values().collect())
}
