//! Page-local table discovery and candidate conversion.
//!
//! Detectors, selected by the page's painting style:
//! - [`PdfTableDetector::StrokedGrid`]: one stroked path of full-span rulings.
//! - [`PdfTableDetector::FilledGrid`]: thin filled rectangles forming closed bands.
//! - [`PdfTableDetector::OpenRows`]: filled rules without outer borders, with row spans.
//!
//! All detectors reject XObjects (`Do`) and non-default page Rotate/UserUnit.
use super::paths::number;
use super::{
    DISCOVERY_SCHEMA, PdfTableConversion, PdfTableGrid, PdfTableSpan, convert_inspection, filled,
    invalid, limit, stroked,
};
use crate::{Error, Options, Result};
use lopdf::content::Content;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

/// Which detector produced a candidate.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfTableDetector {
    StrokedGrid,
    FilledGrid,
    OpenRows,
}

impl PdfTableDetector {
    fn note(self) -> &'static str {
        match self {
            Self::StrokedGrid => "Detected from one stroked grid path",
            Self::FilledGrid => {
                "Detected from thin filled rulings; full-width section rows become separate candidates"
            }
            Self::OpenRows => {
                "Open-sided table: outer edges inferred from horizontal rules; merged cells are row spans only"
            }
        }
    }
}

/// A discovered grid. `candidate` is one-based and page-local; it is only
/// valid for the same input bytes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableCandidate {
    pub candidate: usize,
    pub detector: PdfTableDetector,
    pub page: u32,
    pub x_boundaries: Vec<f64>,
    pub y_boundaries: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cell_spans: Vec<PdfTableSpan>,
}

