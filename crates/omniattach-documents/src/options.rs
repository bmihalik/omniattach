use crate::xml::XmlTableSpec;

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
#[derive(Debug, Clone)]
pub struct Options {
    pub limits: Limits,
    /// Explicit fallback encoding label, e.g. `windows-1250`. Default: strict UTF-8/BOM.
    pub encoding: Option<String>,
    pub csv_has_headers: bool,
    pub csv_delimiter: Option<u8>,
    pub xml_table: Option<XmlTableSpec>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            encoding: None,
            csv_has_headers: true,
            csv_delimiter: None,
            xml_table: None,
        }
    }
}
