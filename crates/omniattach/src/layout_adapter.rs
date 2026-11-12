//! Opt-in layout backends whose relative Markdown asset paths must survive export.
use crate::*;
use omniattach_backends::{Backend, BackendContext, Runner, TextKind};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Read, path::{Component, Path}, sync::Arc};

struct LayoutConverter<B> { backend: B, runner: Arc<dyn Runner> }
fn exported(path: &str) -> Result<String> {
    if path.is_empty() || path.contains(['\\', ':', '\0']) || !Path::new(path).components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(Error::InvalidOutput("unsafe layout artifact path".into()));
    }
    Ok(format!("layout/{path}"))
}
fn hash_file(path: &Path, job: &JobContext) -> Result<String> {
    let mut reader = fs::File::open(path)?; let mut hash = Sha256::new(); let mut buffer = [0u8; 65536];
    loop { job.check()?; let count = reader.read(&mut buffer)?; if count == 0 { break; } hash.update(&buffer[..count]); }
    Ok(format!("{:x}", hash.finalize()))
}
impl<B: Backend + 'static> Converter for LayoutConverter<B> {
    fn convert(&self, input: &StagedInput, request: &ConversionRequest, job: &JobContext) -> Result<ConversionData> {
        job.check()?;
        if !job.policy().allow_external { return Err(Error::ExternalDisabled); }
        if request.options != serde_json::json!({}) && request.options != serde_json::Value::Null {
            return Err(Error::Configuration("layout options belong in the configured MinerU/Marker backend; request options and document dehyphenation do not apply".into()));
        }
        let mut context = BackendContext::new(self.runner.as_ref());
        context.run_options.cancellation = job.cancellation().clone();
        if let Some(remaining) = job.remaining() { context.run_options.timeout = context.run_options.timeout.min(remaining); }
        context.limits.input_bytes = job.policy().max_input_bytes;
        context.limits.artifact_bytes = job.policy().max_export_bytes;
        context.limits.artifact_count = job.policy().max_export_files;
        context.scratch_parent = job.policy().scratch_parent.clone();
        let converted = self.backend.convert(&input.path, &context)?;
        job.check()?;
        let manifest = &converted.manifest;
        if manifest.assets.len() > job.policy().max_export_files { return Err(Error::Limit("layout artifact count".into())); }
        let total = manifest.assets.iter().try_fold(0u64, |n,a| n.checked_add(a.bytes)).ok_or_else(|| Error::Limit("layout artifact bytes overflow".into()))?;
        if total > job.policy().max_export_bytes { return Err(Error::Limit("layout artifact bytes".into())); }
        let package = manifest.metadata.get("layout_package").ok_or_else(|| Error::InvalidOutput("layout package metadata missing".into()))?;
        if package.get("contract").and_then(|v| v.as_str()) != Some("omniattach-layout-package/1") { return Err(Error::InvalidOutput("unsupported layout package contract".into())); }
        let main = package.get("markdown_asset").and_then(|v| v.as_str()).ok_or_else(|| Error::InvalidOutput("layout Markdown asset missing".into()))?;
        let main_export = exported(main)?;
        if !manifest.assets.iter().any(|a| a.path == main && a.media_type == "text/markdown") { return Err(Error::InvalidOutput("layout Markdown is not a declared Markdown artifact".into())); }
        let hashes = package.get("sha256").and_then(|v| v.as_object()).ok_or_else(|| Error::InvalidOutput("layout file hashes missing".into()))?;
        if manifest.text.len() != 1 || manifest.text[0].kind != TextKind::Markdown { return Err(Error::InvalidOutput("layout package must provide one Markdown text unit".into())); }
        let main_hash = hashes.get(main).and_then(|v| v.as_str()).ok_or_else(|| Error::InvalidOutput("layout Markdown hash missing".into()))?;
        if format!("{:x}", Sha256::digest(manifest.text[0].text.as_bytes())) != main_hash { return Err(Error::InvalidOutput("layout text and Markdown artifact disagree".into())); }
        let mut data = ConversionData::default();
        let mut names = HashSet::new(); let mut mapping = Vec::new(); let mut remaining = job.policy().max_export_bytes;
        for (index, asset) in manifest.assets.iter().enumerate() {
            job.check()?;
            let name = exported(&asset.path)?;
            if !names.insert(name.clone()) { return Err(Error::InvalidOutput("duplicate layout artifact".into())); }
            let expected = hashes.get(&asset.path).and_then(|v| v.as_str()).ok_or_else(|| Error::InvalidOutput("layout artifact hash missing".into()))?;
            let source = converted.asset_path(index)?;
            let destination = input.output_dir.join(&name);
            if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; }
            let copied = crate::bundle::copy_checked(&source, &destination, remaining, job)?;
            if copied != asset.bytes || hash_file(&destination, job)? != expected { return Err(Error::InvalidOutput("layout artifact changed during facade transfer".into())); }
            remaining -= copied;
            data.artifacts.push(PendingArtifact::new(&name, &asset.media_type).located(serde_json::json!({"kind":"backend_artifact","backend_path":asset.path})));
            mapping.push(serde_json::json!({"backend_path":asset.path,"artifact_path":name,"sha256":expected,"media_type":asset.media_type}));
        }
        data.text.push(TextFragment { text: manifest.text[0].text.clone(), location: serde_json::json!({"kind":"generated_markdown","artifact":main_export,"backend":manifest.backend}) });
        for warning in &manifest.diagnostics { data.warn(&warning.code, &warning.message); }
        data = data.payload("omniattach/layout-package/1", serde_json::json!({"backend":manifest.backend,"profile":package.get("profile"),"markdown_artifact":main_export,"artifacts":mapping}))?;
        data = data.payload("omniattach-backends/0.1", manifest)?;
        data.warn("LAYOUT_ASSET_PATHS_PRESERVED", "Relative package paths are preserved under layout/. Use the layout-package payload's markdown_artifact as the base for Markdown image links; nested backend paths are backend-relative, not facade export paths.");
        data.warn("BACKEND_SOURCE_IS_STAGED", "Facade source identity refers to the original request; nested backend source.name identifies its staged copy");
        job.check()?; Ok(data)
    }
}
impl OmniAttachBuilder {
    /// Register the configured MinerU profile without starting a process.
    pub fn with_mineru(self, backend: backends::MinerU) -> Result<Self> { self.with_mineru_runner(backend, Arc::new(backends::ProcessRunner)) }
    pub fn with_mineru_runner(self, backend: backends::MinerU, runner: Arc<dyn Runner>) -> Result<Self> { self.with_layout_backend(backend, runner) }
    /// Register the configured Marker adapter without starting a process.
    pub fn with_marker(self, backend: backends::Marker) -> Result<Self> { self.with_marker_runner(backend, Arc::new(backends::ProcessRunner)) }
    pub fn with_marker_runner(self, backend: backends::Marker, runner: Arc<dyn Runner>) -> Result<Self> { self.with_layout_backend(backend, runner) }
    fn with_layout_backend<B: Backend + 'static>(self, backend: B, runner: Arc<dyn Runner>) -> Result<Self> {
        let id = backend.id().to_owned();
        self.register(Route::new(id, vec![Format::Pdf], vec![Goal::Extract, Goal::Markdown, Goal::PdfLayout, Goal::PdfScientific], ExecutionKind::External).described("Explicit model-based PDF conversion with a relocatable Markdown/image/JSON package; no automatic fallback"), LayoutConverter { backend, runner })
    }
}
