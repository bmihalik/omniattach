//! Minimal page-content path interpreter shared by the table detectors.
//!
//! Tracks `q`/`Q`/`cm`, builds transformed path segments from `m`/`l`/`h`/`re`,
//! and hands each painted path to a detector. Clipping paths are either
//! rejected or, when allowed, reduced to an axis-aligned rectangle.
use super::{invalid, limit};
use crate::{Options, Result};
use lopdf::{Object, content::Content};

pub(super) type Point = [f64; 2];
pub(super) type Segment = (Point, Point);
/// Axis-aligned rectangle `[x0, y0, x1, y1]`.
pub(super) type Rect = [f64; 4];
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];
const MAX_STACK: usize = 128;
const MAX_CLIPS: usize = 128;

/// One painted path, in page user space.
pub(super) struct Paint<'a> {
    /// Painting operator (`S`, `s`, `f`, `F`, `f*`, `B`, `b`, `n`, ...).
    pub operator: &'a str,
    pub segments: &'a [Segment],
    /// Rectangles from `re`, when each one is axis-aligned and non-degenerate.
    pub rects: &'a [Rect],
    /// Curves, `l` without a current point, or a non-rectangular `re`.
    pub irregular: bool,
    /// Effective rectangular clip at paint time.
    pub clip: Option<Rect>,
}

