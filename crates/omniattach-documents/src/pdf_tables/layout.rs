//! Layout detector: tables bounded by horizontal rules, with columns from
//! vertical rules or text alignment and rows from rules and text lines.
//!
//! It is the fallback for pages the strict detectors cannot read (booktabs
//! tables, open sides, mixed or clipped painting, shaded rows, rotated pages
//! and OCR). It works in an abstract upright frame: callers rotate rules and
//! text boxes first (see [`super::frame`]). Every threshold is relative to
//! the median text height `h` of the page.
use super::rules::Rule;
use super::{MAX_BOUNDARIES, MIN_BOUNDARY_GAP};

/// Text box `[left, bottom, right, top]` in the frame.
pub(super) type Box4 = [f64; 4];

/// A detected grid in the frame; boundaries ascending.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Table {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

/// Text taller than this many `h` (e.g. perpendicular text) does not form lines.
const TALL: f64 = 2.5;
/// Horizontal gap, in `h`, that separates phrases: evidence of a table.
const COLUMN_GAP: f64 = 0.75;
/// Smaller gap, in `h`, that may still separate dense (numeric) columns.
const TIGHT_GAP: f64 = 0.5;
/// Detector tuning that differs between native text and OCR words.
#[derive(Clone, Copy)]
pub(super) struct Params {
    /// Vertical gap between lines, in `h`, that starts a new row. OCR word
    /// boxes hug the glyphs, so their gaps look larger than font boxes'.
    pub row_gap: f64,
}

pub(super) const NATIVE: Params = Params { row_gap: 0.7 };
pub(super) const OCR: Params = Params { row_gap: 1.2 };
/// Rules of one table share their extent at least this much.
const SAME_EXTENT: f64 = 0.9;

struct Line {
    bottom: f64,
    top: f64,
    /// Phrases `[left, right]`, sorted.
    phrases: Vec<[f64; 2]>,
    /// Like `phrases`, split at [`TIGHT_GAP`].
    pieces: Vec<[f64; 2]>,
}

struct Band {
    top: f64,
    bottom: f64,
    lines: Vec<Line>,
    tabular: bool,
}

/// Find tables in one frame.
pub(super) fn tables(rules: &[Rule], boxes: &[Box4], params: Params) -> Vec<Table> {
    let boxes: Vec<Box4> = boxes
        .iter()
        .copied()
        .filter(|b| b.iter().all(|n| n.is_finite()) && b[2] > b[0] && b[3] > b[1])
        .collect();
    let Some(h) = median(boxes.iter().map(|b| b[3] - b[1])) else {
        return Vec::new();
    };
    let normal: Vec<Box4> = boxes
        .iter()
        .copied()
        .filter(|b| b[3] - b[1] <= TALL * h)
        .collect();
    let horizontal: Vec<Rule> = rules
        .iter()
        .copied()
        .filter(|r| r.horizontal && r.len() >= (3.0 * h).max(20.0))
        .collect();
    let vertical: Vec<Rule> = rules
        .iter()
        .copied()
        .filter(|r| !r.horizontal && r.len() >= h)
        .collect();

    let mut out: Vec<Table> = Vec::new();
    for mut group in groups(&horizontal) {
        let left = group.iter().map(|r| r.from).fold(f64::INFINITY, f64::min);
        let right = group.iter().map(|r| r.to).fold(f64::NEG_INFINITY, f64::max);
        extend_open_ends(&mut group, &vertical, left, right, h);
        let bands = bands(&group, left, right, &normal, &vertical, h);
        for segment in segments(bands) {
            if let Some(t) = table(&segment, left, right, &normal, &vertical, h, params)
                && !out.iter().any(|o| overlaps(o, &t))
            {
                out.push(t);
            }
        }
    }
    out.sort_by(|a, b| b.ys[b.ys.len() - 1].total_cmp(&a.ys[a.ys.len() - 1]));
    out
}

fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut v: Vec<f64> = values.filter(|n| n.is_finite() && *n > 0.0).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

fn overlaps(a: &Table, b: &Table) -> bool {
    let (ax, ay, bx, by) = (&a.xs, &a.ys, &b.xs, &b.ys);
    ax[0] < bx[bx.len() - 1]
        && bx[0] < ax[ax.len() - 1]
        && ay[0] < by[by.len() - 1]
        && by[0] < ay[ay.len() - 1]
}

