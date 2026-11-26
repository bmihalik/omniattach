//! Closed grids drawn as one stroked path (`S`/`s`) of full-span rulings.
use super::paths::{self, Segment};
use super::{MAX_BOUNDARIES, MAX_CANDIDATES, MIN_BOUNDARY_GAP, PdfTableGrid, limit};
use crate::{Options, Result};
use lopdf::content::Content;

/// Coordinate matching tolerance (PDF units).
const EPS: f64 = 0.01;

/// Grids in paint order. Clipping is rejected in this mode.
pub(super) fn discover(
    content: &Content,
    page: u32,
    options: &Options,
) -> Result<Vec<PdfTableGrid>> {
    let mut grids = Vec::new();
    paths::scan(content, options, false, |paint| {
        if paint.irregular || !matches!(paint.operator, "S" | "s") {
            return Ok(());
        }
        if let Some((xs, ys)) = grid(paint.segments) {
            if grids.len() >= MAX_CANDIDATES.min(options.limits.blocks) {
                return Err(limit("table candidates"));
            }
            grids.push(PdfTableGrid {
                page,
                x_boundaries: xs,
                y_boundaries: ys,
                cell_spans: Vec::new(),
            });
        }
        Ok(())
    })?;
    Ok(grids)
}

/// Every segment must be a horizontal or vertical ruling spanning the whole
/// path extent; at least three rulings per axis, including the borders.
fn grid(segments: &[Segment]) -> Option<(Vec<f64>, Vec<f64>)> {
    let points = || segments.iter().flat_map(|(a, b)| [a, b]);
    let left = points().map(|p| p[0]).min_by(f64::total_cmp)?;
    let right = points().map(|p| p[0]).max_by(f64::total_cmp)?;
    let bottom = points().map(|p| p[1]).min_by(f64::total_cmp)?;
    let top = points().map(|p| p[1]).max_by(f64::total_cmp)?;
    if right - left <= MIN_BOUNDARY_GAP || top - bottom <= MIN_BOUNDARY_GAP {
        return None;
    }
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (a, b) in segments {
        let spans = |lo: f64, hi: f64, min: f64, max: f64| {
            (lo.min(hi) - min).abs() <= EPS && (lo.max(hi) - max).abs() <= EPS
        };
        if (a[1] - b[1]).abs() <= EPS && spans(a[0], b[0], left, right) {
            ys.push(a[1]);
        } else if (a[0] - b[0]).abs() <= EPS && spans(a[1], b[1], bottom, top) {
            xs.push(a[0]);
        } else {
            return None;
        }
    }
    for axis in [&mut xs, &mut ys] {
        axis.sort_by(f64::total_cmp);
        axis.dedup_by(|a, b| (*a - *b).abs() <= EPS);
        if axis.len() < 3
            || axis.len() > MAX_BOUNDARIES
            || axis.windows(2).any(|w| w[1] - w[0] <= MIN_BOUNDARY_GAP)
        {
            return None;
        }
    }
    let edge = |axis: &[f64], lo: f64, hi: f64| {
        (axis[0] - lo).abs() <= EPS && (axis[axis.len() - 1] - hi).abs() <= EPS
    };
    (edge(&xs, left, right) && edge(&ys, bottom, top)).then_some((xs, ys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(data: &[u8]) -> Result<Vec<PdfTableGrid>> {
        discover(&Content::decode(data).unwrap(), 1, &Options::default())
    }

    #[test]
    fn transforms_grid() {
        let g = find(b"2 0 0 2 5 7 cm 0 0 m 20 0 l 0 10 m 20 10 l 0 20 m 20 20 l 0 0 m 0 20 l 10 0 m 10 20 l 20 0 m 20 20 l S").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].x_boundaries, vec![5., 25., 45.]);
        assert_eq!(g[0].y_boundaries, vec![7., 27., 47.]);
    }

    #[test]
    fn rectangles_form_a_grid() {
        let g = find(b"0 0 20 20 re 10 0 m 10 20 l 0 10 m 20 10 l S").unwrap();
        assert_eq!(g[0].x_boundaries, vec![0., 10., 20.]);
    }

    #[test]
    fn unsupported_content_fails() {
        assert!(find(b"/F Do").is_err());
        assert!(find(b"W n").is_err());
    }

    #[test]
    fn separate_strokes_partial_rulings_and_curves_are_ignored() {
        assert!(find(b"0 0 m 20 0 l S 0 10 m 20 10 l S").unwrap().is_empty());
        assert!(
            find(b"0 0 20 20 re 10 0 m 10 15 l 0 10 m 20 10 l S")
                .unwrap()
                .is_empty()
        );
        assert!(
            find(b"0 0 20 20 re 10 0 m 10 20 l 0 10 m 20 10 l 0 0 5 5 0 0 c S")
                .unwrap()
                .is_empty()
        );
    }
}
