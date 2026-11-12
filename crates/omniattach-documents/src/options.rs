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
/// Recognizes one image-only PDF page, receiving its one-based number.
pub type PdfOcrFn = dyn Fn(u32) -> crate::Result<String> + Send + Sync;
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
    /// Explicit fallback encoding label; default is strict UTF-8/BOM.
    pub encoding: Option<String>,
    pub csv_has_headers: bool,
    pub csv_delimiter: Option<u8>,
    pub xml_table: Option<XmlTableSpec>,
    /// OCR for otherwise empty PDF pages.
    pub pdf_ocr: Option<PdfOcr>,
    /// Estimated maximum tokens in a joined line-end word. Default 6; 0 disables.
    pub dehyphenate_max_tokens: usize,
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
            dehyphenate_max_tokens: 6,
        }
    }
}
