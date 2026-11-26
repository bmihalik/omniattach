//! Page-local table discovery and candidate conversion.
//!
//! Strict detectors, selected by the page's painting style, run first:
//! - [`PdfTableDetector::StrokedGrid`]: one stroked path of full-span rulings.
//! - [`PdfTableDetector::FilledGrid`]: thin filled rectangles forming closed bands.
//! - [`PdfTableDetector::OpenRows`]: filled rules without outer borders, with row spans.
//!
//! They reject XObjects (`Do`), clips they cannot prove harmless, mixed
//! painting and non-default page Rotate/UserUnit. When they decline or find
//! nothing, [`PdfTableDetector::Layout`] infers tables from any visible rules
//! plus text alignment, in upright and rotated reading frames.
use super::paths::number;
use super::{
    DISCOVERY_SCHEMA, MAX_CANDIDATES, PdfTableConversion, PdfTableGrid, PdfTableSpan,
    PdfTextAssignment, convert_inspection, filled, frame, invalid, layout, limit, rules, stroked,
};
use crate::{Error, Options, PdfElementKind, PdfInspection, Result};
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
    /// Rules plus text alignment; boundaries are inferred.
    Layout,
    /// [`Self::Layout`] on OCR words and raster rulings of a scanned page.
    OcrLayout,
}

