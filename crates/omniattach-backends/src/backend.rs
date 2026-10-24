use crate::{
    ArtifactBundle, CommandSpec, Error, Invocation, Limits, Manifest, ProcessOutput, Result,
    RunOptions, Runner,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub struct BackendContext<'a> {
    pub runner: &'a dyn Runner,
    pub run_options: RunOptions,
    pub limits: Limits,
    pub scratch_parent: Option<PathBuf>,
    pub diagnostic_tail_bytes: usize,
}
impl<'a> BackendContext<'a> {
    pub fn new(runner: &'a dyn Runner) -> Self {
        Self {
            runner,
            run_options: RunOptions::default(),
            limits: Limits::default(),
            scratch_parent: None,
            diagnostic_tail_bytes: 4096,
        }
    }
    pub(crate) fn run(&self, spec: &CommandSpec, manifest: &mut Manifest) -> Result<ProcessOutput> {
        let output = self.runner.run(spec, &self.run_options)?;
        manifest.invocations.push(Invocation {
            program: spec.program.to_string_lossy().into_owned(),
            elapsed_ms: output.elapsed.as_millis().min(u64::MAX as u128) as u64,
            stdout_bytes: output.stdout.len(),
            stderr_bytes: output.stderr.len(),
            stderr_tail: crate::process::tail(&output.stderr, self.diagnostic_tail_bytes),
        });
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    PdfText,
    Ocr,
    OfficeConversion,
    Audio,
    Transcription,
    VideoFrames,
    MediaProbe,
    MarkdownConversion,
    FileConversion,
}

/// Backend-specific configuration is held by the concrete backend instance.
pub trait Backend: Send + Sync {
    fn id(&self) -> &str;
    fn capability(&self) -> Capability;
    fn convert(&self, source: &Path, context: &BackendContext<'_>) -> Result<ArtifactBundle>;
}
#[derive(Default)]
pub struct Registry {
    backends: BTreeMap<String, Box<dyn Backend>>,
}
impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, backend: impl Backend + 'static) -> Result<()> {
        if self.backends.contains_key(backend.id()) {
            return Err(Error::Configuration(format!(
                "duplicate backend {}",
                backend.id()
            )));
        }
        self.backends.insert(backend.id().into(), Box::new(backend));
        Ok(())
    }
    pub fn capabilities(&self) -> Vec<(&str, Capability)> {
        self.backends
            .values()
            .map(|b| (b.id(), b.capability()))
            .collect()
    }
    /// Explicit selection only. Failures never silently invoke another backend.
    pub fn convert(
        &self,
        id: &str,
        source: impl AsRef<Path>,
        context: &BackendContext<'_>,
    ) -> Result<ArtifactBundle> {
        self.backends
            .get(id)
            .ok_or_else(|| Error::UnknownBackend(id.into()))?
            .convert(source.as_ref(), context)
    }
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub program: PathBuf,
    pub env: BTreeMap<std::ffi::OsString, std::ffi::OsString>,
}
impl Tool {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            env: BTreeMap::new(),
        }
    }
    pub fn command(&self, cwd: &Path) -> CommandSpec {
        let mut spec = CommandSpec::new(&self.program, cwd);
        spec.env = self.env.clone();
        spec
    }
    /// Probe explicitly: tool availability is never checked by loading/registering a backend.
    pub fn probe(&self, args: &[&str], context: &BackendContext<'_>) -> Result<ProcessOutput> {
        let temp = tempfile::tempdir()?;
        context.runner.run(
            &self.command(temp.path()).args(args.iter().copied()),
            &context.run_options,
        )
    }
}
