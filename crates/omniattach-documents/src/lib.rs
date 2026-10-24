//! Local document extraction into typed blocks, tables and source spans.
//!
//! ```
//! use omniattach_documents::{extract_bytes, Options};
//! let document = extract_bytes("notes.md", b"# Notes\n\nHello.", &Options::default())?;
//! assert_eq!(document.title.as_deref(), Some("Notes"));
//! assert!(document.to_markdown().contains("# Notes"));
//! # Ok::<(), omniattach_documents::Error>(())
//! ```
mod error;
mod html;
mod model;
#[cfg(feature = "office")]
mod office;
mod options;
#[cfg(feature = "office")]
mod package;
#[cfg(feature = "pdf")]
mod pdf;
mod render;
#[cfg(feature = "office")]
mod spreadsheet;
mod text;
pub mod xml;

pub use error::{Error, Result};
pub use model::*;
pub use options::{Limits, Options, PdfOcr, PdfOcrFn};
pub use render::CsvOptions;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
pub use xml::{Repeated, XmlColumn, XmlTableSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Code,
    Markdown,
    Html,
    Csv,
    Tsv,
    Xml,
    Json,
    JsonLines,
    Docx,
    Pptx,
    Odt,
    Odp,
    Workbook,
    Epub,
    Pdf,
}

/// Extension-based routing with package-signature validation for archive formats.
/// This is not a general content-type sniffer.
pub fn detect_format(name: &str) -> Result<Format> {
    let file = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_ascii_lowercase();
    if matches!(file.as_str(), "makefile" | "dockerfile" | "cmakelists.txt") {
        return Ok(Format::Code);
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(match extension.as_str() {
        "txt" | "log" | "ini" | "cfg" | "conf" | "toml" | "yaml" | "yml" => Format::Text,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cpp" | "hpp" | "go" | "java"
        | "sh" | "sql" | "rb" | "cs" | "swift" | "kt" | "r" | "jl" | "lua" => Format::Code,
        "md" | "markdown" => Format::Markdown,
        "html" | "htm" | "xhtml" => Format::Html,
        "csv" => Format::Csv,
        "tsv" => Format::Tsv,
        "xml" => Format::Xml,
        "json" => Format::Json,
        "jsonl" | "ndjson" => Format::JsonLines,
        "docx" => Format::Docx,
        "pptx" => Format::Pptx,
        "odt" => Format::Odt,
        "odp" => Format::Odp,
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => Format::Workbook,
        "epub" => Format::Epub,
        "pdf" => Format::Pdf,
        _ => return Err(Error::UnsupportedFormat(extension)),
    })
}

pub fn extract(path: impl AsRef<Path>, options: &Options) -> Result<Document> {
    let path = path.as_ref();
    let mut bytes = Vec::new();
    File::open(path)?
        .take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    extract_bytes(&path.to_string_lossy(), &bytes, options)
}
pub fn extract_bytes(name: &str, bytes: &[u8], options: &Options) -> Result<Document> {
    extract_as(name, bytes, detect_format(name)?, options)
}
/// Explicit override useful for extensionless inputs. Never writes beside the input.
pub fn extract_as(name: &str, bytes: &[u8], format: Format, options: &Options) -> Result<Document> {
    if bytes.len() > options.limits.input_bytes {
        return Err(Error::LimitExceeded("input bytes".into()));
    }
    let mut doc = Document {
        schema_version: "omniattach-documents/0.1".into(),
        source: Source {
            name: name.into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            byte_length: bytes.len(),
            encoding: None,
        },
        title: None,
        format: format!("{format:?}").to_ascii_lowercase(),
        extractor: format!("omniattach-documents/{}", env!("CARGO_PKG_VERSION")),
        blocks: Vec::new(),
        tables: Vec::new(),
        diagnostics: Vec::new(),
        assets: Vec::new(),
    };
    match format {
        Format::Docx
        | Format::Pptx
        | Format::Odt
        | Format::Odp
        | Format::Workbook
        | Format::Epub => {
            #[cfg(feature = "office")]
            match format {
                Format::Docx => office::docx(&mut doc, bytes, options)?,
                Format::Pptx => office::pptx(&mut doc, bytes, options)?,
                Format::Odt | Format::Odp => {
                    office::odf(&mut doc, bytes, format == Format::Odp, options)?
                }
                Format::Workbook => spreadsheet::extract(&mut doc, bytes, options)?,
                Format::Epub => office::epub(&mut doc, bytes, options)?,
                _ => unreachable!(),
            }
            #[cfg(not(feature = "office"))]
            return Err(Error::FeatureDisabled("office".into()));
        }
        Format::Pdf => {
            #[cfg(feature = "pdf")]
            pdf::extract(&mut doc, bytes, options)?;
            #[cfg(not(feature = "pdf"))]
            return Err(Error::FeatureDisabled("pdf".into()));
        }
        _ => {
            let (input, encoding) = text::decode(bytes, options.encoding.as_deref())?;
            if input
                .chars()
                .any(|c| c == '\0' || (c < ' ' && !matches!(c, '\n' | '\r' | '\t' | '\u{c}')))
            {
                return Err(Error::InvalidInput(
                    "binary control characters in text input".into(),
                ));
            }
            doc.source.encoding = Some(encoding);
            match format {
                Format::Text | Format::Code => text::plain(
                    &mut doc,
                    &input,
                    format == Format::Code,
                    Path::new(name)
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(str::to_owned),
                    options,
                )?,
                Format::Markdown => text::markdown(&mut doc, &input, options)?,
                Format::Html => html::extract(&mut doc, &input, None, options)?,
                Format::Csv | Format::Tsv => text::delimited(
                    &mut doc,
                    &input,
                    options.csv_delimiter.unwrap_or(if format == Format::Tsv {
                        b'\t'
                    } else {
                        b','
                    }),
                    options,
                )?,
                Format::Xml => xml::extract(
                    &mut doc,
                    &input,
                    options.xml_table.as_ref().ok_or(Error::MappingRequired)?,
                    options,
                )?,
                Format::Json => {
                    let _: serde_json::Value = serde_json::from_str(&input)?;
                    text::plain(&mut doc, &input, true, Some("json".into()), options)?;
                }
                Format::JsonLines => {
                    for line in input.lines().filter(|s| !s.trim().is_empty()) {
                        let _: serde_json::Value = serde_json::from_str(line)?;
                    }
                    text::plain(&mut doc, &input, true, Some("jsonl".into()), options)?;
                }
                _ => unreachable!(),
            }
        }
    }
    if doc.blocks.len() > options.limits.blocks {
        return Err(Error::LimitExceeded("blocks".into()));
    }
    let cells = doc
        .tables
        .iter()
        .flat_map(|t| &t.rows)
        .try_fold(0usize, |sum, row| sum.checked_add(row.len()))
        .ok_or_else(|| Error::LimitExceeded("cell count overflow".into()))?;
    if cells > options.limits.cells {
        return Err(Error::LimitExceeded("total cells".into()));
    }
    Ok(doc)
}

pub fn extract_xml_table(
    name: &str,
    bytes: &[u8],
    spec: XmlTableSpec,
    options: &Options,
) -> Result<Document> {
    let mut options = options.clone();
    options.xml_table = Some(spec);
    extract_as(name, bytes, Format::Xml, &options)
}