/// Run `on_paint` for every painted path. With `allow_clips`, simple
/// rectangular `W n` clips are tracked and all clips are returned; otherwise
/// any clipping operator is an error. XObjects are always rejected.
pub(super) fn scan(
    content: &Content,
    options: &Options,
    allow_clips: bool,
    mut on_paint: impl FnMut(Paint<'_>) -> Result<()>,
) -> Result<Vec<Rect>> {
    if content.operations.len() > options.limits.xml_nodes as usize {
        return Err(limit("content operations"));
    }
    let mut matrix = IDENTITY;
    let mut clip: Option<Rect> = None;
    let mut stack: Vec<(Matrix, Option<Rect>)> = Vec::new();
    let mut clips = Vec::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut rects: Vec<Rect> = Vec::new();
    let (mut current, mut start): (Option<Point>, Option<Point>) = (None, None);
    let mut curved = false;
    let mut bad_rect = false;
    let mut pending_clip = false;

    for op in &content.operations {
        let a = &op.operands;
        let pair = |i: usize| -> Result<Point> {
            let get = |k: usize| number(a.get(k).ok_or_else(|| invalid("missing coordinate"))?);
            Ok([get(i)?, get(i + 1)?])
        };
        match op.operator.as_str() {
            "q" => {
                if stack.len() >= MAX_STACK {
                    return Err(limit("graphics stack"));
                }
                stack.push((matrix, clip));
            }
            "Q" => (matrix, clip) = stack.pop().ok_or_else(|| invalid("unbalanced Q"))?,
            "cm" => {
                if a.len() != 6 {
                    return Err(invalid("invalid cm"));
                }
                let v = a.iter().map(number).collect::<Result<Vec<_>>>()?;
                matrix = compose([v[0], v[1], v[2], v[3], v[4], v[5]], matrix);
                if matrix.iter().any(|n| !n.is_finite()) {
                    return Err(invalid("matrix overflow"));
                }
            }
            "Do" => {
                return Err(invalid(
                    "selected page invokes XObjects; Forms and images unsupported",
                ));
            }
            "m" => {
                let p = transform(matrix, pair(0)?);
                current = Some(p);
                start = Some(p);
            }
            "l" => {
                let p = transform(matrix, pair(0)?);
                match current {
                    Some(c) => segments.push((c, p)),
                    None => curved = true,
                }
                current = Some(p);
            }
            "h" => {
                if let (Some(c), Some(s)) = (current, start) {
                    if c != s {
                        segments.push((c, s));
                    }
                    current = Some(s);
                }
            }
            "re" => {
                let (p, size) = (pair(0)?, pair(2)?);
                let c = [
                    transform(matrix, p),
                    transform(matrix, [p[0] + size[0], p[1]]),
                    transform(matrix, [p[0] + size[0], p[1] + size[1]]),
                    transform(matrix, [p[0], p[1] + size[1]]),
                ];
                let edges = [(c[0], c[1]), (c[1], c[2]), (c[2], c[3]), (c[3], c[0])];
                match rectangle(&edges) {
                    Some(r) => rects.push(r),
                    None => bad_rect = true,
                }
                segments.extend(edges);
                current = Some(c[0]);
                start = current;
            }
            "c" | "v" | "y" => curved = true,
            "W" | "W*" => {
                if !allow_clips {
                    return Err(invalid("clipped page content unsupported"));
                }
                if curved || bad_rect || pending_clip {
                    return Err(invalid("complex clipped page content unsupported"));
                }
                pending_clip = true;
            }
            "S" | "s" | "B" | "B*" | "b" | "b*" | "f" | "F" | "f*" | "n" => {
                let operator = op.operator.as_str();
                if matches!(operator, "s" | "b" | "b*")
                    && let (Some(c), Some(s)) = (current, start)
                    && c != s
                {
                    segments.push((c, s));
                }
                if segments.len() > options.limits.archive_entries {
                    return Err(limit("path segments"));
                }
                if segments
                    .iter()
                    .any(|(a, b)| a.iter().chain(b).any(|n| !n.is_finite()))
                {
                    return Err(invalid("path overflow"));
                }
                if pending_clip {
                    if operator != "n" {
                        return Err(invalid("painted clipping path unsupported"));
                    }
                    let mut r = rectangle(&segments).ok_or_else(|| {
                        invalid("nonrectangular clipped page content unsupported")
                    })?;
                    if let Some(old) = clip {
                        r = [
                            r[0].max(old[0]),
                            r[1].max(old[1]),
                            r[2].min(old[2]),
                            r[3].min(old[3]),
                        ];
                    }
                    clip = Some(r);
                    clips.push(r);
                    if clips.len() > MAX_CLIPS {
                        return Err(limit("rectangular clips"));
                    }
                } else {
                    on_paint(Paint {
                        operator,
                        segments: &segments,
                        rects: &rects,
                        irregular: curved || bad_rect,
                        clip,
                    })?;
                }
                segments.clear();
                rects.clear();
                (current, start) = (None, None);
                (curved, bad_rect, pending_clip) = (false, false, false);
            }
            _ => {}
        }
        if segments.len() > options.limits.archive_entries {
            return Err(limit("path segments"));
        }
    }
    if !stack.is_empty() || pending_clip {
        return Err(invalid("unbalanced graphics state"));
    }
    Ok(clips)
}

pub(super) fn number(o: &Object) -> Result<f64> {
    let n = match o {
        Object::Integer(v) => *v as f64,
        Object::Real(v) => *v as f64,
        _ => return Err(invalid("non-numeric geometry")),
    };
    if !n.is_finite() {
        return Err(invalid("non-finite geometry"));
    }
    Ok(n)
}

fn transform(m: Matrix, p: Point) -> Point {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

fn compose(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// A closed four-edge axis-aligned rectangle, or `None`.
fn rectangle(edges: &[Segment]) -> Option<Rect> {
    if edges.len() != 4 {
        return None;
    }
    let xs = || edges.iter().map(|(a, _)| a[0]);
    let ys = || edges.iter().map(|(a, _)| a[1]);
    let x0 = xs().min_by(f64::total_cmp)?;
    let x1 = xs().max_by(f64::total_cmp)?;
    let y0 = ys().min_by(f64::total_cmp)?;
    let y1 = ys().max_by(f64::total_cmp)?;
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    for corner in [[x0, y0], [x1, y0], [x1, y1], [x0, y1]] {
        if edges.iter().filter(|(a, _)| *a == corner).count() != 1 {
            return None;
        }
    }
    for (i, (a, b)) in edges.iter().enumerate() {
        if *b != edges[(i + 1) % 4].0 || (a[0] != b[0] && a[1] != b[1]) || a == b {
            return None;
        }
    }
    Some([x0, y0, x1, y1])
}

/// `a` lies inside `b`.
pub(super) fn inside(a: Rect, b: Rect) -> bool {
    a[0] >= b[0] && a[1] >= b[1] && a[2] <= b[2] && a[3] <= b[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paints(data: &[u8], allow_clips: bool) -> Result<Vec<(String, usize)>> {
        let mut out = Vec::new();
        scan(
            &Content::decode(data).unwrap(),
            &Options::default(),
            allow_clips,
            |p| {
                out.push((p.operator.to_string(), p.segments.len()));
                Ok(())
            },
        )?;
        Ok(out)
    }

    #[test]
    fn applies_transforms_and_restores_state() {
        let content = Content::decode(b"q 2 0 0 2 5 7 cm 0 0 m 10 0 l S Q 0 0 m 10 0 l S").unwrap();
        let mut seen = Vec::new();
        scan(&content, &Options::default(), false, |p| {
            seen.push(p.segments[0]);
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, vec![([5., 7.], [25., 7.]), ([0., 0.], [10., 0.])]);
    }

    #[test]
    fn rejects_xobjects_clips_and_unbalanced_state() {
        assert!(paints(b"/F Do", true).is_err());
        assert!(paints(b"0 0 10 10 re W n", false).is_err());
        assert!(paints(b"q", false).is_err());
        assert!(paints(b"0 0 m 5 5 l 0 5 l h W n", true).is_err());
    }

    #[test]
    fn rectangular_clips_are_tracked_not_painted() {
        let content = Content::decode(b"0 0 100 100 re W n 10 10 20 20 re f").unwrap();
        let mut clip = None;
        let clips = scan(&content, &Options::default(), true, |p| {
            clip = p.clip;
            Ok(())
        })
        .unwrap();
        assert_eq!(clips, vec![[0., 0., 100., 100.]]);
        assert_eq!(clip, Some([0., 0., 100., 100.]));
    }

    #[test]
    fn honors_operation_limit() {
        let mut options = Options::default();
        options.limits.xml_nodes = 0;
        assert!(
            scan(
                &Content::decode(b"0 0 m").unwrap(),
                &options,
                false,
                |_| Ok(())
            )
            .is_err()
        );
    }
}
