use crate::xml::XmlTableSpec;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Limits {
    pub input_bytes: usize,
    pub archive_bytes: u64,
    pub archive_entries: usize,
    pub xml_nodes: u32,
    pub blocks: usize,
    pub cells: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 64 * 1024 * 1024,
            archive_bytes: 256 * 1024 * 1024,
            archive_entries: 10_000,
            xml_nodes: 1_000_000,
            blocks: 100_000,
            cells: 1_000_000,
        }
    }
}

/// Recognizes text on one image-only PDF page. Receives the one-based page number and returns plain text; empty text means nothing was recognized.
pub type PdfOcrFn = dyn Fn(u32) -> crate::Result<String> + Send + Sync;

/// Per-page OCR callback for PDF pages that have no extractable text.
#[derive(Clone)]
pub struct PdfOcr(pub Arc<PdfOcrFn>);

impl std::fmt::Debug for PdfOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PdfOcr(..)")
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub limits: Limits,
    /// Explicit fallback encoding label, e.g. `windows-1250`. Default: strict UTF-8/BOM.
    pub encoding: Option<String>,
    pub csv_has_headers: bool,
    pub csv_delimiter: Option<u8>,
    pub xml_table: Option<XmlTableSpec>,
    /// OCR for PDF pages without extractable text. None means such pages are only reported.
    pub pdf_ocr: Option<PdfOcr>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            encoding: None,
            csv_has_headers: true,
            csv_delimiter: None,
            xml_table: None,
            pdf_ocr: None,
        }
    }
}
