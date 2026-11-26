//! Tables whose rulings are thin filled rectangles (`re f`).
//!
//! Rulings are merged across paints, then consecutive rows with identical
//! column boundaries form one closed band. A full-width section heading
//! becomes its own one-column candidate. If no closed band exists, the
//! open-sided fallback in [`super::open_rows`] is tried.
use super::paths::{self, Rect, inside};
use super::{MAX_BOUNDARIES, MAX_CANDIDATES, MAX_CELLS, PdfTableGrid, invalid, limit, open_rows};
use crate::{Options, Result};
use lopdf::content::Content;

/// Join/alignment tolerance between ruling pieces (PDF units).
pub(super) const JOIN: f64 = 0.8;
/// A ruling is at most this thick and at least `MIN_ASPECT` times longer.
const MAX_THICKNESS: f64 = 2.0;
const MIN_ASPECT: f64 = 3.0;
const MAX_RULINGS: usize = 4096;

/// A horizontal or vertical ruling: `axis` is its centre line, `lo..hi` its extent.
#[derive(Clone, Copy, Debug)]
pub(super) struct Line {
    pub horizontal: bool,
    pub axis: f64,
    pub lo: f64,
    pub hi: f64,
}

/// Pages that paint rectangles with a fill operator use this detector.
pub(super) fn applies(content: &Content) -> bool {
    let has = |ops: &[&str]| {
        content
            .operations
            .iter()
            .any(|o| ops.contains(&o.operator.as_str()))
    };
    has(&["re"]) && has(&["f", "F", "f*"])
}

/// Returns grids plus whether they came from the open-sided fallback.
pub(super) fn discover(
    content: &Content,
    page: u32,
    options: &Options,
) -> Result<(Vec<PdfTableGrid>, bool)> {
    let mut lines = Vec::new();
    let clips = paths::scan(content, options, true, |paint| {
        if !matches!(paint.operator, "f" | "F" | "f*" | "n") {
            return Err(invalid("mixed stroked/filled table geometry unsupported"));
        }
        if paint.operator == "n"
            || paint.irregular
            || paint.rects.len() != 1
            || paint.segments.len() != 4
        {
            return Ok(());
        }
        let r = paint.rects[0];
        let Some(line) = ruling(r) else { return Ok(()) };
        if let Some(c) = paint.clip {
            let disjoint = r[2] <= c[0] || r[0] >= c[2] || r[3] <= c[1] || r[1] >= c[3];
            if disjoint {
                return Ok(());
            }
            if !inside(r, c) {
                return Err(invalid("clipped border intersects table geometry"));
            }
        }
        lines.push(line);
        if lines.len() > MAX_RULINGS.min(options.limits.archive_entries) {
            return Err(limit("filled border segments"));
        }
        Ok(())
    })?;
    let lines = merge(lines);
    let mut grids = closed_bands(&lines, page, options)?;
    let open = grids.is_empty();
    if open {
        grids = open_rows::assemble(&lines, page, options)?;
    }
    for g in &grids {
        let extent: Rect = [
            g.x_boundaries[0],
            g.y_boundaries[0],
            last(&g.x_boundaries),
            last(&g.y_boundaries),
        ];
        if clips.iter().any(|clip| !inside(extent, *clip)) {
            return Err(invalid(
                "table not contained in every rectangular page clip",
            ));
        }
    }
    Ok((grids, open))
}

pub(super) fn last(axis: &[f64]) -> f64 {
    axis[axis.len() - 1]
}

fn ruling(r: Rect) -> Option<Line> {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if h <= MAX_THICKNESS && w >= MIN_ASPECT * h && w > 2. * JOIN {
        Some(Line {
            horizontal: true,
            axis: (r[1] + r[3]) / 2.,
            lo: r[0],
            hi: r[2],
        })
    } else if w <= MAX_THICKNESS && h >= MIN_ASPECT * w && h > 2. * JOIN {
        Some(Line {
            horizontal: false,
            axis: (r[0] + r[2]) / 2.,
            lo: r[1],
            hi: r[3],
        })
    } else {
        None
    }
}

/// Merge collinear pieces whose axes and ends are within `JOIN`.
fn merge(mut lines: Vec<Line>) -> Vec<Line> {
    lines.sort_by(|a, b| {
        a.horizontal
            .cmp(&b.horizontal)
            .then(a.axis.total_cmp(&b.axis))
            .then(a.lo.total_cmp(&b.lo))
    });
    let mut groups: Vec<Vec<Line>> = Vec::new();
    for l in lines {
        match groups.last_mut() {
            Some(g) if g[0].horizontal == l.horizontal && (g[0].axis - l.axis).abs() <= JOIN => {
                g.push(l)
            }
            _ => groups.push(vec![l]),
        }
    }
    let mut result = Vec::new();
    for mut g in groups {
        let axis = g.iter().map(|l| l.axis).sum::<f64>() / g.len() as f64;
        g.sort_by(|a, b| a.lo.total_cmp(&b.lo));
        let mut current = Line { axis, ..g[0] };
        for l in g.into_iter().skip(1) {
            if l.lo <= current.hi + JOIN {
                current.hi = current.hi.max(l.hi);
            } else {
                result.push(current);
                current = Line { axis, ..l };
            }
        }
        result.push(current);
    }
    result
}