impl PdfTableCandidate {
    pub fn grid(&self) -> PdfTableGrid {
        PdfTableGrid {
            page: self.page,
            x_boundaries: self.x_boundaries.clone(),
            y_boundaries: self.y_boundaries.clone(),
            cell_spans: self.cell_spans.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableDiscovery {
    pub schema_version: String,
    pub source: String,
    pub source_sha256: String,
    pub page: u32,
    pub candidates: Vec<PdfTableCandidate>,
    pub warnings: Vec<String>,
}

/// Discover candidates on one page of a PDF held in memory. No OCR or model.
pub fn discover_pdf_tables_bytes(
    name: &str,
    bytes: &[u8],
    page: u32,
    options: &Options,
) -> Result<PdfTableDiscovery> {
    if bytes.len() > options.limits.input_bytes {
        return Err(limit("input bytes"));
    }
    let content = page_content(bytes, page, options)?;
    let (grids, detector) = if filled::applies(&content) {
        let (grids, open) = filled::discover(&content, page, options)?;
        let detector = if open {
            PdfTableDetector::OpenRows
        } else {
            PdfTableDetector::FilledGrid
        };
        (grids, detector)
    } else {
        (
            stroked::discover(&content, page, options)?,
            PdfTableDetector::StrokedGrid,
        )
    };
    let candidates = grids
        .into_iter()
        .enumerate()
        .map(|(index, g)| PdfTableCandidate {
            candidate: index + 1,
            detector,
            page,
            x_boundaries: g.x_boundaries,
            y_boundaries: g.y_boundaries,
            cell_spans: g.cell_spans,
        })
        .collect::<Vec<_>>();
    let mut warnings = vec![
        "Vector rulings in page content only; no XObjects, scanned tables or cross-page joins"
            .to_string(),
    ];
    if !candidates.is_empty() {
        warnings.push(detector.note().into());
    }
    Ok(PdfTableDiscovery {
        schema_version: DISCOVERY_SCHEMA.into(),
        source: name.into(),
        source_sha256: format!("{:x}", Sha256::digest(bytes)),
        page,
        candidates,
        warnings,
    })
}

/// Path variant; reads the input once within the configured size limit.
pub fn discover_pdf_tables(
    path: impl AsRef<Path>,
    page: u32,
    options: &Options,
) -> Result<PdfTableDiscovery> {
    let path = path.as_ref();
    discover_pdf_tables_bytes(
        &path.to_string_lossy(),
        &read(path, options)?,
        page,
        options,
    )
}

/// Re-discover candidate `candidate` (one-based) and convert it from the
/// same bytes.
pub fn convert_pdf_table_bytes(
    name: &str,
    bytes: &[u8],
    page: u32,
    candidate: usize,
    options: &Options,
) -> Result<PdfTableConversion> {
    let index = candidate
        .checked_sub(1)
        .ok_or_else(|| invalid("candidate numbers are one-based"))?;
    let discovery = discover_pdf_tables_bytes(name, bytes, page, options)?;
    let selected = discovery
        .candidates
        .get(index)
        .ok_or_else(|| invalid("candidate not found"))?;
    let report = crate::inspect_pdf_bytes(name, bytes, options)?;
    let mut result = convert_inspection(&report, &selected.grid(), options)?;
    result.warnings.push(selected.detector.note().into());
    Ok(result)
}

/// Path variant of [`convert_pdf_table_bytes`]. Writes nothing.
pub fn convert_pdf_table(
    path: impl AsRef<Path>,
    page: u32,
    candidate: usize,
    options: &Options,
) -> Result<PdfTableConversion> {
    let path = path.as_ref();
    convert_pdf_table_bytes(
        &path.to_string_lossy(),
        &read(path, options)?,
        page,
        candidate,
        options,
    )
}

fn read(path: &Path, options: &Options) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("PDF input is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > options.limits.input_bytes {
        return Err(limit("input bytes"));
    }
    Ok(bytes)
}

/// Decoded content of one page, after checking page Rotate/UserUnit.
fn page_content(bytes: &[u8], page: u32, options: &Options) -> Result<Content> {
    let parse = |e: lopdf::Error| Error::Parse(e.to_string());
    let max_bytes = options.limits.archive_bytes.min(usize::MAX as u64) as usize;
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(max_bytes),
            ..Default::default()
        },
    )
    .map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    if pdf.is_encrypted() {
        return Err(invalid("encrypted PDF unsupported"));
    }
    let pages = pdf.get_pages();
    if pages.len() > options.limits.blocks {
        return Err(limit("PDF pages"));
    }
    let id = *pages
        .get(&page)
        .ok_or_else(|| invalid("page not found; pages are one-based"))?;
    // Rotate and UserUnit are inheritable; walk the page tree upwards.
    let mut dictionary = pdf.get_dictionary(id).map_err(parse)?;
    for depth in 0..32 {
        for (key, expected) in [(b"Rotate".as_slice(), 0.0), (b"UserUnit".as_slice(), 1.0)] {
            if let Ok(v) = dictionary.get(key)
                && number(v)? != expected
            {
                return Err(invalid("non-default page rotation or UserUnit unsupported"));
            }
        }
        let Ok(parent) = dictionary.get(b"Parent") else {
            break;
        };
        if depth == 31 {
            return Err(invalid("page ancestry limit"));
        }
        dictionary = pdf
            .get_dictionary(parent.as_reference().map_err(parse)?)
            .map_err(parse)?;
    }
    let data = pdf
        .get_page_content_with_limit(id, max_bytes)
        .map_err(parse)?;
    Content::decode(&data).map_err(parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_limit_checked_before_parse() {
        let mut o = Options::default();
        o.limits.input_bytes = 0;
        assert!(matches!(
            discover_pdf_tables_bytes("x", b"x", 1, &o),
            Err(Error::LimitExceeded(_))
        ));
    }

    #[test]
    fn zero_candidate_rejected() {
        assert!(convert_pdf_table_bytes("x", b"", 1, 0, &Options::default()).is_err());
    }

    #[test]
    fn candidate_json_without_spans_defaults_to_none() {
        let c: PdfTableCandidate = serde_json::from_str(
            r#"{"candidate":1,"detector":"stroked_grid","page":1,"x_boundaries":[0,10],"y_boundaries":[0,10]}"#,
        )
        .unwrap();
        assert!(c.cell_spans.is_empty());
        assert!(!serde_json::to_string(&c).unwrap().contains("cell_spans"));
    }
}
