//! Explicit model provisioning. Conversion never calls this module implicitly.
use crate::backends::{CancellationToken, CommandSpec, ProcessRunner, RunOptions, Runner};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub max_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSpec {
    pub id: String,
    pub revision: String,
    pub files: Vec<ModelFile>,
}
#[derive(Debug, Clone)]
pub struct ProvisionOptions {
    pub allow_network: bool,
    pub allow_external: bool,
    pub timeout: Duration,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub cancellation: CancellationToken,
}
impl Default for ProvisionOptions {
    fn default() -> Self {
        Self {
            allow_network: false,
            allow_external: false,
            timeout: Duration::from_secs(600),
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            max_total_bytes: 4 * 1024 * 1024 * 1024,
            cancellation: CancellationToken::default(),
        }
    }
}
pub struct FetchContext<'a> {
    pub options: &'a ProvisionOptions,
    pub remaining: Duration,
}
/// Fetchers write only the requested file and must honor the supplied size/deadline.
/// The store independently checks regular-file type, accepted size and SHA-256.
pub trait ModelFetcher {
    fn fetch(&self, file: &ModelFile, destination: &Path, context: &FetchContext<'_>)
    -> Result<()>;
}
/// Explicit HTTPS transport using installed curl and the existing bounded runner.
/// No curl/model/runtime installation is performed.
pub struct CurlModelFetcher {
    pub program: PathBuf,
}
impl Default for CurlModelFetcher {
    fn default() -> Self {
        Self {
            program: "curl".into(),
        }
    }
}
impl ModelFetcher for CurlModelFetcher {
    fn fetch(
        &self,
        file: &ModelFile,
        destination: &Path,
        context: &FetchContext<'_>,
    ) -> Result<()> {
        if !context.options.allow_network || !context.options.allow_external {
            return Err(Error::ExternalDisabled);
        }
        let spec = CommandSpec::new(
            &self.program,
            destination
                .parent()
                .ok_or_else(|| Error::Configuration("missing download parent".into()))?,
        )
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--retry",
            "0",
        ])
        .arg("--max-filesize")
        .arg(file.max_bytes.to_string())
        .arg("--max-time")
        .arg(format!("{:.3}", context.remaining.as_secs_f64()))
        .arg("--output")
        .arg(destination.as_os_str())
        .arg("--url")
        .arg(&file.url);
        let mut options = RunOptions::default();
        options.timeout = context.remaining;
        options.max_stdout_bytes = 4096;
        options.max_stderr_bytes = 8192;
        options.cancellation = context.options.cancellation.clone();
        ProcessRunner.run(&spec, &options).map_err(|e| match e {
            crate::backends::Error::Cancelled => Error::Cancelled,
            crate::backends::Error::Timeout(_) => Error::Deadline,
            crate::backends::Error::Limit(s) => Error::Limit(s),
            other => Error::Backend(other),
        })?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct InstalledModel {
    directory: PathBuf,
    spec: ModelSpec,
}
impl InstalledModel {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn spec(&self) -> &ModelSpec {
        &self.spec
    }
    pub fn file(&self, name: &str) -> Result<PathBuf> {
        if !self.spec.files.iter().any(|f| f.name == name) {
            return Err(Error::Configuration(
                "file is not in verified model specification".into(),
            ));
        }
        Ok(self.directory.join(name))
    }
    /// Runtime and worker are supplied locally, never installed/downloaded here.
    #[cfg(feature = "documents")]
    pub fn equation_worker_config_json(
        &self,
        worker: &Path,
        runtime: &Path,
        max_tokens: usize,
    ) -> Result<serde_json::Value> {
        if !(1..=512).contains(&max_tokens) {
            return Err(Error::Configuration("max_tokens must be 1..=512".into()));
        }
        let options = ProvisionOptions::default();
        let start = Instant::now();
        let worker = worker.canonicalize()?;
        let runtime = runtime.canonicalize()?;
        let mut artifacts = Vec::new();
        for path in [&worker, &runtime] {
            let (sha256, _) = hash(path, u64::MAX, &options, start)?;
            artifacts.push(serde_json::json!({"path":path,"sha256":sha256}));
        }
        for file in &self.spec.files {
            let path = self.file(&file.name)?;
            verify_file(&path, file, &options, start)?;
            artifacts.push(serde_json::json!({"path":path,"sha256":file.sha256}));
        }
        Ok(
            serde_json::json!({"argv":[worker,self.file("encoder_model.onnx")?,self.file("decoder_model_merged.onnx")?,self.file("tokenizer.json")?,max_tokens.to_string()],
            "env":{"ORT_DYLIB_PATH":runtime},"timeout_ms":600000,"artifacts":artifacts}),
        )
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelStatus {
    Missing,
    Ready { model: InstalledModel },
    Invalid { reason: String },
}
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: String,
    spec: ModelSpec,
}
struct Lock {
    path: PathBuf,
    file: Option<fs::File>,
}
impl Drop for Lock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}
fn safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn no_link(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(Error::InvalidInput("symlink in model store path".into()));
        }
    }
    Ok(())
}
fn check(options: &ProvisionOptions, start: Instant) -> Result<Duration> {
    if options.cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    options
        .timeout
        .checked_sub(start.elapsed())
        .filter(|d| !d.is_zero())
        .ok_or(Error::Deadline)
}
fn validate(spec: &ModelSpec, options: &ProvisionOptions) -> Result<()> {
    if !safe(&spec.id)
        || spec.revision.len() != 40
        || !spec.revision.bytes().all(|b| b.is_ascii_hexdigit())
        || spec.files.is_empty()
        || spec.files.len() > 16
    {
        return Err(Error::Configuration(
            "model ID, full 40-hex revision and 1..=16 files required".into(),
        ));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in &spec.files {
        let authority = file
            .url
            .strip_prefix("https://")
            .and_then(|s| s.split('/').next())
            .unwrap_or("");
        if !safe(&file.name)
            || file.name.eq_ignore_ascii_case("model-installation.json")
            || !names.insert(file.name.to_ascii_lowercase())
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || file.url.len() > 8192
            || authority.is_empty()
            || authority.contains(['@', '?', '#'])
            || file
                .url
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || file.max_bytes == 0
            || file.max_bytes > options.max_file_bytes
        {
            return Err(Error::Configuration(
                "invalid HTTPS model file, checksum or byte limit".into(),
            ));
        }
        total = total
            .checked_add(file.max_bytes)
            .ok_or_else(|| Error::Limit("model declared bytes".into()))?;
    }
    if total > options.max_total_bytes {
        return Err(Error::Limit("model declared total bytes".into()));
    }
    Ok(())
}
fn hash(
    path: &Path,
    limit: u64,
    options: &ProvisionOptions,
    start: Instant,
) -> Result<(String, u64)> {
    no_link(path)?;
    let mut reader = fs::File::open(path)?;
    if !reader.metadata()?.is_file() {
        return Err(Error::InvalidInput(
            "model artifact is not a regular file".into(),
        ));
    }
    let mut digest = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(options, start)?;
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        size = size
            .checked_add(n as u64)
            .ok_or_else(|| Error::Limit("model file size overflow".into()))?;
        if size > limit {
            return Err(Error::Limit("model file bytes".into()));
        }
        digest.update(&buffer[..n]);
    }
    Ok((format!("{:x}", digest.finalize()), size))
}
fn verify_file(
    path: &Path,
    file: &ModelFile,
    options: &ProvisionOptions,
    start: Instant,
) -> Result<()> {
    let (actual, _) = hash(path, file.max_bytes, options, start)?;
    if actual != file.sha256.to_ascii_lowercase() {
        return Err(Error::InvalidOutput(format!(
            "model checksum mismatch: {}",
            file.name
        )));
    }
    Ok(())
}
impl ModelStore {
    /// Does not touch disk or network. No hardcoded user cache directory.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        Ok(Self {
            root: if root.is_absolute() {
                root
            } else {
                std::env::current_dir()?.join(root)
            },
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn destination(&self, spec: &ModelSpec) -> PathBuf {
        self.root.join(&spec.id).join(&spec.revision)
    }
    fn verify_at(
        &self,
        spec: &ModelSpec,
        options: &ProvisionOptions,
        start: Instant,
    ) -> Result<InstalledModel> {
        let directory = self.destination(spec);
        no_link(&self.root)?;
        no_link(&self.root.join(&spec.id))?;
        no_link(&directory)?;
        let marker = directory.join("model-installation.json");
        no_link(&marker)?;
        let mut raw = Vec::new();
        fs::File::open(marker)?
            .take(1_048_577)
            .read_to_end(&mut raw)?;
        if raw.len() > 1_048_576 {
            return Err(Error::Limit("model receipt bytes".into()));
        }
        let receipt: Receipt = serde_json::from_slice(&raw)?;
        if receipt.schema_version != "omniattach/model-installation/0.1" || receipt.spec != *spec {
            return Err(Error::InvalidOutput(
                "installed model specification differs".into(),
            ));
        }
        for file in &spec.files {
            verify_file(&directory.join(&file.name), file, options, start)?;
        }
        check(options, start)?;
        Ok(InstalledModel {
            directory: directory.canonicalize()?,
            spec: spec.clone(),
        })
    }
    pub fn verify(&self, spec: &ModelSpec, options: &ProvisionOptions) -> Result<InstalledModel> {
        validate(spec, options)?;
        self.verify_at(spec, options, Instant::now())
    }
    /// Offline inspection. Missing/corrupt/incomplete installations are not Ready.
    pub fn inspect(&self, spec: &ModelSpec, options: &ProvisionOptions) -> Result<ModelStatus> {
        validate(spec, options)?;
        let start = Instant::now();
        check(options, start)?;
        match fs::symlink_metadata(self.destination(spec)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ModelStatus::Missing),
            Err(e) => Err(e.into()),
            Ok(_) => match self.verify_at(spec, options, start) {
                Ok(model) => Ok(ModelStatus::Ready { model }),
                Err(Error::Cancelled) => Err(Error::Cancelled),
                Err(Error::Deadline) => Err(Error::Deadline),
                Err(e) => Ok(ModelStatus::Invalid {
                    reason: e.to_string(),
                }),
            },
        }
    }
    /// Explicit provisioning. A valid cache works offline; existing corrupt or
    /// incomplete revision directories are never silently repaired/replaced.
    pub fn provision(
        &self,
        spec: &ModelSpec,
        options: &ProvisionOptions,
        fetcher: &dyn ModelFetcher,
    ) -> Result<InstalledModel> {
        validate(spec, options)?;
        let start = Instant::now();
        check(options, start)?;
        let destination = self.destination(spec);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return self.verify_at(spec, options, start),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if !options.allow_network {
            return Err(Error::Configuration(
                "model missing; explicit network permission required".into(),
            ));
        }
        no_link(&self.root)?;
        fs::create_dir_all(&self.root)?;
        let parent = self.root.join(&spec.id);
        no_link(&parent)?;
        fs::create_dir_all(&parent)?;
        let lock_path = parent.join(format!(".{}.lock", spec.revision));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        let _lock = Lock {
            path: lock_path,
            file: Some(file),
        };
        if fs::symlink_metadata(&destination).is_ok() {
            return self.verify_at(spec, options, start);
        }
        let stage = tempfile::Builder::new()
            .prefix(".model-download-")
            .tempdir_in(&parent)?;
        for file in &spec.files {
            let remaining = check(options, start)?;
            let path = stage.path().join(&file.name);
            fetcher.fetch(file, &path, &FetchContext { options, remaining })?;
            verify_file(&path, file, options, start)?;
        }
        check(options, start)?;
        // create_dir is no-clobber. Receipt publication, not directory existence,
        // marks availability. An interrupted publication remains visibly invalid.
        fs::create_dir(&destination)?;
        for file in &spec.files {
            check(options, start)?;
            fs::rename(stage.path().join(&file.name), destination.join(&file.name))?;
        }
        let receipt = serde_json::to_vec_pretty(&Receipt {
            schema_version: "omniattach/model-installation/0.1".into(),
            spec: spec.clone(),
        })?;
        if receipt.len() > 1_048_576 {
            return Err(Error::Limit("model receipt bytes".into()));
        }
        let mut marker = tempfile::NamedTempFile::new_in(&destination)?;
        marker.write_all(&receipt)?;
        marker.as_file().sync_all()?;
        check(options, start)?;
        marker
            .persist_noclobber(destination.join("model-installation.json"))
            .map_err(|e| Error::Io(e.error))?;
        self.verify_at(spec, options, start)
    }
}

/// Byte pins from the previously evaluated local TexTeller ONNX profile.
/// Downloading is still explicit; this is not a remote-signature or license check.
pub fn texteller3_onnx_spec() -> Result<ModelSpec> {
    Ok(serde_json::from_str(include_str!(
        "../model-specs/texteller3-onnx.json"
    ))?)
}
