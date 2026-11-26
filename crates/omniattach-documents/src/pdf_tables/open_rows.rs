//! Fallback for filled-rule tables without outer vertical borders.
//!
//! Needs at least three horizontal rules of equal extent and two interior
//! dividers covering the full height. A horizontal rule missing inside one
//! column produces a vertical row span (merged cell) there.
use super::filled::{JOIN, Line, horizontal_groups, last, sort_top_down};
use super::{
    MAX_BOUNDARIES, MAX_CANDIDATES, PdfTableGrid, PdfTableSpan, dimensions, invalid, limit,
};
use crate::{Options, Result};

const MAX_GROUPS: usize = 128;
const MAX_WORK: usize = 4_000_000;

/// Interval coverage of `lo..hi` by rulings on one axis line: `Some(true)`
/// fully covered, `Some(false)` absent, `None` partially covered (ambiguous).
fn coverage(
    lines: &[Line],
    horizontal: bool,
    axis: f64,
    lo: f64,
    hi: f64,
    work: &mut usize,
) -> Result<Option<bool>> {
    *work = work.saturating_add(lines.len());
    if *work > MAX_WORK {
        return Err(limit("open table geometry work"));
    }
    let mut intervals: Vec<(f64, f64)> = lines
        .iter()
        .filter(|l| {
            l.horizontal == horizontal
                && (l.axis - axis).abs() <= JOIN
                && l.hi > lo + JOIN
                && l.lo < hi - JOIN
        })
        .map(|l| (l.lo, l.hi))
        .collect();
    if intervals.is_empty() {
        return Ok(Some(false));
    }
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = lo;
    for (a, b) in intervals {
        if a > end + 2. * JOIN {
            return Ok(None);
        }
        end = end.max(b);
    }
    Ok((end >= hi - JOIN).then_some(true))
}

pub(super) fn assemble(lines: &[Line], page: u32, options: &Options) -> Result<Vec<PdfTableGrid>> {
    let groups = horizontal_groups(lines);
    if groups.len() > MAX_GROUPS {
        return Err(limit("open table rule groups"));
    }
    let mut result: Vec<PdfTableGrid> = Vec::new();
    let mut work = 0usize;
    for hs in groups {
        if hs.len() < 3 {
            continue;
        }
        if let Some(grid) = candidate(lines, &hs, page, options, &mut work)? {
            if result
                .iter()
                .any(|c| c.x_boundaries == grid.x_boundaries && c.y_boundaries == grid.y_boundaries)
            {
                continue;
            }
            if result.len() >= MAX_CANDIDATES.min(options.limits.blocks) {
                return Err(limit("table candidates"));
            }
            result.push(grid);
        }
    }
    sort_top_down(&mut result);
    // Overlapping fallback regions are ambiguous; never emit text twice.
    for (i, a) in result.iter().enumerate() {
        for b in &result[i + 1..] {
            let overlap = a.x_boundaries[0] < last(&b.x_boundaries) - JOIN
                && b.x_boundaries[0] < last(&a.x_boundaries) - JOIN
                && a.y_boundaries[0] < last(&b.y_boundaries) - JOIN
                && b.y_boundaries[0] < last(&a.y_boundaries) - JOIN;
            if overlap {
                return Err(invalid("overlapping open-sided table regions"));
            }
        }
    }
    Ok(result)
}