/// A table cut by the page edge (or a clip) has column rules running past
/// its last horizontal rule: close it where those rules end.
fn extend_open_ends(group: &mut Vec<Rule>, vertical: &[Rule], left: f64, right: f64, h: f64) {
    let (top, bottom) = (group[0].at, group[group.len() - 1].at);
    let inside: Vec<&Rule> = vertical
        .iter()
        .filter(|v| v.at >= left - 1.0 && v.at <= right + 1.0 && v.to > bottom && v.from < top)
        .collect();
    let rule = |at| Rule {
        horizontal: true,
        at,
        from: left,
        to: right,
    };
    let below = inside
        .iter()
        .filter(|v| v.from < bottom - h)
        .map(|v| v.from)
        .fold(f64::INFINITY, f64::min);
    let above = inside
        .iter()
        .filter(|v| v.to > top + h)
        .map(|v| v.to)
        .fold(f64::NEG_INFINITY, f64::max);
    // Require at least two column rules to agree, so one stray line does not.
    let agree = |f: &dyn Fn(&Rule) -> bool| inside.iter().filter(|v| f(v)).count() >= 2;
    if below.is_finite() && agree(&|v| v.from < bottom - h) {
        group.push(rule(below));
    }
    if above.is_finite() && agree(&|v| v.to > top + h) {
        group.insert(0, rule(above));
    }
}

/// Horizontal rules sharing an extent, each group sorted top-down.
fn groups(rules: &[Rule]) -> Vec<Vec<Rule>> {
    let mut sorted = rules.to_vec();
    sorted.sort_by(|a, b| b.len().total_cmp(&a.len()));
    let mut groups: Vec<(Rule, Vec<Rule>)> = Vec::new();
    for r in sorted {
        let found = groups.iter_mut().find(|(reference, _)| {
            let overlap = r.to.min(reference.to) - r.from.max(reference.from);
            overlap >= SAME_EXTENT * reference.len() && r.len() <= reference.len() / SAME_EXTENT
        });
        match found {
            Some((_, members)) => members.push(r),
            None => groups.push((r, vec![r])),
        }
    }
    let mut groups = groups
        .into_iter()
        .map(|(_, mut members)| {
            members.sort_by(|a, b| b.at.total_cmp(&a.at));
            members.dedup_by(|a, b| (a.at - b.at).abs() <= MIN_BOUNDARY_GAP);
            members
        })
        .filter(|m| m.len() >= 2)
        .collect::<Vec<_>>();
    // Many rules sharing an extent are the strongest table evidence.
    groups.sort_by_key(|m| std::cmp::Reverse(m.len()));
    groups
}

