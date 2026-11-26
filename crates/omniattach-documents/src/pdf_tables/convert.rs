//! Assign inspected text runs to grid cells and render Markdown.
//!
//! One algorithm serves plain grids and grids with merged cells: a plain grid
//! is simply a partition where every cell spans one slot.
use super::{
    MAX_TEXT_BYTES, PdfTableConversion, PdfTableGrid, PdfTableSpan, PdfTextAssignment,
    TABLE_SCHEMA, cell_lines, frame, invalid, limit, markdown,
};
use crate::{Options, PdfBounds, PdfElementKind, PdfInspection, Result};
use std::collections::BTreeSet;

/// Positioned text inside one cell (assignment bounds, not original bounds).
#[derive(Clone, Debug)]
pub(super) struct Run {
    pub bounds: [f64; 4],
    pub text: String,
    pub gap: bool,
    pub sequence: usize,
}

/// Merged-cell layout: `owner[row * columns + column]` is the span index.
struct Partition {
    spans: Vec<PdfTableSpan>,
    owner: Vec<usize>,
    columns: usize,
    rows: usize,
}

impl Partition {
    fn new(spans: &[PdfTableSpan], columns: usize, rows: usize) -> Result<Self> {
        let spans = if spans.is_empty() {
            (0..rows)
                .flat_map(|row| {
                    (0..columns).map(move |column| PdfTableSpan {
                        row,
                        column,
                        row_span: 1,
                        column_span: 1,
                    })
                })
                .collect()
        } else {
            spans.to_vec()
        };
        if spans.len() > columns * rows {
            return Err(invalid("invalid cell-span partition"));
        }
        let mut owner = vec![usize::MAX; columns * rows];
        for (index, s) in spans.iter().enumerate() {
            if s.row >= rows
                || s.column >= columns
                || s.row_span == 0
                || s.column_span == 0
                || s.row_span > rows - s.row
                || s.column_span > columns - s.column
            {
                return Err(invalid("invalid cell span"));
            }
            for row in s.row..s.row + s.row_span {
                for column in s.column..s.column + s.column_span {
                    let slot = &mut owner[row * columns + column];
                    if *slot != usize::MAX {
                        return Err(invalid("overlapping cell spans"));
                    }
                    *slot = index;
                }
            }
        }
        if owner.contains(&usize::MAX) {
            return Err(invalid("incomplete cell-span partition"));
        }
        Ok(Self {
            spans,
            owner,
            columns,
            rows,
        })
    }

    /// Owner of a top-down slot.
    fn at(&self, row: usize, column: usize) -> usize {
        self.owner[row * self.columns + column]
    }
}

/// Where a text run goes.
enum Placement {
    Cell {
        index: usize,
        bottom: f64,
        top: f64,
    },
    /// Small vertical incursion from outside the table; ignored.
    Outside,
}

/// Vertical tolerance for estimated text bounds: min(1.5 units, 15% of height).
fn tolerance(height: f64) -> f64 {
    1.5_f64.min(0.15 * height)
}

fn crossing(sequence: usize, detail: &str) -> crate::Error {
    invalid(format!(
        "text crosses grid boundary at sequence {sequence}: {detail}"
    ))
}

/// Lenient placement: the cell containing the run's centre, or outside.
fn centre(grid: &PdfTableGrid, cells: &Partition, b: PdfBounds) -> Placement {
    let (xs, ys) = (&grid.x_boundaries, &grid.y_boundaries);
    let x = (b.left + b.right) / 2.0;
    let y = (b.bottom + b.top) / 2.0;
    let column = xs.windows(2).position(|w| x >= w[0] && x < w[1]);
    let row = ys.windows(2).position(|w| y >= w[0] && y < w[1]);
    match (column, row) {
        (Some(column), Some(y)) => Placement::Cell {
            index: cells.at(cells.rows - 1 - y, column),
            bottom: b.bottom,
            top: b.top,
        },
        _ => Placement::Outside,
    }
}

