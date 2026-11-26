//! Reading frames: tables whose text runs up (90) or down (270) the page are
//! detected and converted in the page rotated so that their text is upright.
use crate::{PdfElement, PdfElementKind};

/// Share of a page's text characters a rotated frame needs to be tried.
const MIN_SHARE: f64 = 0.2;

/// `[left, bottom, right, top]` mapped into the frame of `rotation`:
/// 90 maps `(x, y)` to `(y, -x)`, 270 maps it to `(-y, x)`.
pub(super) fn to_frame(b: [f64; 4], rotation: u16) -> [f64; 4] {
    let [l, bo, r, t] = b;
    match rotation {
        90 => [bo, -r, t, -l],
        270 => [-t, l, -bo, r],
        _ => b,
    }
}

/// Frame of one text run's baseline angle (degrees), within 10 degrees.
pub(super) fn of_angle(angle: Option<f64>) -> Option<u16> {
    match angle {
        None => Some(0),
        Some(a) if (a - 90.0).abs() <= 10.0 => Some(90),
        Some(a) if (a + 90.0).abs() <= 10.0 => Some(270),
        Some(a) if a.abs() <= 10.0 => Some(0),
        _ => None,
    }
}

/// Frames worth trying on `page`: always upright, plus 90/270 when enough
/// of the page's text runs that way.
pub(super) fn frames(elements: &[PdfElement], page: u32) -> Vec<u16> {
    let mut chars = [0usize; 3];
    for e in elements {
        if e.anchor.page != page || e.kind != PdfElementKind::Text {
            continue;
        }
        let n = e.text.as_deref().map_or(0, |t| t.trim().chars().count());
        match of_angle(e.text_rotation) {
            Some(0) => chars[0] += n,
            Some(90) => chars[1] += n,
            Some(270) => chars[2] += n,
            _ => {}
        }
    }
    let total = chars.iter().sum::<usize>().max(1) as f64;
    let mut out = vec![0];
    for (count, rotation) in [(chars[1], 90), (chars[2], 270)] {
        if count as f64 / total >= MIN_SHARE {
            out.push(rotation);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_makes_text_upright() {
        // A run going up the page at x 10..20, y 0..100.
        let up = to_frame([10., 0., 20., 100.], 90);
        assert_eq!(up, [0., -20., 100., -10.]);
        let down = to_frame([10., 0., 20., 100.], 270);
        assert_eq!(down, [-100., 10., 0., 20.]);
        assert_eq!(of_angle(Some(89.0)), Some(90));
        assert_eq!(of_angle(Some(-90.0)), Some(270));
        assert_eq!(of_angle(Some(45.0)), None);
    }
}
