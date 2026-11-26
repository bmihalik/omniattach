//! Axis-aligned rulings from any painting style, for the layout detector.
//!
//! Unlike the strict detectors this accepts stroked lines, thin filled
//! rectangles and the edges of shaded cells together, ignores clips, images
//! and Form XObjects, and drops white paint.
use super::paths::{self, Clips, Rect, Segment};
use crate::{Options, Result};
use lopdf::content::Content;

/// A horizontal (`at` = y, span in x) or vertical (`at` = x, span in y) rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rule {
    pub horizontal: bool,
    pub at: f64,
    pub from: f64,
    pub to: f64,
}

impl Rule {
    pub fn len(&self) -> f64 {
        self.to - self.from
    }

    /// Cut to `[x0, y0, x1, y1]`; false when nothing visible remains.
    fn clip(&mut self, [x0, y0, x1, y1]: Rect) -> bool {
        let tolerance = 1.0;
        let (lo, hi, across_lo, across_hi) = if self.horizontal {
            (x0, x1, y0, y1)
        } else {
            (y0, y1, x0, x1)
        };
        if self.at < across_lo - tolerance || self.at > across_hi + tolerance {
            return false;
        }
        self.from = self.from.max(lo);
        self.to = self.to.min(hi);
        self.len() > 0.0
    }

    /// The same rule in a frame rotated by `rotation` degrees (see
    /// [`super::frame`]).
    pub fn rotated(self, rotation: u16) -> Self {
        match (rotation, self.horizontal) {
            (90, true) => Self {
                horizontal: false,
                at: self.at,
                from: -self.to,
                to: -self.from,
            },
            (90, false) => Self {
                horizontal: true,
                at: -self.at,
                from: self.from,
                to: self.to,
            },
            (270, true) => Self {
                horizontal: false,
                at: -self.at,
                from: self.from,
                to: self.to,
            },
            (270, false) => Self {
                horizontal: true,
                at: self.at,
                from: -self.to,
                to: -self.from,
            },
            _ => self,
        }
    }
}

/// Thin rectangles up to this thickness are rules, not shaded areas.
const MAX_THICKNESS: f64 = 3.0;
/// Axis tolerance for stroked segments.
const SKEW: f64 = 0.5;
/// Rules this close on the same axis line are merged.
const JOIN: f64 = 1.5;
/// Shaded rectangles larger than this in both directions are page or panel
/// backgrounds, not table cells.
const MAX_SHADE: f64 = 500.0;