fn place(
    grid: &PdfTableGrid,
    cells: &Partition,
    bounds: crate::PdfBounds,
    sequence: usize,
) -> Result<Placement> {
    let (xs, ys) = (&grid.x_boundaries, &grid.y_boundaries);
    let rows = cells.rows;
    let height = bounds.top - bounds.bottom;
    let middle = bounds.bottom + height / 2.0;
    let tolerance = tolerance(height);

    // Centre outside the table: tolerate only a small estimated incursion.
    if middle < ys[0] || middle > ys[rows] {
        let intrusion = (bounds.top.min(ys[rows]) - bounds.bottom.max(ys[0])).max(0.0);
        if bounds.estimated && intrusion <= tolerance {
            return Ok(Placement::Outside);
        }
        return Err(crossing(sequence, "substantially overlaps the table edge"));
    }
    let column = xs
        .windows(2)
        .position(|w| bounds.left >= w[0] && bounds.right <= w[1])
        .ok_or_else(|| crossing(sequence, "column boundary"))?;
    let index = match ys.windows(2).position(|w| middle > w[0] && middle < w[1]) {
        Some(y) => cells.at(rows - 1 - y, column),
        // Centre exactly on an interior rule: valid only when that rule is
        // absent here, i.e. both neighbouring slots belong to the same cell.
        None => {
            let boundary = ys
                .iter()
                .position(|v| *v == middle)
                .filter(|b| *b > 0 && *b < rows)
                .ok_or_else(|| crossing(sequence, "centre lies on the table edge"))?;
            let above = cells.at(rows - boundary - 1, column);
            if above != cells.at(rows - boundary, column) {
                return Err(crossing(sequence, "centre lies on a row boundary"));
            }
            above
        }
    };
    let span = &cells.spans[index];
    let bottom = ys[rows - span.row - span.row_span];
    let top = ys[rows - span.row];
    if bounds.estimated {
        let spill = (bottom - bounds.bottom).max(0.0) + (bounds.top - top).max(0.0);
        if spill > tolerance {
            return Err(crossing(sequence, "vertical spill exceeds tolerance"));
        }
        Ok(Placement::Cell {
            index,
            bottom: bounds.bottom.max(bottom),
            top: bounds.top.min(top),
        })
    } else if bounds.bottom >= bottom && bounds.top <= top {
        Ok(Placement::Cell {
            index,
            bottom: bounds.bottom,
            top: bounds.top,
        })
    } else {
        Err(crossing(sequence, "row boundary"))
    }
}

/// Convert a grid against an inspection of the same PDF.
///
/// The grid is trusted as a selection; this does not prove a real table
/// exists. With [`PdfTextAssignment::Contained`], text with unknown position
/// on the page, or text crossing a cell boundary, is an error rather than
/// being guessed into a cell.
pub fn convert_inspection(
    report: &PdfInspection,
    grid: &PdfTableGrid,
    options: &Options,
) -> Result<PdfTableConversion> {
    if grid.page == 0 || grid.page as usize > report.page_count {
        return Err(invalid("table page out of range"));
    }
    if report.elements.len() > options.limits.blocks {
        return Err(limit("inspection elements"));
    }
    let lenient = grid.text_assignment == PdfTextAssignment::Centre;
    let mut texts = Vec::new();
    for e in &report.elements {
        if e.anchor.page != grid.page || e.kind != PdfElementKind::Text {
            continue;
        }
        let Some(bounds) = e.bounds else {
            if lenient {
                continue;
            }
            return Err(invalid("unpositioned text on selected page"));
        };
        texts.push(TextRun {
            bounds,
            text: e.text.clone(),
            gap: e.word_gap_before,
            sequence: e.sequence,
        });
    }
    convert_runs(
        &report.source.name,
        &report.source.sha256,
        &texts,
        grid,
        options,
    )
}

/// One positioned text run (page coordinates) for [`convert_runs`].
pub(super) struct TextRun {
    pub bounds: PdfBounds,
    pub text: Option<String>,
    pub gap: bool,
    pub sequence: usize,
}