fn centre(b: &Box4) -> [f64; 2] {
    [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
}

/// Cluster boxes into text lines (top-down) with merged phrases.
fn lines(boxes: &[Box4], h: f64) -> Vec<Line> {
    let mut sorted = boxes.to_vec();
    sorted.sort_by(|a, b| b[3].total_cmp(&a[3]));
    let mut lines: Vec<(f64, f64, Vec<Box4>)> = Vec::new();
    for b in sorted {
        let height = b[3] - b[1];
        let found = lines.iter_mut().rev().take(8).find(|(bottom, top, _)| {
            let overlap = top.min(b[3]) - bottom.max(b[1]);
            overlap >= 0.5 * height.min(top - bottom)
        });
        match found {
            Some((bottom, top, members)) => {
                *bottom = bottom.min(b[1]);
                *top = top.max(b[3]);
                members.push(b);
            }
            None => lines.push((b[1], b[3], vec![b])),
        }
    }
    let mut out: Vec<Line> = lines
        .into_iter()
        .map(|(bottom, top, mut members)| {
            members.sort_by(|a, b| a[0].total_cmp(&b[0]));
            let split = |gap: f64| {
                let mut out: Vec<[f64; 2]> = Vec::new();
                for m in &members {
                    match out.last_mut() {
                        Some(p) if m[0] - p[1] < gap * h => p[1] = p[1].max(m[2]),
                        _ => out.push([m[0], m[2]]),
                    }
                }
                out
            };
            Line {
                bottom,
                top,
                phrases: split(COLUMN_GAP),
                pieces: split(TIGHT_GAP),
            }
        })
        .collect();
    out.sort_by(|a, b| b.top.total_cmp(&a.top));
    out
}

fn crossed(vertical: &[Rule], left: f64, right: f64, bottom: f64, top: f64, h: f64) -> bool {
    vertical.iter().any(|v| {
        v.at > left + h
            && v.at < right - h
            && v.to.min(top) - v.from.max(bottom) >= 0.5 * (top - bottom)
    })
}

fn bands(
    group: &[Rule],
    left: f64,
    right: f64,
    boxes: &[Box4],
    vertical: &[Rule],
    h: f64,
) -> Vec<Band> {
    group
        .windows(2)
        .map(|w| {
            let (top, bottom) = (w[0].at, w[1].at);
            let inside: Vec<Box4> = boxes
                .iter()
                .copied()
                .filter(|b| {
                    let [x, y] = centre(b);
                    x >= left && x <= right && y > bottom && y < top
                })
                .collect();
            let lines = lines(&inside, h);
            // Multi-column prose: every phrase of most lines is wide.
            let wide = |l: &Line| {
                l.phrases
                    .iter()
                    .all(|p| p[1] - p[0] >= 0.3 * (right - left))
            };
            let prose =
                lines.len() >= 5 && lines.iter().filter(|l| wide(l)).count() * 5 >= lines.len() * 4;
            let tabular = !prose && lines.iter().any(|l| l.phrases.len() >= 2)
                || crossed(vertical, left, right, bottom, top, h);
            Band {
                top,
                bottom,
                lines,
                tabular,
            }
        })
        .collect()
}

/// Split at prose between rules, then trim untabular edge bands.
fn segments(bands: Vec<Band>) -> Vec<Vec<Band>> {
    let mut out: Vec<Vec<Band>> = vec![Vec::new()];
    for band in bands {
        if !band.tabular && band.lines.len() >= 2 {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(band);
        }
    }
    out.into_iter()
        .filter_map(|mut s| {
            // A one-line edge band (typically the header) may use tighter gaps.
            let keep = |b: &Band| b.tabular || (b.lines.len() == 1 && b.lines[0].pieces.len() >= 2);
            let core = s.iter().filter(|b| b.tabular).count();
            while s.first().is_some_and(|b| !keep(b) || core == 0) {
                s.remove(0);
            }
            while s.last().is_some_and(|b| !keep(b)) {
                s.pop();
            }
            (!s.is_empty()).then_some(s)
        })
        .collect()
}

fn table(
    mut segment: &[Band],
    left: f64,
    right: f64,
    boxes: &[Box4],
    vertical: &[Rule],
    h: f64,
    params: Params,
) -> Option<Table> {
    // A caption or note between rules is one line crossing the columns.
    let (mut xs, mut ruled) = columns(segment, left, right, vertical, h)?;
    while segment.len() > 1 {
        if caption(&segment[0], &xs, h) {
            segment = &segment[1..];
        } else if caption(&segment[segment.len() - 1], &xs, h) {
            segment = &segment[..segment.len() - 1];
        } else {
            break;
        }
        (xs, ruled) = columns(segment, left, right, vertical, h)?;
    }
    let ys = rows(segment, &xs, h, params);
    if ys.len() < 3 || ys.len() > MAX_BOUNDARIES {
        return None;
    }
    let cells = |xs: &[f64]| {
        let mut filled = vec![vec![false; xs.len() - 1]; ys.len() - 1];
        for b in boxes {
            let [x, y] = centre(b);
            let column = xs.windows(2).position(|w| x > w[0] && x < w[1]);
            let row = ys.windows(2).position(|w| y > w[0] && y < w[1]);
            if let (Some(c), Some(r)) = (column, row) {
                filled[r][c] = true;
            }
        }
        filled
    };
    // Empty outer columns (e.g. a rule overhanging the text) join their neighbour.
    loop {
        let filled = cells(&xs);
        let empty = |c: usize| filled.iter().all(|r| !r[c]);
        let n = xs.len() - 1;
        if n > 2 && empty(0) {
            xs.remove(1);
        } else if n > 2 && empty(n - 1) {
            xs.remove(n - 1);
        } else {
            break;
        }
    }
    // At least two rows must have text in two or more columns, or, with
    // drawn column rules, one such row and text in two rows.
    let filled = cells(&xs);
    let rich = filled
        .iter()
        .filter(|r| r.iter().filter(|c| **c).count() >= 2)
        .count();
    let rows_with_text = filled.iter().filter(|r| r.contains(&true)).count();
    let evidence = rich >= 2 || (ruled && rich >= 1 && rows_with_text >= 2);
    (evidence && xs.len() >= 3).then_some(Table { xs, ys })
}

fn caption(band: &Band, xs: &[f64], h: f64) -> bool {
    band.lines.len() == 1
        && band.lines[0].pieces.len() <= 2
        && band.lines[0].pieces.iter().any(|p| {
            xs[1..xs.len() - 1]
                .iter()
                .any(|x| p[0] < x - h && p[1] > x + h)
        })
}

/// Column boundaries: interior vertical rules, else text alignment; the
/// outer boundaries enclose both the rules and the text.
/// Also reports whether the columns come from drawn rules.
fn columns(
    segment: &[Band],
    left: f64,
    right: f64,
    vertical: &[Rule],
    h: f64,
) -> Option<(Vec<f64>, bool)> {
    let top = segment[0].top;
    let bottom = segment[segment.len() - 1].bottom;
    let phrases: Vec<&[f64; 2]> = segment
        .iter()
        .flat_map(|b| &b.lines)
        .flat_map(|l| &l.phrases)
        .collect();
    let text_left = phrases.iter().map(|p| p[0]).fold(left, f64::min);
    let text_right = phrases.iter().map(|p| p[1]).fold(right, f64::max);
    let mut interior: Vec<f64> = vertical
        .iter()
        .filter(|v| {
            v.at > left + h
                && v.at < right - h
                && v.to.min(top) - v.from.max(bottom) >= 0.5 * (top - bottom)
        })
        .map(|v| v.at)
        .collect();
    let ruled = !interior.is_empty();
    if !ruled {
        interior = text_gaps(segment, left, right, h);
    }
    interior.sort_by(f64::total_cmp);
    let mut xs = vec![text_left.min(left) - 0.5];
    for x in interior {
        if x - xs[xs.len() - 1] > 2.0 * MIN_BOUNDARY_GAP {
            xs.push(x);
        }
    }
    let end = text_right.max(right) + 0.5;
    if end - xs[xs.len() - 1] <= 2.0 * MIN_BOUNDARY_GAP {
        xs.pop();
    }
    xs.push(end);
    (xs.len() >= 3 && xs.len() <= MAX_BOUNDARIES).then_some((xs, ruled))
}

/// Column boundaries from vertical whitespace shared by (almost) all lines.
fn text_gaps(segment: &[Band], left: f64, right: f64, h: f64) -> Vec<f64> {
    const BIN: f64 = 0.25;
    let lines: Vec<&Line> = segment.iter().flat_map(|b| &b.lines).collect();
    if lines.is_empty() || right - left > 100_000.0 {
        return Vec::new();
    }
    let bins = ((right - left) / BIN).ceil() as usize + 1;
    let mut coverage = vec![0usize; bins];
    let bin = |x: f64| (((x - left) / BIN).floor().max(0.0) as usize).min(bins - 1);
    for line in &lines {
        for p in &line.pieces {
            for c in &mut coverage[bin(p[0])..=bin(p[1])] {
                *c += 1;
            }
        }
    }
    // A few spanning headers may cross a column gap.
    let allowed = lines.len() / 10;
    let first = coverage.iter().position(|c| *c > allowed);
    let last = coverage.iter().rposition(|c| *c > allowed);
    let (Some(first), Some(last)) = (first, last) else {
        return Vec::new();
    };
    let mut gaps = Vec::new();
    let mut start = None;
    for (i, c) in coverage.iter().enumerate().take(last + 1).skip(first) {
        match (start, *c <= allowed) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                if (i - s) as f64 * BIN >= 0.4 * h {
                    gaps.extend(
                        split(&coverage[s..i], s, 0.4 * h / BIN)
                            .iter()
                            .map(|b| left + b * BIN),
                    );
                }
                start = None;
            }
            _ => {}
        }
    }
    gaps
}