/// Group horizontal rulings that share the same left/right extent.
pub(super) fn horizontal_groups(lines: &[Line]) -> Vec<Vec<Line>> {
    let mut groups: Vec<Vec<Line>> = Vec::new();
    for h in lines.iter().filter(|l| l.horizontal) {
        match groups
            .iter_mut()
            .find(|g| (g[0].lo - h.lo).abs() <= JOIN && (g[0].hi - h.hi).abs() <= JOIN)
        {
            Some(g) => g.push(*h),
            None => groups.push(vec![*h]),
        }
    }
    groups
}

fn closed_bands(lines: &[Line], page: u32, options: &Options) -> Result<Vec<PdfTableGrid>> {
    let mut output = Vec::new();
    let push = |pending: &mut Option<(Vec<f64>, Vec<f64>)>,
                output: &mut Vec<PdfTableGrid>|
     -> Result<()> {
        if let Some((xs, ys)) = pending.take() {
            if output.len() >= MAX_CANDIDATES.min(options.limits.blocks) {
                return Err(limit("table candidates"));
            }
            if (xs.len() - 1) * (ys.len() - 1) > options.limits.cells.min(MAX_CELLS) {
                return Err(limit("table cells"));
            }
            output.push(PdfTableGrid {
                page,
                x_boundaries: xs,
                y_boundaries: ys,
                cell_spans: Vec::new(),
            });
        }
        Ok(())
    };
    for mut hs in horizontal_groups(lines) {
        if hs.len() < 2 {
            continue;
        }
        if hs.len() > MAX_BOUNDARIES {
            return Err(limit("filled table rows"));
        }
        hs.sort_by(|a, b| a.axis.total_cmp(&b.axis));
        let (left, right) = (hs[0].lo, hs[0].hi);
        let mut pending: Option<(Vec<f64>, Vec<f64>)> = None;
        for pair in hs.windows(2) {
            let (bottom, top) = (pair[0].axis, pair[1].axis);
            if top - bottom <= 2. * JOIN {
                push(&mut pending, &mut output)?;
                continue;
            }
            let verticals: Vec<&Line> = lines
                .iter()
                .filter(|l| {
                    !l.horizontal
                        && l.axis >= left - JOIN
                        && l.axis <= right + JOIN
                        && l.hi > bottom + JOIN
                        && l.lo < top - JOIN
                })
                .collect();
            let partial = verticals
                .iter()
                .any(|l| l.lo > bottom + JOIN || l.hi < top - JOIN);
            let mut xs: Vec<f64> = verticals.iter().map(|l| l.axis).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|a, b| (*a - *b).abs() <= JOIN);
            if partial
                || xs.len() < 2
                || (xs[0] - left).abs() > JOIN
                || (last(&xs) - right).abs() > JOIN
            {
                push(&mut pending, &mut output)?;
                continue;
            }
            if xs.len() > MAX_BOUNDARIES || xs.windows(2).any(|w| w[1] - w[0] <= 2. * JOIN) {
                return Err(invalid("ambiguous filled table columns"));
            }
            if let Some((old, ys)) = &mut pending
                && *old == xs
                && (last(ys) - bottom).abs() < 0.01
            {
                ys.push(top);
                continue;
            }
            push(&mut pending, &mut output)?;
            pending = Some((xs, vec![bottom, top]));
        }
        push(&mut pending, &mut output)?;
    }
    sort_top_down(&mut output);
    Ok(output)
}

/// Order candidates top to bottom, then left to right.
pub(super) fn sort_top_down(grids: &mut [PdfTableGrid]) {
    grids.sort_by(|a, b| {
        last(&b.y_boundaries)
            .total_cmp(&last(&a.y_boundaries))
            .then(a.x_boundaries[0].total_cmp(&b.x_boundaries[0]))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> String {
        let mut s = String::from("0 0 100 100 re W n ");
        for y in [10, 30, 50, 70] {
            s += &format!("10 {y} 60 0.5 re f ");
        }
        for x in [10, 70] {
            s += &format!("{x} 10 0.5 60.5 re f ");
        }
        s + "40 10 0.5 40 re f "
    }

    fn find(text: &str, options: &Options) -> Result<Vec<PdfTableGrid>> {
        discover(&Content::decode(text.as_bytes()).unwrap(), 1, options).map(|(g, _)| g)
    }

    #[test]
    fn splits_section_heading_without_inventing_divider() {
        let g = find(&filled(), &Options::default()).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!((g[0].x_boundaries.len(), g[0].y_boundaries.len()), (2, 2));
        assert_eq!((g[1].x_boundaries.len(), g[1].y_boundaries.len()), (3, 3));
    }

    #[test]
    fn rejects_cut_border_complex_clip_and_mixed_strokes() {
        for text in [
            filled().replace("0 0 100 100", "20 0 80 100"),
            filled().replace("0 0 100 100 re W", "0 0 m 50 0 l 25 100 l h W"),
            filled() + "0 0 m 10 10 l S",
        ] {
            assert!(find(&text, &Options::default()).is_err(), "{text}");
        }
    }

    #[test]
    fn duplicate_rectangles_do_not_duplicate_tables() {
        let s = filled();
        let twice = s.clone() + &s;
        assert_eq!(
            find(&s, &Options::default()).unwrap(),
            find(&twice, &Options::default()).unwrap()
        );
    }

    #[test]
    fn honors_limits() {
        let mut o = Options::default();
        o.limits.cells = 0;
        assert!(matches!(
            find(&filled(), &o),
            Err(crate::Error::LimitExceeded(_))
        ));
    }
}
