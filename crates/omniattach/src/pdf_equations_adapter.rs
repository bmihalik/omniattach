//! Explicit external-worker equation route; default extraction is unchanged.
use crate::*;
use backends::{CommandSpec, ProcessRunner, RunOptions, Runner};
use documents::{Block, BlockKind, ExtractedAsset, pdf_equations::*};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquationWorkerConfig {
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    pub artifacts: Vec<EquationWorkerArtifact>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquationWorkerArtifact {
    pub path: String,
    pub sha256: String,
}
fn default_timeout() -> u64 {
    600_000
}
impl EquationWorkerConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut data = Vec::new();
        fs::File::open(path)?
            .take(1_048_577)
            .read_to_end(&mut data)?;
        if data.len() > 1_048_576 {
            return Err(Error::Limit("equation worker config bytes".into()));
        }
        let config: Self = serde_json::from_slice(&data)?;
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<()> {
        if self.argv.is_empty()
            || self.argv.len() > 128
            || !Path::new(&self.argv[0]).is_absolute()
            || self.argv.iter().any(|s| s.contains('\0'))
            || self.timeout_ms == 0
            || self.artifacts.is_empty()
            || self.artifacts.len() > 64
        {
            return Err(Error::Configuration("equation worker requires absolute executable, bounded argv, positive timeout and declared artifacts".into()));
        }
        if !self.artifacts.iter().any(|a| a.path == self.argv[0])
            || self.artifacts.iter().any(|a| {
                !Path::new(&a.path).is_absolute()
                    || a.sha256.len() != 64
                    || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            })
            || self
                .env
                .iter()
                .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
        {
            return Err(Error::Configuration(
                "invalid equation worker artifact/environment".into(),
            ));
        }
        Ok(())
    }
}
impl OmniAttachBuilder {
    pub fn with_pdf_equation_worker(self, config: EquationWorkerConfig) -> Result<Self> {
        self.with_pdf_equation_worker_runner(config, Arc::new(ProcessRunner))
    }
    /// Runner injection is for embedders/tests; implementations must honor limits.
    pub fn with_pdf_equation_worker_runner(
        self,
        config: EquationWorkerConfig,
        runner: Arc<dyn Runner>,
    ) -> Result<Self> {
        config.validate()?;
        self.register(Route::new("pdf-equations", vec![Format::Pdf], vec![Goal::Custom("pdf-equations".into())], ExecutionKind::External)
            .described("Explicit native PDF image equation recognition; configured local worker, image fallback, no automatic detection"), EquationConverter { config, runner })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    block_ids: Vec<String>,
}
struct EquationConverter {
    config: EquationWorkerConfig,
    runner: Arc<dyn Runner>,
}
fn hash_file(path: &Path, job: &JobContext) -> Result<String> {
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Configuration(
            "worker inputs must be regular files".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        job.check()?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl Converter for EquationConverter {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        job: &JobContext,
    ) -> Result<ConversionData> {
        job.check()?;
        if !job.policy().allow_external {
            return Err(Error::ExternalDisabled);
        }
        let selection: Selection = serde_json::from_value(request.options.clone())
            .map_err(|e| Error::Configuration(e.to_string()))?;
        let limits = EquationLimits::default();
        let selected: BTreeSet<_> = selection.block_ids.iter().cloned().collect();
        if selected.len() != selection.block_ids.len() || selected.len() > limits.max_equations {
            return Err(Error::Configuration(
                "duplicate or excessive equation block IDs".into(),
            ));
        }
        let mut base_request = request.clone();
        base_request.options = Value::Null;
        let mut data =
            crate::pdf_images_adapter::PdfImageConverter.convert(input, &base_request, job)?;
        let payload = data
            .payloads
            .pop()
            .ok_or_else(|| Error::InvalidOutput("missing image projection payload".into()))?;
        let doc: documents::Document = serde_json::from_value(payload.value)?;
        if selected.iter().any(|id| {
            !doc.blocks
                .iter()
                .any(|b| b.id == *id && matches!(&b.kind, BlockKind::Image { .. }))
        }) {
            return Err(Error::Configuration(
                "selection contains an unknown/non-image block ID".into(),
            ));
        }
        if data.artifacts.len().saturating_add(1) > job.policy().max_export_files {
            return Err(Error::Limit("equation artifact count".into()));
        }
        let mut images = Vec::new();
        for block in &doc.blocks {
            if !selected.contains(&block.id) {
                continue;
            }
            let BlockKind::Image { asset_id } = &block.kind else {
                unreachable!()
            };
            if let Some(asset) = doc.assets.iter().find(|a| a.id == *asset_id) {
                if matches!(asset.media_type.as_str(), "image/png" | "image/jpeg")
                    && asset.bytes.len() <= limits.max_image_bytes
                {
                    images
                        .push(json!({"id":block.id,"path":input.output_dir.join(&asset.filename)}));
                }
            }
        }
        let mut predictions = BTreeMap::new();
        let mut response =
            json!({"results":[],"metadata":{"execution":"not_run_no_eligible_selection"}});
        if !images.is_empty() {
            for artifact in &self.config.artifacts {
                if hash_file(Path::new(&artifact.path), job)?
                    != artifact.sha256.to_ascii_lowercase()
                {
                    return Err(Error::Configuration(format!(
                        "equation artifact hash mismatch: {}",
                        artifact.path
                    )));
                }
            }
            let request_path = input
                .path
                .parent()
                .ok_or_else(|| Error::InvalidInput("missing job directory".into()))?
                .join("equation-request.json");
            let bytes = serde_json::to_vec(
                &json!({"schema_version":"omniattach/formula-worker/0.1","images":images}),
            )?;
            if bytes.len() > 1_048_576 {
                return Err(Error::Limit("equation request bytes".into()));
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&request_path)?
                .write_all(&bytes)?;
            let mut command =
                CommandSpec::new(&self.config.argv[0], request_path.parent().unwrap())
                    .args(self.config.argv[1..].iter().cloned())
                    .arg("--request-file")
                    .arg(request_path.as_os_str());
            command
                .env
                .extend(self.config.env.iter().map(|(k, v)| (k.into(), v.into())));
            let mut run = RunOptions::default();
            run.timeout = Duration::from_millis(self.config.timeout_ms);
            if let Some(left) = job.remaining() {
                run.timeout = run.timeout.min(left);
            }
            run.cancellation = job.cancellation().clone();
            run.max_stdout_bytes = 8 * 1024 * 1024;
            run.max_stderr_bytes = 65536;
            let output = self.runner.run(&command, &run).map_err(|e| match e {
                backends::Error::Timeout(_) => Error::Deadline,
                backends::Error::Cancelled => Error::Cancelled,
                backends::Error::Limit(message) => Error::Limit(message),
                other => Error::InvalidOutput(format!("equation worker: {other}")),
            })?;
            job.check()?;
            if output.stdout.len() > run.max_stdout_bytes
                || output.stderr.len() > run.max_stderr_bytes
            {
                return Err(Error::Limit("equation worker output bytes".into()));
            }
            response = serde_json::from_slice(&output.stdout)?;
            let results = response["results"]
                .as_array()
                .ok_or_else(|| Error::InvalidOutput("missing equation results".into()))?;
            let expected: BTreeSet<_> = images.iter().map(|x| x["id"].as_str().unwrap()).collect();
            for result in results {
                let id = result["id"]
                    .as_str()
                    .ok_or_else(|| Error::InvalidOutput("missing result ID".into()))?;
                if !expected.contains(id)
                    || predictions.contains_key(id)
                    || !matches!(result["status"].as_str(), Some("ok" | "error"))
                    || (result["status"] == "ok" && !result["latex"].is_string())
                {
                    return Err(Error::InvalidOutput(
                        "unknown/duplicate ID or invalid equation result".into(),
                    ));
                }
                predictions.insert(id.to_owned(), result.clone());
            }
            if predictions.len() != expected.len() {
                return Err(Error::InvalidOutput("missing equation result IDs".into()));
            }
            if hash_file(&input.path, job)? != input.source.sha256 {
                return Err(Error::InvalidOutput("worker changed staged input".into()));
            }
            for asset in &doc.assets {
                if hash_file(
                    &crate::bundle::relative_file(&input.output_dir, &asset.filename)?,
                    job,
                )? != asset.sha256
                {
                    return Err(Error::InvalidOutput("worker changed image artifact".into()));
                }
            }
        }
        let diagnostic_count = doc.diagnostics.len();
        let mut recognize = |block: &Block, _: &ExtractedAsset| -> documents::Result<String> {
            let pred = predictions
                .get(&block.id)
                .ok_or_else(|| documents::Error::Parse("worker result unavailable".into()))?;
            if pred["status"] == "ok" {
                Ok(pred["latex"].as_str().unwrap().to_owned())
            } else {
                Err(documents::Error::Parse(
                    pred["error"]
                        .as_str()
                        .unwrap_or("equation recognition failed")
                        .chars()
                        .take(512)
                        .collect(),
                ))
            }
        };
        let result = recognize_image_equations(doc, &selection.block_ids, &mut recognize, &limits)
            .map_err(Error::Document)?;
        job.check()?;
        let markdown = result.to_markdown();
        let json = result.to_json().map_err(Error::Document)?;
        let worker = serde_json::to_vec_pretty(&response)?;
        let mut total = (markdown.len() as u64)
            .checked_add(json.len() as u64)
            .and_then(|n| n.checked_add(worker.len() as u64))
            .ok_or_else(|| Error::Limit("equation artifact bytes".into()))?;
        for asset in &result.document().assets {
            total = total
                .checked_add(asset.byte_length as u64)
                .ok_or_else(|| Error::Limit("equation artifact bytes".into()))?;
        }
        if total > job.policy().max_export_bytes {
            return Err(Error::Limit("equation artifact bytes".into()));
        }
        // Replace only job-owned projection files; durable export is still create-new.
        let md_path = crate::bundle::relative_file(&input.output_dir, "document.md")?;
        let json_path = crate::bundle::relative_file(&input.output_dir, "document.json")?;
        fs::write(md_path, &markdown)?;
        fs::write(json_path, &json)?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(input.output_dir.join("equation-worker.json"))?
            .write_all(&worker)?;
        data.artifacts.push(PendingArtifact::new(
            "equation-worker.json",
            "application/json",
        ));
        data.text.clear();
        data.text.push(TextFragment {
            text: markdown,
            location: json!({"kind":"pdf_equations","document_artifact":"document.json"}),
        });
        data.payloads.clear();
        data = data.payload("omniattach/pdf-equations/0.1", &result)?;
        for diagnostic in result.document().diagnostics.iter().skip(diagnostic_count) {
            data.warn(&diagnostic.code, &diagnostic.message);
        }
        data.warn("PDF_EQUATIONS_EXPLICIT_SELECTION","Native PNG/JPEG images only; selection is explicit, completion unknown; no automatic detector or orientation correction");
        job.check()?;
        Ok(data)
    }
}