/// Shared conversion for inspected and OCR text.
pub(super) fn convert_runs(
    source_name: &str,
    source_sha256: &str,
    texts: &[TextRun],
    grid: &PdfTableGrid,
    options: &Options,
) -> Result<PdfTableConversion> {
    let (columns, rows) = grid.dimensions()?;
    if !matches!(grid.rotation, 0 | 90 | 270) {
        return Err(invalid("table rotation must be 0, 90 or 270"));
    }
    if columns * rows > options.limits.cells {
        return Err(limit("table cells"));
    }
    let cells = Partition::new(&grid.cell_spans, columns, rows)?;
    let (xs, ys) = (&grid.x_boundaries, &grid.y_boundaries);
    let byte_limit = options.limits.archive_bytes.min(MAX_TEXT_BYTES);
    let lenient = grid.text_assignment == PdfTextAssignment::Centre;

    let mut runs: Vec<Vec<Run>> = vec![Vec::new(); cells.spans.len()];
    let mut sequences = Vec::new();
    let mut seen = BTreeSet::new();
    let (mut bytes, mut adjusted, mut excluded) = (0u64, 0usize, 0usize);
    for t in texts {
        let page = t.bounds;
        if [page.left, page.bottom, page.right, page.top]
            .iter()
            .any(|n| !n.is_finite())
            || page.left > page.right
            || page.bottom > page.top
            || (!lenient && (page.left == page.right || page.bottom == page.top))
        {
            return Err(invalid("invalid text geometry"));
        }
        let [left, bottom, right, top] = frame::to_frame(
            [page.left, page.bottom, page.right, page.top],
            grid.rotation,
        );
        let b = PdfBounds {
            left,
            bottom,
            right,
            top,
            estimated: page.estimated,
        };
        if b.right <= xs[0] || b.left >= xs[columns] || b.top <= ys[0] || b.bottom >= ys[rows] {
            continue;
        }
        let Some(text) = t.text.as_ref() else {
            if lenient {
                continue;
            }
            return Err(invalid("undecodable text in table"));
        };
        if text.trim().is_empty() {
            continue;
        }
        let placement = if lenient {
            centre(grid, &cells, b)
        } else {
            place(grid, &cells, b, t.sequence)?
        };
        let (index, bottom, top) = match placement {
            Placement::Outside => {
                excluded += 1;
                continue;
            }
            Placement::Cell { index, bottom, top } => (index, bottom, top),
        };
        if !lenient && (bottom, top) != (b.bottom, b.top) {
            adjusted += 1;
        }
        bytes = bytes.saturating_add(text.len() as u64);
        if bytes > byte_limit {
            return Err(limit("table text bytes"));
        }
        if !seen.insert(t.sequence) {
            return Err(invalid("duplicate source sequence in inspection"));
        }
        sequences.push(t.sequence);
        runs[index].push(Run {
            bounds: [b.left, bottom, b.right, top],
            text: text.clone(),
            gap: t.gap,
            sequence: t.sequence,
        });
    }
    if sequences.is_empty() {
        return Err(invalid("selected grid has no decoded text"));
    }

    let mut rendered = vec![vec![String::new(); columns]; rows];
    for (span, cell) in cells.spans.iter().zip(&runs) {
        rendered[span.row][span.column] = render_cell(cell)?;
    }
    let markdown = markdown(&rendered);
    if markdown.len() as u64 > options.limits.archive_bytes {
        return Err(limit("table Markdown bytes"));
    }

    let mut warnings = vec![
        "Text bounds are estimated; coordinates are unnormalized PDF user space; \
         the first row is rendered as the Markdown header"
            .to_string(),
    ];
    if !grid.cell_spans.is_empty() {
        warnings.push(
            "Merged cells are written at their top-left slot; covered slots stay empty \
             (see grid.cell_spans)"
                .into(),
        );
    }
    if lenient {
        warnings.push(
            "Boundaries are inferred from layout; each text run is placed by its centre \
             and runs centred outside the grid are skipped"
                .into(),
        );
    } else if adjusted > 0 || excluded > 0 {
        warnings.push(format!(
            "Vertical tolerance applied: {adjusted} text runs clamped to their cell, \
             {excluded} small incursions from outside ignored"
        ));
    }
    Ok(PdfTableConversion {
        schema_version: TABLE_SCHEMA.into(),
        source_name: source_name.into(),
        source_sha256: source_sha256.into(),
        grid: grid.clone(),
        source_sequences: sequences,
        first_row_is_header: true,
        markdown,
        warnings,
    })
}