/// Collect and merge all visible axis-aligned rules of one page, cut to the
/// clip in effect and to `page` (the media box), when known.
pub(super) fn collect(
    content: &Content,
    options: &Options,
    page: Option<Rect>,
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    paths::scan(content, options, Clips::Ignore, |paint| {
        let start = rules.len();
        if paint.irregular && paint.rects.is_empty() {
            return Ok(());
        }
        let op = paint.operator;
        let stroke = matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*");
        let fill = matches!(op, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
        if stroke && !paint.white_stroke {
            for s in paint.segments {
                rules.extend(segment(*s));
            }
        }
        if fill && !paint.white_fill {
            let polygons = if paint.rects.is_empty() {
                subpaths(paint.segments)
                    .filter_map(bounding)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            for r in paint.rects.iter().copied().chain(polygons) {
                rules.extend(rectangle(r));
            }
        }
        for clip in [paint.clip, page].into_iter().flatten() {
            let mut kept = rules.split_off(start);
            kept.retain_mut(|r| r.clip(clip));
            rules.extend(kept);
        }
        if rules.len() > options.limits.archive_entries {
            return Err(super::limit("table rules"));
        }
        Ok(())
    })?;
    Ok(merge(rules))
}

fn segment((a, b): Segment) -> Option<Rule> {
    let (dx, dy) = ((b[0] - a[0]).abs(), (b[1] - a[1]).abs());
    if dy <= SKEW && dx > SKEW {
        Some(Rule {
            horizontal: true,
            at: (a[1] + b[1]) / 2.0,
            from: a[0].min(b[0]),
            to: a[0].max(b[0]),
        })
    } else if dx <= SKEW && dy > SKEW {
        Some(Rule {
            horizontal: false,
            at: (a[0] + b[0]) / 2.0,
            from: a[1].min(b[1]),
            to: a[1].max(b[1]),
        })
    } else {
        None
    }
}

/// Split a flat segment list into connected subpaths.
fn subpaths(segments: &[Segment]) -> impl Iterator<Item = &[Segment]> {
    let mut start = 0;
    let mut out = Vec::new();
    for i in 1..=segments.len() {
        if i == segments.len()
            || segments[i].0 != segments[i - 1].1
            || segments[i - 1].1 == segments[start].0
        {
            out.push(&segments[start..i]);
            start = i;
        }
    }
    out.into_iter()
}

/// A closed path of four axis-aligned edges, as a rectangle.
fn bounding(segments: &[Segment]) -> Option<Rect> {
    if segments.len() != 4 || segments.iter().any(|s| segment(*s).is_none()) {
        return None;
    }
    let xs = segments.iter().flat_map(|(a, b)| [a[0], b[0]]);
    let ys = segments.iter().flat_map(|(a, b)| [a[1], b[1]]);
    let r = [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

/// A filled rectangle is a rule when thin, otherwise its four edges bound a
/// shaded cell or row.
fn rectangle([x0, y0, x1, y1]: Rect) -> Vec<Rule> {
    let (w, h) = (x1 - x0, y1 - y0);
    let rule = |horizontal, at, from, to| Rule {
        horizontal,
        at,
        from,
        to,
    };
    if h <= MAX_THICKNESS && w > h {
        vec![rule(true, (y0 + y1) / 2.0, x0, x1)]
    } else if w <= MAX_THICKNESS && h > w {
        vec![rule(false, (x0 + x1) / 2.0, y0, y1)]
    } else if w > MAX_SHADE && h > MAX_SHADE {
        Vec::new()
    } else {
        vec![
            rule(true, y0, x0, x1),
            rule(true, y1, x0, x1),
            rule(false, x0, y0, y1),
            rule(false, x1, y0, y1),
        ]
    }
}

/// Join collinear, touching or overlapping rules on each axis.
pub(super) fn merge(mut rules: Vec<Rule>) -> Vec<Rule> {
    rules.retain(|r| [r.at, r.from, r.to].iter().all(|n| n.is_finite()) && r.len() > 0.0);
    // Each pass can only shrink the list; stop when it no longer does.
    loop {
        let before = rules.len();
        rules = merge_pass(rules);
        if rules.len() == before {
            return rules;
        }
    }
}

fn merge_pass(mut rules: Vec<Rule>) -> Vec<Rule> {
    rules.sort_by(|a, b| {
        (a.horizontal, a.at, a.from)
            .partial_cmp(&(b.horizontal, b.at, b.from))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<Rule> = Vec::new();
    'next: for r in rules {
        for o in out.iter_mut().rev() {
            if o.horizontal != r.horizontal || r.at - o.at > JOIN {
                break;
            }
            if r.from <= o.to + JOIN && r.to >= o.from - JOIN {
                let (lw, rw) = (o.len().max(1.0), r.len().max(1.0));
                o.at = (o.at * lw + r.at * rw) / (lw + rw);
                o.from = o.from.min(r.from);
                o.to = o.to.max(r.to);
                continue 'next;
            }
        }
        out.push(r);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(data: &[u8]) -> Vec<Rule> {
        collect(&Content::decode(data).unwrap(), &Options::default(), None).unwrap()
    }

    #[test]
    fn mixes_strokes_thin_fills_and_shaded_edges() {
        let r = rules(b"0 0 m 100 0 l S 0 20 100 0.5 re f 0 40 100 10 re f");
        let h: Vec<_> = r.iter().filter(|r| r.horizontal).map(|r| r.at).collect();
        assert_eq!(h, vec![0.0, 20.25, 40.0, 50.0]);
        assert_eq!(r.iter().filter(|r| !r.horizontal).count(), 2);
    }

    #[test]
    fn filled_polygons_with_many_cells_give_cell_edges() {
        let r = rules(
            b"0 0 m 50 0 l 50 10 l 0 10 l 0 0 l h 50 0 m 100 0 l 100 10 l 50 10 l 50 0 l h f",
        );
        let v: Vec<_> = r.iter().filter(|r| !r.horizontal).map(|r| r.at).collect();
        assert_eq!(v, vec![0.0, 50.0, 100.0]);
        assert_eq!(r.iter().filter(|r| r.horizontal).count(), 2);
    }

    #[test]
    fn ignores_white_paint_clips_and_images() {
        let r = rules(b"q 1 g 0 0 100 0.5 re f Q 0 0 50 50 re W n /Im Do 1 G 0 0 m 9 0 l S");
        assert!(r.is_empty());
    }

    #[test]
    fn rules_are_cut_to_clips() {
        let r = rules(b"q 0 0 50 50 re W n 0 10 m 100 10 l S 0 80 m 100 80 l S Q");
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].from, r[0].to), (0.0, 50.0));
    }

    #[test]
    fn merges_collinear_pieces() {
        let r = rules(b"0 0 m 50 0 l S 50.5 0.2 m 100 0.2 l S");
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].from, r[0].to), (0.0, 100.0));
    }

    #[test]
    fn rotation_maps_axes() {
        let v = Rule {
            horizontal: false,
            at: 10.,
            from: 0.,
            to: 50.,
        };
        assert_eq!(
            v.rotated(90),
            Rule {
                horizontal: true,
                at: -10.,
                from: 0.,
                to: 50.
            }
        );
        let h = Rule {
            horizontal: true,
            at: 10.,
            from: 0.,
            to: 50.,
        };
        assert_eq!(h.rotated(270).from, 0.);
        assert_eq!(h.rotated(270).at, -10.);
    }
}
