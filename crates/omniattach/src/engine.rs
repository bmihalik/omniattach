use crate::*;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Input paths are job-local. original_name is metadata, not a path to use for reading.
#[derive(Debug, Clone)]
pub struct StagedInput {
    pub path: PathBuf,
    pub output_dir: PathBuf,
    pub source: SourceIdentity,
    pub format: Format,
}
pub trait Converter: Send + Sync {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData>;
}
impl<F> Converter for F
where
    F: Fn(&StagedInput, &ConversionRequest, &JobContext) -> Result<ConversionData> + Send + Sync,
{
    fn convert(
        &self,
        i: &StagedInput,
        r: &ConversionRequest,
        c: &JobContext,
    ) -> Result<ConversionData> {
        self(i, r, c)
    }
}
struct Entry {
    route: Route,
    converter: Arc<dyn Converter>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub route: Route,
    pub detection: Detection,
    pub goal: Goal,
    pub warnings: Vec<String>,
}
/// Immutable registry after build. Concurrent calls share converters, not job workspaces.
pub struct OmniAttach {
    entries: BTreeMap<String, Entry>,
}
pub struct OmniAttachBuilder {
    entries: BTreeMap<String, Entry>,
}
impl Default for OmniAttachBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl OmniAttachBuilder {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
    pub fn register(mut self, route: Route, converter: impl Converter + 'static) -> Result<Self> {
        if route.id.trim().is_empty() || route.formats.is_empty() || route.goals.is_empty() {
            return Err(Error::Configuration(
                "nonempty route id, formats and goals required".into(),
            ));
        }
        if self.entries.contains_key(&route.id) {
            return Err(Error::Configuration(format!(
                "duplicate route {}",
                route.id
            )));
        }
        self.entries.insert(
            route.id.clone(),
            Entry {
                route,
                converter: Arc::new(converter),
            },
        );
        Ok(self)
    }
    /// Default is deliberately limited to actually implemented portable text routes.
    pub fn with_portable_defaults(self) -> Result<Self> {
        crate::portable::register(self)
    }
    pub fn build(self) -> OmniAttach {
        OmniAttach {
            entries: self.entries,
        }
    }
}
impl Default for OmniAttach {
    fn default() -> Self {
        Self::new()
    }
}
impl OmniAttach {
    pub fn new() -> Self {
        OmniAttachBuilder::new()
            .with_portable_defaults()
            .expect("unique static routes")
            .build()
    }
    pub fn builder() -> OmniAttachBuilder {
        OmniAttachBuilder::new()
    }
    /// These are registered route contracts, not tool/model health checks.
    pub fn capabilities(&self) -> Vec<Route> {
        self.entries.values().map(|e| e.route.clone()).collect()
    }
    fn select(
        &self,
        format: Format,
        request: &ConversionRequest,
        context: &FacadeContext,
    ) -> Result<&Entry> {
        if let Some(id) = &request.route {
            let entry = self
                .entries
                .get(id)
                .ok_or_else(|| Error::UnknownRoute(id.clone()))?;
            if !entry.route.accepts(format, &request.goal) {
                return Err(Error::RouteMismatch {
                    route: id.clone(),
                    format,
                    goal: request.goal.clone(),
                });
            }
            if entry.route.execution == ExecutionKind::External && !context.policy.allow_external {
                return Err(Error::ExternalDisabled);
            }
            return Ok(entry);
        }
        let eligible: Vec<_> = self
            .entries
            .values()
            .filter(|e| e.route.accepts(format, &request.goal))
            .collect();
        let permitted: Vec<_> = eligible
            .iter()
            .filter(|e| {
                e.route.execution != ExecutionKind::External || context.policy.allow_external
            })
            .copied()
            .collect();
        match permitted.as_slice() {
            [] if !eligible.is_empty() => Err(Error::ExternalDisabled),
            [] => Err(Error::NoRoute {
                format,
                goal: request.goal.clone(),
            }),
            [entry] => Ok(entry),
            _ => Err(Error::AmbiguousRoute(
                permitted.iter().map(|e| e.route.id.clone()).collect(),
            )),
        }
    }
    /// Advisory plan from current file prefix. Conversion redetects the staged bytes.
    pub fn plan(&self, request: &ConversionRequest, context: &FacadeContext) -> Result<Plan> {
        if context.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let metadata = fs::metadata(&request.path)?;
        if !metadata.is_file() {
            return Err(Error::InvalidInput("not a regular file".into()));
        }
        if metadata.len() > context.policy.max_input_bytes {
            return Err(Error::Limit("input bytes".into()));
        }
        let detection = detect(&request.path)?;
        self.make_plan(request, context, detection)
    }
    fn make_plan(
        &self,
        request: &ConversionRequest,
        context: &FacadeContext,
        detection: Detection,
    ) -> Result<Plan> {
        let format = request.format.unwrap_or(detection.format);
        if format == Format::Unknown {
            return Err(Error::UnknownFormat);
        }
        let entry = self.select(format, request, context)?;
        let mut warnings = detection.warnings.clone();
        if let Some(override_format) = request.format
            && override_format != detection.format
        {
            warnings.push(format!(
                "explicit format override {override_format:?} replaces detected {:?}",
                detection.format
            ));
        }
        Ok(Plan {
            route: entry.route.clone(),
            detection,
            goal: request.goal.clone(),
            warnings,
        })
    }
    pub fn convert(
        &self,
        request: &ConversionRequest,
        context: &FacadeContext,
    ) -> Result<FacadeBundle> {
        // Reserve a context before staging, so the cooperative deadline includes copying.
        let initial = JobContext::new(context, request.route.as_deref().unwrap_or("planning"))?;
        initial.check()?;
        let metadata = fs::metadata(&request.path)?;
        if !metadata.is_file() {
            return Err(Error::InvalidInput("not a regular file".into()));
        }
        if metadata.len() > context.policy.max_input_bytes {
            return Err(Error::Limit("input bytes".into()));
        }
        let directory = if let Some(parent) = &context.policy.scratch_parent {
            tempfile::Builder::new()
                .prefix("omniattach-facade-")
                .tempdir_in(parent.canonicalize()?)?
        } else {
            tempfile::Builder::new()
                .prefix("omniattach-facade-")
                .tempdir()?
        };
        let extension = request
            .path
            .extension()
            .and_then(|v| v.to_str())
            .filter(|v| v.len() <= 16 && v.bytes().all(|b| b.is_ascii_alphanumeric()));
        let input_path = directory.path().join(
            extension
                .map(|e| format!("input.{e}"))
                .unwrap_or_else(|| "input".into()),
        );
        initial.report(Stage::Staging, None);
        let result: Result<FacadeManifest> = (|| {
            let source = stage(&request.path, &input_path, &initial)?;
            initial.check()?;
            let detection = detect(&input_path)?;
            let plan = self.make_plan(request, context, detection)?;
            let format = request.format.unwrap_or(plan.detection.format);
            let job = initial.for_route(&plan.route.id);
            let output_dir = directory.path().join("out");
            fs::create_dir(&output_dir)?;
            let input = StagedInput {
                path: input_path,
                output_dir: output_dir.clone(),
                source: source.clone(),
                format,
            };
            let entry = &self.entries[&plan.route.id];
            job.report(Stage::Converting, None);
            let mut data = entry.converter.convert(&input, request, &job)?;
            job.check()?;
            let artifacts = crate::bundle::validate_artifacts(&output_dir, &data.artifacts, &job)?;
            for warning in plan.warnings {
                data.warn("DETECTION", warning);
            }
            let manifest = FacadeManifest {
                schema_version: "omniattach/facade/0.1".into(),
                route: plan.route,
                source,
                format,
                goal: request.goal.clone(),
                elapsed_ms: job.elapsed().as_millis().min(u64::MAX as u128) as u64,
                text: data.text,
                payloads: data.payloads,
                artifacts,
                warnings: data.warnings,
            };
            job.check()?;
            job.report(Stage::Completed, None);
            Ok(manifest)
        })();
        match result {
            Ok(manifest) => Ok(FacadeBundle::new(manifest, directory)),
            Err(error) => {
                initial.report(Stage::Failed, Some(error.to_string()));
                Err(error)
            }
        }
    }
}
fn stage(source: &Path, destination: &Path, context: &JobContext) -> Result<SourceIdentity> {
    let mut reader = File::open(source)?;
    if !reader.metadata()?.is_file() {
        return Err(Error::InvalidInput("not a regular file".into()));
    }
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        context.check()?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(count as u64);
        if bytes > context.policy().max_input_bytes {
            return Err(Error::Limit("input bytes".into()));
        }
        hash.update(&buffer[..count]);
        writer.write_all(&buffer[..count])?;
    }
    Ok(SourceIdentity {
        name: source.to_string_lossy().into_owned(),
        bytes,
        sha256: format!("{:x}", hash.finalize()),
    })
}
