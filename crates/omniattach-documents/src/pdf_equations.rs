//! Opt-in recognition of explicitly selected native PDF image occurrences.
use crate::{Block, BlockKind, Diagnostic, Document, Error, ExtractedAsset, Options, Result, SourceSpan};
use serde::Serialize;
use std::collections::BTreeSet;

/// Implementations may retain a loaded model. They receive native asset bytes,
/// not viewer-space crops. The caller owns deadlines/isolation for this trusted callback.
pub trait EquationRecognizer {
    fn recognize(&mut self, block: &Block, asset: &ExtractedAsset) -> Result<String>;
}
impl<F> EquationRecognizer for F
where
    F: FnMut(&Block, &ExtractedAsset) -> Result<String>,
{
    fn recognize(&mut self, block: &Block, asset: &ExtractedAsset) -> Result<String> {
        self(block, asset)
    }
}

#[derive(Debug, Clone)]
pub struct EquationLimits {
    pub max_equations: usize,
    pub max_image_bytes: usize,
    pub max_latex_bytes: usize,
}
impl Default for EquationLimits {
    fn default() -> Self {
        Self { max_equations: 64, max_image_bytes: 8 * 1024 * 1024, max_latex_bytes: 16 * 1024 }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EquationOutcome {
    Recognized { latex: String },
    Fallback { reason: String },
}
#[derive(Debug, Clone, Serialize)]
pub struct EquationResult {
    pub block_id: String,
    pub asset_id: String,
    pub source: SourceSpan,
    #[serde(flatten)]
    pub outcome: EquationOutcome,
}
/// The canonical document is preserved; only this wrapper's Markdown substitutes math.
#[derive(Debug, Clone, Serialize)]
pub struct EquationDocument {
    schema_version: String,
    document: Document,
    equations: Vec<EquationResult>,
}
impl EquationDocument {
    pub fn document(&self) -> &Document { &self.document }
    pub fn equations(&self) -> &[EquationResult] { &self.equations }
    pub fn to_json(&self) -> Result<String> { Ok(serde_json::to_string_pretty(self)?) }
    pub fn to_markdown(&self) -> String {
        if !self.equations.iter().any(|e| matches!(&e.outcome, EquationOutcome::Recognized { .. })) {
            return self.document.to_markdown();
        }
        // Rendering needs asset metadata, never another copy of image payloads.
        let mut shell = Document {
            schema_version: self.document.schema_version.clone(), source: self.document.source.clone(),
            title: self.document.title.clone(), format: self.document.format.clone(), extractor: self.document.extractor.clone(),
            blocks: Vec::new(), tables: self.document.tables.clone(), diagnostics: Vec::new(),
            assets: self.document.assets.iter().map(|a| ExtractedAsset {
                id: a.id.clone(), filename: a.filename.clone(), media_type: a.media_type.clone(),
                sha256: a.sha256.clone(), byte_length: a.byte_length, bytes: Vec::new(), source: a.source.clone(),
            }).collect(),
        };
        let mut parts = Vec::new();
        for block in &self.document.blocks {
            if let Some(EquationResult { outcome: EquationOutcome::Recognized { latex }, .. }) = self.equations.iter().find(|e| e.block_id == block.id) {
                parts.push(format!("$$\n{}\n$$", math_body(latex)));
            } else {
                shell.blocks.clear(); shell.blocks.push(block.clone());
                let rendered = shell.to_markdown();
                parts.push(rendered.strip_suffix('\n').unwrap_or(&rendered).to_owned());
            }
        }
        if parts.is_empty() { String::new() } else { parts.join("\n\n") + "\n" }
    }
}
fn math_body(latex: &str) -> &str {
    let text = latex.trim();
    for (left, right) in [(r"\[", r"\]"), (r"\(", r"\)"), ("$$", "$$")] {
        if let Some(body) = text.strip_prefix(left).and_then(|s| s.strip_suffix(right)) { return body.trim(); }
    }
    text
}

/// Explicit opt-in convenience path; ordinary extraction and Options are unchanged.
pub fn extract_pdf_with_equations(
    name: &str, bytes: &[u8], options: &Options, selected_block_ids: &[String],
    recognizer: &mut dyn EquationRecognizer, limits: &EquationLimits,
) -> Result<EquationDocument> {
    recognize_image_equations(crate::extract_pdf_with_images(name, bytes, options)?, selected_block_ids, recognizer, limits)
}

/// Consume an image-bearing PDF projection. Selection is by occurrence block ID,
/// never by shared asset ID. No automatic detector, renderer, download or OCR runs.
pub fn recognize_image_equations(
    mut document: Document, selected_block_ids: &[String],
    recognizer: &mut dyn EquationRecognizer, limits: &EquationLimits,
) -> Result<EquationDocument> {
    if document.format != "pdf" { return Err(Error::InvalidInput("equation projection requires PDF input".into())); }
    if selected_block_ids.len() > limits.max_equations { return Err(Error::LimitExceeded("equation occurrences".into())); }
    let selected: BTreeSet<_> = selected_block_ids.iter().map(String::as_str).collect();
    if selected.len() != selected_block_ids.len() { return Err(Error::InvalidInput("duplicate equation block IDs".into())); }
    // Validate the entire selection before any callback runs.
    for id in &selected {
        if !document.blocks.iter().any(|b| b.id == *id && matches!(&b.kind, BlockKind::Image { .. })) {
            return Err(Error::InvalidInput(format!("not an image occurrence: {id}")));
        }
    }
    let mut equations = Vec::new();
    let mut diagnostics = Vec::new();
    for block in &document.blocks {
        if !selected.contains(block.id.as_str()) { continue; }
        let BlockKind::Image { asset_id } = &block.kind else { unreachable!() };
        let prediction = match document.assets.iter().find(|a| a.id == *asset_id) {
            None => Err("image asset unavailable".to_string()),
            Some(asset) if !matches!(asset.media_type.as_str(), "image/png" | "image/jpeg") => Err("unsupported equation image media type".into()),
            Some(asset) if asset.bytes.len() > limits.max_image_bytes => Err("equation image byte limit".into()),
            Some(asset) => recognizer.recognize(block, asset).map_err(|e| e.to_string()),
        };
        let outcome = match prediction {
            Ok(latex) if latex.len() > limits.max_latex_bytes => EquationOutcome::Fallback { reason: "equation LaTeX byte limit".into() },
            Ok(latex) if math_body(&latex).is_empty() => EquationOutcome::Fallback { reason: "empty equation recognition".into() },
            Ok(latex) => EquationOutcome::Recognized { latex },
            Err(reason) => EquationOutcome::Fallback { reason: reason.chars().take(512).collect() },
        };
        if let EquationOutcome::Fallback { reason } = &outcome {
            diagnostics.push(Diagnostic { code: "PDF_EQUATION_FALLBACK".into(), message: reason.clone(), source: Some(block.source.clone()) });
        }
        equations.push(EquationResult { block_id: block.id.clone(), asset_id: asset_id.clone(), source: block.source.clone(), outcome });
    }
    document.diagnostics.extend(diagnostics);
    Ok(EquationDocument { schema_version: "omniattach/pdf-equations/0.1".into(), document, equations })
}