/// Render one cell: lines joined by `<br>`, Markdown/HTML-sensitive
/// characters escaped as entities.
fn render_cell(runs: &[Run]) -> Result<String> {
    let refs: Vec<&Run> = runs.iter().collect();
    let mut text = String::new();
    for (line_index, line) in cell_lines::ordered(&refs)?.iter().enumerate() {
        if line_index > 0 {
            text.push_str("<br>");
        }
        let mut previous: Option<&Run> = None;
        for run in line.iter().copied() {
            if previous.is_some_and(|last| run.gap || run.bounds[0] - last.bounds[2] > 1.0) {
                text.push(' ');
            }
            escape_into(&mut text, &run.text);
            previous = Some(run);
        }
    }
    Ok(text)
}

fn escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '|' => out.push_str("&#124;"),
            '\\' => out.push_str("&#92;"),
            '*' => out.push_str("&#42;"),
            '_' => out.push_str("&#95;"),
            '`' => out.push_str("&#96;"),
            '[' => out.push_str("&#91;"),
            ']' => out.push_str("&#93;"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{test_report, test_text};
    use super::*;
    use crate::Error;

    fn grid(xs: &[f64], ys: &[f64]) -> PdfTableGrid {
        PdfTableGrid {
            page: 1,
            x_boundaries: xs.to_vec(),
            y_boundaries: ys.to_vec(),
            cell_spans: Vec::new(),
            rotation: 0,
            text_assignment: Default::default(),
        }
    }

    fn convert(grid: &PdfTableGrid, texts: &[(&str, [f64; 4])]) -> Result<PdfTableConversion> {
        let elements: Vec<_> = texts
            .iter()
            .enumerate()
            .map(|(i, (text, bounds))| test_text(i + 1, text, *bounds))
            .collect();
        convert_inspection(
            &test_report(serde_json::Value::Array(elements)),
            grid,
            &Options::default(),
        )
    }

    #[test]
    fn reconstructs_top_down_and_preserves_empty_cells() {
        let texts = [
            ("Header", [1., 11., 9., 19.]),
            ("Value", [11., 1., 19., 9.]),
        ];
        let result = convert(&grid(&[0., 10., 20.], &[0., 10., 20.]), &texts).unwrap();
        assert_eq!(
            result.markdown,
            "| Header |  |\n| --- | --- |\n|  | Value |\n"
        );
        assert_eq!(result.source_sequences, vec![1, 2]);
    }

    #[test]
    fn rejects_crossing_text_and_invalid_grids() {
        let crossing = convert(
            &grid(&[0., 10., 20.], &[0., 10.]),
            &[("bad", [9., 1., 11., 9.])],
        );
        assert!(
            matches!(crossing, Err(Error::InvalidInput(m)) if m.contains("crosses grid boundary"))
        );
        assert!(convert(&grid(&[0., 0.], &[0., 10.]), &[]).is_err());
        assert!(convert(&grid(&[0., f64::NAN], &[0., 10.]), &[]).is_err());
    }

    #[test]
    fn escapes_markdown_and_html() {
        let result = convert(
            &grid(&[0., 10.], &[0., 10.]),
            &[("a|<b>&_", [1., 1., 9., 9.])],
        );
        assert!(
            result
                .unwrap()
                .markdown
                .contains("a&#124;&lt;b&gt;&amp;&#95;")
        );
    }

    #[test]
    fn joins_fragments_and_multiple_lines() {
        let texts = [
            ("A", [1., 11., 3., 19.]),
            ("B", [5., 11., 8., 19.]),
            ("C", [1., 1., 8., 9.]),
        ];
        let result = convert(&grid(&[0., 10.], &[0., 20.]), &texts).unwrap();
        assert!(result.markdown.contains("A B<br>C"));
    }

    #[test]
    fn typed_result_retains_provenance_and_serializes() {
        let result = convert(&grid(&[0., 10.], &[0., 10.]), &[("Cell", [1., 1., 9., 9.])]).unwrap();
        assert_eq!(result.source_sha256, "test-hash");
        assert_eq!(result.markdown, "| Cell |\n| --- |\n");
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("cell_spans"));
        let round: PdfTableConversion = serde_json::from_str(&json).unwrap();
        assert_eq!(result, round);
    }

    #[test]
    fn enforces_page_and_cell_limit() {
        let report = test_report(serde_json::json!([test_text(1, "Cell", [1., 1., 9., 9.])]));
        let mut g = grid(&[0., 10.], &[0., 10.]);
        g.page = 0;
        assert!(convert_inspection(&report, &g, &Options::default()).is_err());
        g.page = 1;
        let mut options = Options::default();
        options.limits.cells = 0;
        assert!(matches!(
            convert_inspection(&report, &g, &options),
            Err(Error::LimitExceeded(_))
        ));
    }

    #[test]
    fn heading_overhang_is_clamped_within_tolerance() {
        let ys = [636.4686618993976, 650.3226660120883];
        let texts = [("Heading", [1., 639.7501144371943, 9., 651.4168149742009])];
        let result = convert(&grid(&[0., 10.], &ys), &texts).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("1 text runs clamped"))
        );
    }

    #[test]
    fn small_incursion_from_adjacent_band_is_ignored() {
        let ys = [473.13565165304607, 500.1144604580768];
        let texts = [
            ("Above", [1., 480., 9., 490.]),
            ("Below", [1., 462.56299517397565, 9., 474.22969571098224]),
        ];
        let result = convert(&grid(&[0., 10.], &ys), &texts).unwrap();
        assert_eq!(result.source_sequences, vec![1]);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("1 small incursions"))
        );
    }

    #[test]
    fn large_or_ambiguous_vertical_crossings_fail() {
        let two_rows = grid(&[0., 10.], &[0., 10., 20.]);
        let one_row = grid(&[0., 10.], &[0., 10.]);
        assert!(convert(&two_rows, &[("x", [1., 5., 9., 15.])]).is_err());
        assert!(convert(&one_row, &[("x", [1., 1., 9., 13.])]).is_err());
        assert!(convert(&one_row, &[("x", [1., -8., 9., 3.])]).is_err());
        assert!(convert(&two_rows, &[("x", [1., 9.9, 9., 10.1])]).is_err());
    }

    #[test]
    fn tolerance_applies_to_estimated_bounds_only() {
        let mut report = test_report(serde_json::json!([test_text(
            7,
            "Cell",
            [1., 1., 9., 10.5]
        )]));
        let g = grid(&[0., 10.], &[0., 10.]);
        assert!(convert_inspection(&report, &g, &Options::default()).is_ok());
        assert_eq!(report.elements[0].bounds.unwrap().top, 10.5);
        report.elements[0].bounds.as_mut().unwrap().estimated = false;
        assert!(convert_inspection(&report, &g, &Options::default()).is_err());
    }

    #[test]
    fn tolerance_does_not_relax_column_containment() {
        assert!(
            convert(
                &grid(&[0., 10., 20.], &[0., 10.]),
                &[("x", [9., 1., 11., 9.])]
            )
            .is_err()
        );
    }

    fn spanned() -> PdfTableGrid {
        // Four columns; first column has a two-row span in rows 1..=2.
        let mut g = grid(&[0., 20., 50., 70., 100.], &[0., 20., 40., 60., 80.]);
        for row in 0..4 {
            for column in 0..4 {
                if column == 0 && row == 2 {
                    continue;
                }
                let row_span = if column == 0 && row == 1 { 2 } else { 1 };
                g.cell_spans.push(PdfTableSpan {
                    row,
                    column,
                    row_span,
                    column_span: 1,
                });
            }
        }
        g
    }

    #[test]
    fn text_across_absent_row_line_has_one_owner() {
        let texts = [
            ("Group", [1., 45., 15., 55.]),
            ("continued", [1., 35., 15., 45.]),
            ("A", [21., 45., 30., 55.]),
            ("B", [21., 25., 30., 35.]),
        ];
        let result = convert(&spanned(), &texts).unwrap();
        assert!(result.markdown.contains("Group<br>continued"));
        assert_eq!(result.markdown.matches("continued").count(), 1);
        assert_eq!(result.source_sequences, vec![1, 2, 3, 4]);
        assert!(result.warnings.iter().any(|w| w.contains("Merged cells")));
    }

    #[test]
    fn partition_rejects_overlap_and_gaps() {
        let mut g = spanned();
        g.cell_spans.push(g.cell_spans[0].clone());
        assert!(Partition::new(&g.cell_spans, 4, 4).is_err());
        g.cell_spans.pop();
        g.cell_spans.remove(0);
        assert!(Partition::new(&g.cell_spans, 4, 4).is_err());
        assert!(Partition::new(&spanned().cell_spans, 4, 4).is_ok());
    }
}
