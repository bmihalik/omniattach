use crate::{Asset, AssetKind, Error, Location, Manifest, Result, Source};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct Limits {
    pub input_bytes: u64,
    /// Post-process accepted output total, NOT a live disk quota.
    pub artifact_bytes: u64,
    pub artifact_count: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 512 * 1024 * 1024,
            artifact_bytes: 512 * 1024 * 1024,
            artifact_count: 10_000,
        }
    }
}

pub struct Workspace {
    directory: TempDir,
    pub input: PathBuf,
    pub source: Source,
}
impl Workspace {
    pub fn new(source: &Path, parent: Option<&Path>, limits: &Limits) -> Result<Self> {
        if !std::fs::metadata(source)?.is_file() {
            return Err(Error::InvalidInput("source is not a regular file".into()));
        }
        let directory = if let Some(parent) = parent {
            tempfile::Builder::new()
                .prefix("omniattach-")
                .tempdir_in(parent.canonicalize()?)?
        } else {
            tempfile::Builder::new().prefix("omniattach-").tempdir()?
        };
        let mut reader = File::open(source)?;
        if !reader.metadata()?.is_file() {
            return Err(Error::InvalidInput("source is not a regular file".into()));
        }
        let extension = source
            .extension()
            .and_then(|v| v.to_str())
            .filter(|s| s.len() <= 16 && s.bytes().all(|c| c.is_ascii_alphanumeric()));
        let input = directory.path().join(
            extension
                .map(|e| format!("input.{e}"))
                .unwrap_or_else(|| "input".into()),
        );
        let mut writer = File::create(&input)?;
        let mut sha = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes.saturating_add(count as u64);
            if bytes > limits.input_bytes {
                return Err(Error::Limit("input bytes".into()));
            }
            sha.update(&buffer[..count]);
            writer.write_all(&buffer[..count])?;
        }
        let source = Source {
            name: source.to_string_lossy().into_owned(),
            bytes,
            sha256: format!("{:x}", sha.finalize()),
        };
        Ok(Self {
            directory,
            input,
            source,
        })
    }
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn read(&self, path: &str, limits: &Limits) -> Result<Vec<u8>> {
        read_bounded(&checked_file(self.root(), path)?, limits.artifact_bytes)
    }
    pub fn asset(
        &self,
        path: &str,
        media_type: &str,
        kind: AssetKind,
        location: Location,
        limits: &Limits,
    ) -> Result<Asset> {
        let file = checked_file(self.root(), path)?;
        let bytes = fs::metadata(file)?.len();
        if bytes == 0 {
            return Err(Error::InvalidOutput(format!("empty artifact {path}")));
        }
        if bytes > limits.artifact_bytes {
            return Err(Error::Limit("artifact bytes".into()));
        }
        Ok(Asset {
            path: path.into(),
            media_type: media_type.into(),
            kind,
            bytes,
            location,
        })
    }
    pub fn finish(self, manifest: Manifest, limits: &Limits) -> Result<ArtifactBundle> {
        if manifest.assets.len() > limits.artifact_count {
            return Err(Error::Limit("artifact count".into()));
        }
        let mut names = std::collections::HashSet::new();
        for asset in &manifest.assets {
            if asset.path == "manifest.json" || !names.insert(asset.path.clone()) {
                return Err(Error::InvalidOutput(
                    "reserved or duplicate artifact path".into(),
                ));
            }
            let path = checked_file(self.root(), &asset.path)?;
            if std::fs::metadata(path)?.len() != asset.bytes {
                return Err(Error::InvalidOutput(
                    "declared artifact length mismatch".into(),
                ));
            }
        }
        let total = manifest.assets.iter().try_fold(0u64, |sum, a| {
            sum.checked_add(a.bytes)
                .ok_or_else(|| Error::Limit("artifact size overflow".into()))
        })?;
        if total > limits.artifact_bytes {
            return Err(Error::Limit("artifact total bytes".into()));
        }
        Ok(ArtifactBundle {
            manifest,
            directory: self.directory,
        })
    }
}
pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Limit("artifact read bytes".into()));
    }
    Ok(bytes)
}
pub(crate) fn checked_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidOutput(
            "artifact path must be relative without traversal".into(),
        ));
    }
    let mut result = root.to_path_buf();
    for component in path.components() {
        result.push(component);
        let meta = fs::symlink_metadata(&result)?;
        if meta.file_type().is_symlink() {
            return Err(Error::InvalidOutput("symlink artifact rejected".into()));
        }
    }
    if !result.is_file() || !result.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(Error::InvalidOutput(
            "artifact is not a contained regular file".into(),
        ));
    }
    Ok(result)
}

/// Owns scratch files. Asset paths stay valid until this bundle is dropped.
#[derive(Debug)]
pub struct ArtifactBundle {
    pub manifest: Manifest,
    directory: TempDir,
}
impl ArtifactBundle {
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn asset_path(&self, index: usize) -> Result<PathBuf> {
        let asset = self
            .manifest
            .assets
            .get(index)
            .ok_or_else(|| Error::InvalidInput("asset index".into()))?;
        checked_file(self.root(), &asset.path)
    }
    /// Creates a new directory, copies only declared assets, then writes manifest.json.
    /// Refuses an existing destination. Not an atomic directory transaction.
    pub fn export(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = destination.as_ref();
        fs::create_dir(destination)?;
        let result = (|| {
            for (index, asset) in self.manifest.assets.iter().enumerate() {
                if asset.path == "manifest.json" {
                    return Err(Error::InvalidOutput("reserved artifact name".into()));
                }
                let source = self.asset_path(index)?;
                let output = destination.join(&asset.path);
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut writer = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)?;
                let mut reader = File::open(source)?;
                std::io::copy(&mut reader, &mut writer)?;
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
