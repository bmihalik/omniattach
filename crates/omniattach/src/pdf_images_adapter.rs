//! Explicit in-process PDF image projection; never competes with ordinary Markdown.
use crate::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::{Read, Write}, path::Path};

impl OmniAttachBuilder {
    /// Register the experimental `pdf-images` route for Goal::Custom("pdf-images").
    /// Registration does not alter ordinary Extract/Markdown route selection.
    pub fn with_pdf_image_projection(self) -> Result<Self> {
        self.register(
            Route::new("pdf-images", vec![Format::Pdf], vec![Goal::Custom("pdf-images".into())], ExecutionKind::InProcess)
                .described("Experimental native PDF text-run/image projection with relocatable Markdown and JPEG/JP2 assets; no OCR or visual reading-order reconstruction"),
            PdfImageConverter,
        )
    }
}
struct PdfImageConverter;
impl Converter for PdfImageConverter {
    fn convert(&self, input: &StagedInput, request: &ConversionRequest, job: &JobContext) -> Result<ConversionData> {
        job.check()?;
        if request.options != serde_json::Value::Null && request.options != serde_json::json!({}) {
            return Err(Error::Configuration("pdf-images accepts no request options; OCR and dehyphenation do not apply to this experimental raw-run projection".into()));
        }
        if job.policy().max_export_files < 2 {
            return Err(Error::Limit("pdf-images requires two document artifacts plus image artifacts".into()));
        }
        let options = documents::Options {
            limits: documents::Limits {
                input_bytes: usize::try_from(job.policy().max_input_bytes).unwrap_or(usize::MAX),
                archive_bytes: documents::Limits::default().archive_bytes.min(job.policy().max_export_bytes),
                archive_entries: documents::Limits::default().archive_entries.min(job.policy().max_export_files.saturating_sub(2)),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bytes = Vec::new();
        fs::File::open(&input.path)?.take(job.policy().max_input_bytes.saturating_add(1)).read_to_end(&mut bytes)?;
        if bytes.len() > options.limits.input_bytes {
            return Err(Error::Limit("PDF input bytes".into()));
        }
        job.check()?;
        let document = documents::extract_pdf_with_images(&input.source.name, &bytes, &options).map_err(Error::Document)?;
        job.check()?;
        let markdown = document.to_markdown();
        let json = document.to_json().map_err(Error::Document)?;
        let mut total = (markdown.len() as u64).checked_add(json.len() as u64)
            .ok_or_else(|| Error::Limit("PDF image artifact byte overflow".into()))?;
        let count = document.assets.len().checked_add(2)
            .ok_or_else(|| Error::Limit("PDF image artifact count overflow".into()))?;
        if count > job.policy().max_export_files {
            return Err(Error::Limit("PDF image artifact count".into()));
        }
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for asset in &document.assets {
            job.check()?;
            let leaf = asset.filename.strip_prefix("images/")
                .ok_or_else(|| Error::InvalidOutput("native PDF image must be under images/".into()))?;
            if leaf.is_empty() || leaf.starts_with('.')
                || !leaf.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                || !names.insert(asset.filename.as_str()) || !ids.insert(asset.id.as_str()) {
                return Err(Error::InvalidOutput("unsafe or duplicate native PDF image path/id".into()));
            }
            if !matches!(asset.media_type.as_str(), "image/jpeg" | "image/jp2")
                || asset.byte_length != asset.bytes.len()
                || asset.sha256 != format!("{:x}", Sha256::digest(&asset.bytes)) {
                return Err(Error::InvalidOutput("invalid native PDF image metadata".into()));
            }
            total = total.checked_add(asset.bytes.len() as u64)
                .ok_or_else(|| Error::Limit("PDF image artifact byte overflow".into()))?;
        }
        if total > job.policy().max_export_bytes {
            return Err(Error::Limit("PDF image artifact bytes including JSON and Markdown".into()));
        }
        for block in &document.blocks {
            if let documents::BlockKind::Image { asset_id } = &block.kind
                && !ids.contains(asset_id.as_str()) {
                return Err(Error::InvalidOutput("native PDF image block has no exported asset".into()));
            }
        }
        let mut data = ConversionData::default().payload(&document.schema_version, &document)?;
        data.text.push(TextFragment {
            text: document.to_text(),
            location: serde_json::json!({"kind":"experimental_pdf_image_projection","document_artifact":"document.json","markdown_artifact":"document.md"}),
        });
        for asset in &document.assets {
            job.check()?;
            write_new(&input.output_dir.join(&asset.filename), &asset.bytes)?;
            data.artifacts.push(PendingArtifact::new(&asset.filename, &asset.media_type).located(
                serde_json::json!({"asset_id":asset.id,"sha256":asset.sha256,"source":asset.source}),
            ));
        }
        for (name, media, bytes) in [
            ("document.json", "application/json", json.as_bytes()),
            ("document.md", "text/markdown", markdown.as_bytes()),
        ] {
            job.check()?;
            write_new(&input.output_dir.join(name), bytes)?;
            data.artifacts.push(PendingArtifact::new(name, media));
        }
        for diagnostic in &document.diagnostics {
            data.warn(&diagnostic.code, &diagnostic.message);
        }
        job.check()?;
        Ok(data)
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)?;
    Ok(())
}
