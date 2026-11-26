//! Conservative cell-local line clustering; scripts become plain inline text.
use super::convert::Run;
use super::invalid;
use crate::Result;
fn height(r: &Run) -> f64 {
    r.bounds[3] - r.bounds[1]
}
fn compatible(anchor: &Run, r: &Run) -> bool {
    if (anchor.bounds[3] - r.bounds[3]).abs() <= 1.0
        && (anchor.bounds[1] - r.bounds[1]).abs() <= 1.0
    {
        return true;
    }
    let h = height(r);
    let ah = height(anchor);
    let overlap = (anchor.bounds[3].min(r.bounds[3]) - anchor.bounds[1].max(r.bounds[1])).max(0.0);
    h <= 0.85 * ah && overlap >= 0.5 * h
}
/// Largest-height runs establish fixed anchors, preventing script chains from
/// bridging real lines. Ambiguous matches stay separate. No run is discarded.
pub(super) fn ordered<'a>(runs: &[&'a Run]) -> Result<Vec<Vec<&'a Run>>> {
    if runs.len() > 16384 {
        return Err(invalid("cell text run limit exceeded"));
    }
    let mut pending = runs.to_vec();
    pending.sort_by(|a, b| {
        height(b)
            .total_cmp(&height(a))
            .then(b.bounds[3].total_cmp(&a.bounds[3]))
            .then(a.bounds[0].total_cmp(&b.bounds[0]))
            .then(a.sequence.cmp(&b.sequence))
    });
    struct Line<'a> {
        runs: Vec<&'a Run>,
        left: f64,
        right: f64,
    }
    let mut lines: Vec<Line<'a>> = Vec::new();
    for r in pending {
        let candidates: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let anchor = line.runs[0];
                if !compatible(anchor, r) {
                    return None;
                }
                let gap = (line.left - r.bounds[2])
                    .max(r.bounds[0] - line.right)
                    .max(0.0);
                let primary = (anchor.bounds[3] - r.bounds[3]).abs() <= 1.0
                    && (anchor.bounds[1] - r.bounds[1]).abs() <= 1.0;
                if primary || gap <= 2.0_f64.max(0.5 * height(anchor)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        if candidates.len() == 1 {
            let line = &mut lines[candidates[0]];
            line.left = line.left.min(r.bounds[0]);
            line.right = line.right.max(r.bounds[2]);
            line.runs.push(r);
        } else {
            if lines.len() >= 1024 {
                return Err(invalid("cell text line limit exceeded"));
            }
            lines.push(Line {
                runs: vec![r],
                left: r.bounds[0],
                right: r.bounds[2],
            });
        }
    }
    lines.sort_by(|a, b| {
        b.runs[0].bounds[3]
            .total_cmp(&a.runs[0].bounds[3])
            .then(a.runs[0].bounds[0].total_cmp(&b.runs[0].bounds[0]))
            .then(a.runs[0].sequence.cmp(&b.runs[0].sequence))
    });
    let mut lines: Vec<Vec<&Run>> = lines.into_iter().map(|line| line.runs).collect();
    for line in &mut lines {
        line.sort_by(|a, b| {
            a.bounds[0]
                .total_cmp(&b.bounds[0])
                .then(a.sequence.cmp(&b.sequence))
        });
    }
    Ok(lines)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(bounds: [f64; 4], text: &str, sequence: usize) -> Run {
        Run {
            bounds,
            text: text.into(),
            gap: false,
            sequence,
        }
    }
    fn text(runs: &[Run]) -> String {
        let refs: Vec<&Run> = runs.iter().collect();
        ordered(&refs)
            .unwrap()
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("<br>")
    }
    #[test]
    fn puts_two_subscripts_in_their_horizontal_positions() {
        let runs = [
            run([1., 10., 9., 20.], "CO", 1),
            run([9.1, 8.5, 12., 15.5], "2", 2),
            run([12.1, 10., 25., 20.], " (CO", 3),
            run([25.1, 8.5, 28., 15.5], "2", 4),
            run([28.1, 10., 45., 20.], " curve)", 5),
        ];
        assert_eq!(text(&runs), "CO2 (CO2 curve)");
    }
    #[test]
    fn keeps_superscript_inline_and_real_next_line_separate() {
        let runs = [
            run([1., 10., 8., 20.], "x", 1),
            run([8.1, 16., 11., 22.], "2", 2),
            run([1., -4., 20., 6.], "Next", 3),
        ];
        assert_eq!(text(&runs), "x2<br>Next");
    }
    #[test]
    fn equal_height_shifted_line_is_not_a_script() {
        let runs = [
            run([1., 10., 8., 20.], "A", 1),
            run([9., 7., 16., 17.], "B", 2),
        ];
        assert_eq!(text(&runs), "A<br>B");
    }
    #[test]
    fn ambiguous_script_is_retained_as_a_separate_line() {
        let runs = [
            run([1., 10., 8., 20.], "A", 1),
            run([1., 6., 8., 16.], "B", 2),
            run([8.1, 11., 10., 15.], "2", 3),
        ];
        let refs: Vec<&Run> = runs.iter().collect();
        let result = ordered(&refs).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.iter().map(Vec::len).sum::<usize>(), 3);
    }
}