impl PdfTableDetector {
    pub(super) fn note(self) -> &'static str {
        match self {
            Self::StrokedGrid => "Detected from one stroked grid path",
            Self::FilledGrid => {
                "Detected from thin filled rulings; full-width section rows become separate candidates"
            }
            Self::OpenRows => {
                "Open-sided table: outer edges inferred from horizontal rules; merged cells are row spans only"
            }
            Self::Layout => {
                "Layout table: bounded by horizontal rules; columns from vertical rules or text alignment, \
                 rows from rules and text lines; merged cells are not detected"
            }
            Self::OcrLayout => {
                "OCR layout table: words and rulings recognized from a page image; text may contain OCR errors"
            }
        }
    }

    /// Inferred grids place text by centre; drawn grids require containment.
    pub fn text_assignment(self) -> PdfTextAssignment {
        match self {
            Self::Layout | Self::OcrLayout => PdfTextAssignment::Centre,
            _ => PdfTextAssignment::Contained,
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
    /// Reading frame of the boundaries; see [`PdfTableGrid::rotation`].
    #[serde(default, skip_serializing_if = "super::is_zero")]
    pub rotation: u16,
}

impl PdfTableCandidate {
    pub fn grid(&self) -> PdfTableGrid {
        PdfTableGrid {
            page: self.page,
            x_boundaries: self.x_boundaries.clone(),
            y_boundaries: self.y_boundaries.clone(),
            cell_spans: self.cell_spans.clone(),
            rotation: self.rotation,
            text_assignment: self.detector.text_assignment(),
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
    discover(name, bytes, page, options, None)
}

fn discover(
    name: &str,
    bytes: &[u8],
    page: u32,
    options: &Options,
    report: Option<&PdfInspection>,
) -> Result<PdfTableDiscovery> {
    if bytes.len() > options.limits.input_bytes {
        return Err(limit("input bytes"));
    }
    let PageContent {
        content,
        default_page,
        media_box,
    } = page_content(bytes, page, options)?;
    let strict = if default_page {
        strict(&content, page, options)
    } else {
        Err(invalid("non-default page rotation or UserUnit unsupported"))
    };
    let mut warnings = Vec::new();
    let candidates = match strict {
        Ok((grids, detector)) if !grids.is_empty() => {
            warnings.push(
                "Vector rulings in page content only; no XObjects, scanned tables or cross-page joins"
                    .to_string(),
            );
            warnings.push(detector.note().into());
            grids
                .into_iter()
                .map(|g| (detector, 0, g.x_boundaries, g.y_boundaries, g.cell_spans))
                .collect::<Vec<_>>()
        }
        Err(Error::LimitExceeded(m)) => return Err(Error::LimitExceeded(m)),
        declined => {
            if let Err(e) = declined {
                warnings.push(format!("Strict ruled-grid detectors declined: {e}"));
            }
            let owned;
            let report = match report {
                Some(r) => r,
                None => {
                    owned = crate::inspect_pdf_bytes(name, bytes, options)?;
                    &owned
                }
            };
            let found = layout_tables(&content, media_box, report, page, options)?;
            warnings.push(
                "Layout detection: rules from any painting style plus text alignment; images and \
                 Form XObjects are ignored and cross-page joins are not made"
                    .into(),
            );
            if found.is_empty() && !has_text(report, page) {
                warnings.push(
                    "Page has no native text; scanned tables need the OCR path (pdf_tables::ocr)"
                        .into(),
                );
            } else if !found.is_empty() {
                warnings.push(PdfTableDetector::Layout.note().into());
            }
            found
                .into_iter()
                .map(|(rotation, t)| (PdfTableDetector::Layout, rotation, t.xs, t.ys, Vec::new()))
                .collect()
        }
    };
    if candidates.len() > MAX_CANDIDATES {
        return Err(limit("table candidates"));
    }
    let candidates = candidates
        .into_iter()
        .enumerate()
        .map(
            |(index, (detector, rotation, xs, ys, spans))| PdfTableCandidate {
                candidate: index + 1,
                detector,
                page,
                x_boundaries: xs,
                y_boundaries: ys,
                cell_spans: spans,
                rotation,
            },
        )
        .collect::<Vec<_>>();
    Ok(PdfTableDiscovery {
        schema_version: DISCOVERY_SCHEMA.into(),
        source: name.into(),
        source_sha256: format!("{:x}", Sha256::digest(bytes)),
        page,
        candidates,
        warnings,
    })
}

fn strict(
    content: &Content,
    page: u32,
    options: &Options,
) -> Result<(Vec<PdfTableGrid>, PdfTableDetector)> {
    if filled::applies(content) {
        let (grids, open) = filled::discover(content, page, options)?;
        let detector = if open {
            PdfTableDetector::OpenRows
        } else {
            PdfTableDetector::FilledGrid
        };
        Ok((grids, detector))
    } else {
        Ok((
            stroked::discover(content, page, options)?,
            PdfTableDetector::StrokedGrid,
        ))
    }
}

fn has_text(report: &PdfInspection, page: u32) -> bool {
    report.elements.iter().any(|e| {
        e.anchor.page == page
            && e.kind == PdfElementKind::Text
            && e.text.as_deref().is_some_and(|t| !t.trim().is_empty())
    })
}

/// Layout tables in every reading frame worth trying, top-down per frame.
fn layout_tables(
    content: &Content,
    media_box: Option<[f64; 4]>,
    report: &PdfInspection,
    page: u32,
    options: &Options,
) -> Result<Vec<(u16, layout::Table)>> {
    let page_rules = rules::collect(content, options, media_box)?;
    let mut out = Vec::new();
    for rotation in frame::frames(&report.elements, page) {
        let boxes: Vec<layout::Box4> = report
            .elements
            .iter()
            .filter(|e| {
                e.anchor.page == page
                    && e.kind == PdfElementKind::Text
                    && e.text.as_deref().is_some_and(|t| !t.trim().is_empty())
                    && frame::of_angle(e.text_rotation) == Some(rotation)
            })
            .filter_map(|e| e.bounds)
            .map(|b| frame::to_frame([b.left, b.bottom, b.right, b.top], rotation))
            .collect();
        let frame_rules: Vec<_> = page_rules.iter().map(|r| r.rotated(rotation)).collect();
        for t in layout::tables(&frame_rules, &boxes, layout::NATIVE) {
            if (t.xs.len() - 1) * (t.ys.len() - 1) <= options.limits.cells.min(super::MAX_CELLS) {
                out.push((rotation, t));
            }
        }
    }
    Ok(out)
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
    if bytes.len() > options.limits.input_bytes {
        return Err(limit("input bytes"));
    }
    let report = crate::inspect_pdf_bytes(name, bytes, options)?;
    let discovery = discover(name, bytes, page, options, Some(&report))?;
    let selected = discovery
        .candidates
        .get(index)
        .ok_or_else(|| invalid("candidate not found"))?;
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

/// Decoded content of one page.
struct PageContent {
    content: Content,
    /// Rotate and UserUnit are defaults.
    default_page: bool,
    media_box: Option<[f64; 4]>,
}

fn page_content(bytes: &[u8], page: u32, options: &Options) -> Result<PageContent> {
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
    let mut default_page = true;
    let mut media_box = None;
    for depth in 0..32 {
        if media_box.is_none()
            && let Ok(lopdf::Object::Array(a)) = dictionary.get(b"MediaBox")
            && a.len() == 4
            && let Ok(v) = a.iter().map(number).collect::<Result<Vec<f64>>>()
        {
            media_box = Some([
                v[0].min(v[2]),
                v[1].min(v[3]),
                v[0].max(v[2]),
                v[1].max(v[3]),
            ]);
        }
        for (key, expected) in [(b"Rotate".as_slice(), 0.0), (b"UserUnit".as_slice(), 1.0)] {
            if let Ok(v) = dictionary.get(key)
                && number(v)? != expected
            {
                default_page = false;
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
    Ok(PageContent {
        content: Content::decode(&data).map_err(parse)?,
        default_page,
        media_box,
    })
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
