use crate::{Error, Format, Goal, JobContext, Result, Route};
use serde::{Deserialize, Serialize};
#[cfg(feature = "backends")]
use std::io::{Read, Write};
use std::{
    fs::{self, File},
    path::{Component, Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    /// Backend-owned schema name. No claim of a universal canonical document schema.
    pub schema: String,
    pub value: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Warning {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextFragment {
    pub text: String,
    /// Original converter location schema; null means no original location supplied.
    pub location: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    /// Relative to the bundle artifact directory; never a path outside the bundle.
    pub path: String,
    pub media_type: String,
    pub bytes: u64,
    pub location: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FacadeManifest {
    pub schema_version: String,
    pub route: Route,
    pub source: SourceIdentity,
    pub format: Format,
    pub goal: Goal,
    pub elapsed_ms: u64,
    pub text: Vec<TextFragment>,
    pub payloads: Vec<Payload>,
    pub artifacts: Vec<Artifact>,
    pub warnings: Vec<Warning>,
}
impl FacadeManifest {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Converter declares an already-written file under StagedInput::output_dir.
#[derive(Debug, Clone)]
pub struct PendingArtifact {
    pub path: String,
    pub media_type: String,
    pub location: serde_json::Value,
}
impl PendingArtifact {
    pub fn new(path: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            media_type: media_type.into(),
            location: serde_json::Value::Null,
        }
    }
    pub fn located(mut self, location: serde_json::Value) -> Self {
        self.location = location;
        self
    }
}
#[derive(Debug, Clone, Default)]
pub struct ConversionData {
    pub text: Vec<TextFragment>,
    pub payloads: Vec<Payload>,
    pub artifacts: Vec<PendingArtifact>,
    pub warnings: Vec<Warning>,
}
impl ConversionData {
    pub fn payload(mut self, schema: impl Into<String>, value: impl Serialize) -> Result<Self> {
        self.payloads.push(Payload {
            schema: schema.into(),
            value: serde_json::to_value(value)?,
        });
        Ok(self)
    }
    pub fn warn(&mut self, code: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(Warning {
            code: code.into(),
            message: message.into(),
        });
    }
}

pub(crate) fn relative_file(root: &Path, path: &str) -> Result<PathBuf> {
    if path.is_empty()
        || path.contains('\\')
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidOutput(
            "artifact path must be relative without traversal".into(),
        ));
    }
    let mut file = root.to_path_buf();
    for component in Path::new(path).components() {
        file.push(component);
        if fs::symlink_metadata(&file)?.file_type().is_symlink() {
            return Err(Error::InvalidOutput("symlink artifact".into()));
        }
    }
    if !file.is_file() || !file.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(Error::InvalidOutput(
            "artifact must be a contained regular file".into(),
        ));
    }
    Ok(file)
}
pub(crate) fn validate_artifacts(
    output: &Path,
    pending: &[PendingArtifact],
    ctx: &JobContext,
) -> Result<Vec<Artifact>> {
    if pending.len() > ctx.policy().max_export_files {
        return Err(Error::Limit("artifact count".into()));
    }
    let mut names = std::collections::HashSet::new();
    let mut total = 0u64;
    let mut artifacts = Vec::new();
    for item in pending {
        ctx.check()?;
        let first = Path::new(&item.path).components().next();
        if first == Some(Component::Normal(std::ffi::OsStr::new("manifest.json")))
            || !names.insert(item.path.clone())
        {
            return Err(Error::InvalidOutput(
                "reserved or duplicate artifact path".into(),
            ));
        }
        let file = relative_file(output, &item.path)?;
        let bytes = fs::metadata(file)?.len();
        total = total
            .checked_add(bytes)
            .ok_or_else(|| Error::Limit("artifact byte overflow".into()))?;
        if total > ctx.policy().max_export_bytes {
            return Err(Error::Limit("artifact bytes".into()));
        }
        artifacts.push(Artifact {
            path: item.path.clone(),
            media_type: item.media_type.clone(),
            bytes,
            location: item.location.clone(),
        });
    }
    Ok(artifacts)
}

/// Facade-owned scratch data. Artifacts live until drop or explicit export.
#[derive(Debug)]
pub struct FacadeBundle {
    manifest: FacadeManifest,
    directory: TempDir,
}
impl FacadeBundle {
    pub(crate) fn new(manifest: FacadeManifest, directory: TempDir) -> Self {
        Self {
            manifest,
            directory,
        }
    }
    pub fn manifest(&self) -> &FacadeManifest {
        &self.manifest
    }
    pub fn root(&self) -> PathBuf {
        self.directory.path().join("out")
    }
    pub fn artifact_path(&self, index: usize) -> Result<PathBuf> {
        let artifact = self
            .manifest
            .artifacts
            .get(index)
            .ok_or_else(|| Error::InvalidOutput("artifact index".into()))?;
        relative_file(&self.root(), &artifact.path)
    }
    pub fn payload(&self, schema: &str) -> Option<&serde_json::Value> {
        self.manifest
            .payloads
            .iter()
            .find(|p| p.schema == schema)
            .map(|p| &p.value)
    }
    pub fn plain_text(&self) -> String {
        self.manifest
            .text
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    /// Copies only declared artifacts, then manifest.json, to a NEW directory.
    /// Synchronous, not atomic, not cancellable; failure attempts to remove the partial directory.
    pub fn export(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = destination.as_ref();
        fs::create_dir(destination)?;
        let result = (|| {
            for (index, artifact) in self.manifest.artifacts.iter().enumerate() {
                let source = self.artifact_path(index)?;
                let output = destination.join(&artifact.path);
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut writer = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)?;
                let mut file = File::open(source)?;
                let copied = std::io::copy(&mut file, &mut writer)?;
                if copied != artifact.bytes {
                    return Err(Error::InvalidOutput(
                        "artifact changed after conversion".into(),
                    ));
                }
            }
            fs::write(destination.join("manifest.json"), self.manifest.to_json()?)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(destination);
        }
        result
    }
}

#[cfg(feature = "backends")]
pub(crate) fn copy_checked(
    source: &Path,
    destination: &Path,
    limit: u64,
    context: &JobContext,
) -> Result<u64> {
    let mut reader = File::open(source)?;
    if !reader.metadata()?.is_file() {
        return Err(Error::InvalidInput("not a regular file".into()));
    }
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut buffer = [0; 65536];
    let mut total = 0u64;
    loop {
        context.check()?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > limit {
            return Err(Error::Limit("copy bytes".into()));
        }
        writer.write_all(&buffer[..count])?;
    }
    Ok(total)
}
