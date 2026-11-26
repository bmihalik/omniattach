//! Opt-in native-text table extraction, separate from ordinary extraction.
//!
//! Two steps, both model-free:
//! 1. [`discovery`] finds candidate grids from vector rulings on one page.
//! 2. [`convert_inspection`] assigns inspected text runs to the grid cells and
//!    renders Markdown.
//!
//! Coordinates are the inspection's unnormalized PDF user space (origin bottom
//! left). Text bounds are estimates, so assignment is strict horizontally and
//! has a small, bounded vertical tolerance.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

mod cell_lines;
mod convert;
pub mod discovery;
mod filled;
mod open_rows;
mod paths;
mod stroked;

pub use convert::convert_inspection;
pub use discovery::{
    PdfTableCandidate, PdfTableDetector, PdfTableDiscovery, convert_pdf_table,
    convert_pdf_table_bytes, discover_pdf_tables, discover_pdf_tables_bytes,
};

/// Output schema of [`PdfTableConversion`].
pub const TABLE_SCHEMA: &str = "omniattach/pdf-table/0.1";
/// Output schema of [`PdfTableDiscovery`].
pub const DISCOVERY_SCHEMA: &str = "omniattach/pdf-table-discovery/0.2";

/// Hard engine caps, applied in addition to the configurable [`crate::Limits`].
const MAX_BOUNDARIES: usize = 257;
const MAX_CELLS: usize = 16_384;
const MAX_CANDIDATES: usize = 128;
const MAX_TEXT_BYTES: u64 = 8 * 1024 * 1024;
/// Adjacent grid boundaries must be further apart than this (PDF units).
const MIN_BOUNDARY_GAP: f64 = 0.5;

/// A table grid on one page. Boundaries are ascending; `y` runs bottom-up.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableGrid {
    /// One-based page number.
    pub page: u32,
    pub x_boundaries: Vec<f64>,
    pub y_boundaries: Vec<f64>,
    /// Merged cells. Empty means every cell is 1x1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cell_spans: Vec<PdfTableSpan>,
}

/// A merged cell: zero-based, top-down anchor plus its extent in grid slots.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PdfTableSpan {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

/// Result of converting one grid.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PdfTableConversion {
    pub schema_version: String,
    pub source_name: String,
    pub source_sha256: String,
    pub grid: PdfTableGrid,
    /// Inspection sequences of the text runs placed in the table.
    pub source_sequences: Vec<usize>,
    pub first_row_is_header: bool,
    pub markdown: String,
    pub warnings: Vec<String>,
}

impl PdfTableGrid {
    /// Validate boundaries and return `(columns, rows)`.
    pub(crate) fn dimensions(&self) -> Result<(usize, usize)> {
        dimensions(&self.x_boundaries, &self.y_boundaries)
    }
}

fn dimensions(xs: &[f64], ys: &[f64]) -> Result<(usize, usize)> {
    for axis in [xs, ys] {
        if axis.len() < 2
            || axis.len() > MAX_BOUNDARIES
            || axis.iter().any(|n| !n.is_finite())
            || axis.windows(2).any(|w| w[1] - w[0] <= MIN_BOUNDARY_GAP)
        {
            return Err(invalid(format!(
                "grid needs 2..={MAX_BOUNDARIES} finite, ascending boundaries \
                 separated by more than {MIN_BOUNDARY_GAP} PDF units"
            )));
        }
    }
    let (columns, rows) = (xs.len() - 1, ys.len() - 1);
    if columns * rows > MAX_CELLS {
        return Err(invalid("table cell limit exceeded"));
    }
    Ok((columns, rows))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn limit(message: &str) -> Error {
    Error::LimitExceeded(message.into())
}

/// Render top-down rows as a pipe table; the first row becomes the header.
fn markdown(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for (index, row) in rows.iter().enumerate() {
        out.push('|');
        for cell in row {
            out.push(' ');
            out.push_str(cell);
            out.push_str(" |");
        }
        out.push('\n');
        if index == 0 {
            out.push('|');
            out.push_str(&" --- |".repeat(row.len()));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
fn test_report(elements: serde_json::Value) -> crate::PdfInspection {
    serde_json::from_value(serde_json::json!({
        "schema_version": "omniattach/pdf-inspection/0.1",
        "source": {"name": "test.pdf", "sha256": "test-hash", "byte_length": 1, "encoding": null},
        "page_count": 1,
        "coordinate_space": "PDF user space",
        "diagnostics": [],
        "elements": elements,
    }))
    .unwrap()
}

#[cfg(test)]
fn test_text(sequence: usize, text: &str, bounds: [f64; 4]) -> serde_json::Value {
    serde_json::json!({
        "sequence": sequence,
        "kind": "text",
        "anchor": {"page": 1, "operation_visit": sequence, "form_path": [], "object_id": null, "resource_name": null},
        "bounds": {"left": bounds[0], "bottom": bounds[1], "right": bounds[2], "top": bounds[3], "estimated": true},
        "text": text,
        "word_gap_before": false,
        "pixel_size": null,
        "derived_from": [],
        "evidence": [],
    })
}