/// Boundaries inside a gap (fractional bin indexes): the middle of every
/// empty stretch of at least `min` bins, so text standing alone in the gap
/// (a header over an empty column) gets its own column; else the middle.
fn split(gap: &[usize], offset: usize, min: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut run = None;
    for (i, c) in gap.iter().chain([&1usize]).enumerate() {
        match (run, *c == 0) {
            (None, true) => run = Some(i),
            (Some(s), false) => {
                if (i - s) as f64 >= min {
                    out.push(offset as f64 + (s + i) as f64 / 2.0);
                }
                run = None;
            }
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(offset as f64 + gap.len() as f64 / 2.0);
    }
    out
}

/// Row boundaries (ascending): rules between bands, text lines within.
/// The first band of a multi-band table is its header and stays one row.
fn rows(segment: &[Band], xs: &[f64], h: f64, params: Params) -> Vec<f64> {
    let column = |x: f64| xs.windows(2).position(|w| x > w[0] && x < w[1]);
    // In a ruled grid (four or more bands) each band is one row unless it is
    // much taller than the smallest one (a group of unruled rows).
    let smallest = segment
        .iter()
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.top - b.bottom)
        .fold(f64::INFINITY, f64::min);
    let mut ys: Vec<f64> = vec![segment[0].top];
    for (index, band) in segment.iter().enumerate() {
        let header = index == 0 && segment.len() > 1;
        let grid_row = segment.len() >= 4 && band.top - band.bottom <= 4.0 * smallest;
        if !header && !grid_row && (!band.lines.is_empty() || band.top - band.bottom >= 1.5 * h) {
            let filled: Vec<Vec<usize>> = band
                .lines
                .iter()
                .map(|l| {
                    let mut c: Vec<usize> = l
                        .phrases
                        .iter()
                        .filter_map(|p| column((p[0] + p[1]) / 2.0))
                        .collect();
                    c.dedup();
                    c
                })
                .collect();
            let key = filled.iter().flatten().min().copied();
            let has_key = |i: usize| key.is_some_and(|k| filled[i].first() == Some(&k));
            let multi = filled.iter().filter(|c| c.len() >= 2).count();
            let dense = ((0..filled.len()).all(has_key) && multi * 4 >= filled.len() * 3)
                || multi * 20 >= filled.len() * 17;
            for i in 1..band.lines.len() {
                let gap = band.lines[i - 1].bottom - band.lines[i].top;
                if dense || gap > params.row_gap * h || (has_key(i) && !has_key(i - 1)) {
                    let y = (band.lines[i - 1].bottom + band.lines[i].top) / 2.0;
                    if ys[ys.len() - 1] - y > 2.0 * MIN_BOUNDARY_GAP {
                        ys.push(y);
                    }
                }
            }
        }
        if ys[ys.len() - 1] - band.bottom > 2.0 * MIN_BOUNDARY_GAP {
            ys.push(band.bottom);
        } else if let Some(last) = ys.last_mut() {
            *last = band.bottom;
        }
    }
    ys.reverse();
    ys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(at: f64, from: f64, to: f64) -> Rule {
        Rule {
            horizontal: true,
            at,
            from,
            to,
        }
    }

    fn cell(x: f64, y: f64, w: f64) -> Box4 {
        [x, y, x + w, y + 8.]
    }

    #[test]
    fn booktabs_columns_from_whitespace_rows_from_lines() {
        let rules = [h(100., 0., 200.), h(88., 0., 200.), h(40., 0., 200.)];
        let mut boxes = vec![cell(2., 90., 30.), cell(102., 90., 30.)];
        for i in 0..4 {
            let y = 78. - 10. * i as f64;
            boxes.push(cell(2., y, 40.));
            boxes.push(cell(102., y, 20.));
        }
        let t = tables(&rules, &boxes, NATIVE);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].xs.len(), 3);
        assert_eq!(t[0].ys.len(), 6, "{:?}", t[0].ys);
    }

    #[test]
    fn prose_between_rules_is_not_a_table() {
        let rules = [h(100., 0., 200.), h(40., 0., 200.)];
        let boxes: Vec<_> = (0..5)
            .map(|i| cell(0., 90. - 10. * i as f64, 195.))
            .collect();
        assert!(tables(&rules, &boxes, NATIVE).is_empty());
    }

    #[test]
    fn multi_line_cells_stay_in_one_row() {
        let rules = [h(100., 0., 200.), h(88., 0., 200.), h(40., 0., 200.)];
        let boxes = vec![
            cell(2., 90., 30.),
            cell(102., 90., 30.),
            cell(2., 78., 30.),
            cell(102., 78., 60.),
            cell(102., 68., 60.),
            cell(2., 54., 30.),
            cell(102., 54., 60.),
        ];
        let t = tables(&rules, &boxes, NATIVE);
        assert_eq!(t[0].ys.len(), 4, "{:?}", t[0].ys);
    }

    #[test]
    fn vertical_rules_define_columns() {
        let rules = [
            h(100., 0., 200.),
            h(80., 0., 200.),
            h(60., 0., 200.),
            Rule {
                horizontal: false,
                at: 60.,
                from: 60.,
                to: 100.,
            },
        ];
        let boxes = vec![
            cell(2., 85., 20.),
            cell(70., 85., 20.),
            cell(2., 65., 20.),
            cell(70., 65., 100.),
        ];
        let t = tables(&rules, &boxes, NATIVE);
        assert_eq!(t[0].xs.len(), 3);
        assert_eq!(t[0].xs[1], 60.);
    }
}