fn candidate(
    lines: &[Line],
    hs: &[Line],
    page: u32,
    options: &Options,
    work: &mut usize,
) -> Result<Option<PdfTableGrid>> {
    let (left, right) = (hs[0].lo, hs[0].hi);
    let bottom = hs
        .iter()
        .map(|l| l.axis)
        .min_by(f64::total_cmp)
        .unwrap_or(0.);
    let top = hs
        .iter()
        .map(|l| l.axis)
        .max_by(f64::total_cmp)
        .unwrap_or(0.);
    let verticals: Vec<&Line> = lines
        .iter()
        .filter(|l| {
            !l.horizontal
                && l.hi > bottom + JOIN
                && l.lo < top - JOIN
                && l.axis >= left - JOIN
                && l.axis <= right + JOIN
        })
        .collect();
    // Both outer sides must really be open, not damaged closed borders.
    if verticals
        .iter()
        .any(|l| (l.axis - left).abs() <= JOIN || (l.axis - right).abs() <= JOIN)
    {
        return Ok(None);
    }
    let mut xs = vec![left];
    xs.extend(verticals.iter().map(|l| l.axis));
    xs.push(right);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() <= JOIN);
    if xs.len() < 4 || xs.len() > MAX_BOUNDARIES || xs.windows(2).any(|w| w[1] - w[0] <= 2. * JOIN)
    {
        return Ok(None);
    }
    for x in &xs[1..xs.len() - 1] {
        if coverage(lines, false, *x, bottom, top, work)? != Some(true) {
            return Ok(None);
        }
    }
    let relevant: Vec<&Line> = lines
        .iter()
        .filter(|l| {
            l.horizontal
                && l.axis >= bottom - 0.01
                && l.axis <= top + 0.01
                && l.hi > left + JOIN
                && l.lo < right - JOIN
        })
        .collect();
    let aligned = |v: f64| xs.iter().any(|x| (*x - v).abs() <= JOIN);
    if relevant
        .iter()
        .any(|l| l.lo < left - JOIN || l.hi > right + JOIN || !aligned(l.lo) || !aligned(l.hi))
    {
        return Ok(None);
    }
    let mut ys: Vec<f64> = relevant.iter().map(|l| l.axis).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() <= JOIN);
    if ys.len() < 3 || ys.len() > MAX_BOUNDARIES || ys.windows(2).any(|w| w[1] - w[0] <= 2. * JOIN)
    {
        return Ok(None);
    }
    let (columns, rows) = dimensions(&xs, &ys)?;
    if columns * rows > options.limits.cells {
        return Err(limit("table cells"));
    }
    // Walk each column top-down; a present rule closes the current cell.
    let mut spans = Vec::new();
    for column in 0..columns {
        let mut start = 0;
        for end in 1..=rows {
            match coverage(
                lines,
                true,
                ys[rows - end],
                xs[column],
                xs[column + 1],
                work,
            )? {
                Some(true) => {
                    spans.push(PdfTableSpan {
                        row: start,
                        column,
                        row_span: end - start,
                        column_span: 1,
                    });
                    start = end;
                }
                Some(false) => {}
                None => return Ok(None),
            }
        }
        if start != rows {
            return Ok(None);
        }
    }
    spans.sort_by_key(|s| (s.row, s.column));
    Ok(Some(PdfTableGrid {
        page,
        x_boundaries: xs,
        y_boundaries: ys,
        cell_spans: spans,
        rotation: 0,
        text_assignment: Default::default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Vec<Line> {
        let mut lines = Vec::new();
        for y in [0., 20., 60., 80.] {
            lines.push(Line {
                horizontal: true,
                axis: y,
                lo: 0.,
                hi: 100.,
            });
        }
        lines.push(Line {
            horizontal: true,
            axis: 40.,
            lo: 20.,
            hi: 100.,
        });
        for x in [20., 50., 70.] {
            lines.push(Line {
                horizontal: false,
                axis: x,
                lo: 0.,
                hi: 80.,
            });
        }
        lines
    }

    #[test]
    fn discovers_open_sides_and_keeps_row_span() {
        let found = assemble(&geometry(), 1, &Options::default()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].x_boundaries, vec![0., 20., 50., 70., 100.]);
        assert!(found[0].cell_spans.contains(&PdfTableSpan {
            row: 1,
            column: 0,
            row_span: 2,
            column_span: 1
        }));
    }

    #[test]
    fn missing_divider_is_not_silently_filled() {
        let mut lines = geometry();
        lines.last_mut().unwrap().hi = 40.;
        assert!(assemble(&lines, 1, &Options::default()).unwrap().is_empty());
    }

    #[test]
    fn partial_separator_is_ambiguous() {
        let mut lines = geometry();
        lines.push(Line {
            horizontal: true,
            axis: 30.,
            lo: 30.,
            hi: 100.,
        });
        assert!(assemble(&lines, 1, &Options::default()).unwrap().is_empty());
    }
}
