//! Strict, internal Predictor-15 eligibility and PNG row reconstruction.
use lopdf::{Object, Stream};

pub(super) fn parameters(stream: &Stream, width: u32, channels: u64) -> Option<bool> {
    let object = match stream.dict.get(b"DecodeParms") {
        Err(_) => return Some(false),
        Ok(object) => object,
    };
    if !matches!(stream.dict.get(b"Filter"), Ok(Object::Name(name)) if name == b"FlateDecode") {
        return None;
    }
    let Object::Dictionary(params) = object else {
        return None;
    };
    if params.iter().any(|(key, _)| {
        !matches!(
            key.as_slice(),
            b"Predictor" | b"Columns" | b"Colors" | b"BitsPerComponent"
        )
    }) {
        return None;
    }
    let integer = |key: &[u8], default: i64| match params.get(key) {
        Err(_) => Some(default),
        Ok(Object::Integer(value)) => Some(*value),
        _ => None,
    };
    if integer(b"Predictor", 1)? != 15
        || integer(b"Columns", 1)? != i64::from(width)
        || integer(b"Colors", 1)? != channels as i64
        || integer(b"BitsPerComponent", 8)? != 8
    {
        return None;
    }
    Some(true)
}

pub(super) fn decode(data: &[u8], row_bytes: usize, height: usize, bpp: usize) -> Option<Vec<u8>> {
    if row_bytes == 0 || height == 0 || !matches!(bpp, 1 | 3) || !row_bytes.is_multiple_of(bpp) {
        return None;
    }
    let stride = row_bytes.checked_add(1)?;
    if data.len() != stride.checked_mul(height)? {
        return None;
    }
    let mut raw = vec![0u8; row_bytes.checked_mul(height)?];
    for (y, row) in data.chunks_exact(stride).enumerate() {
        if row[0] > 4 {
            return None;
        }
        for x in 0..row_bytes {
            let pos = y * row_bytes + x;
            let left = if x >= bpp { raw[pos - bpp] } else { 0 };
            let up = if y > 0 { raw[pos - row_bytes] } else { 0 };
            let corner = if y > 0 && x >= bpp {
                raw[pos - row_bytes - bpp]
            } else {
                0
            };
            let predicted = match row[0] {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, corner),
                _ => unreachable!(),
            };
            raw[pos] = row[x + 1].wrapping_add(predicted);
        }
    }
    Some(raw)
}
fn paeth(left: u8, up: u8, corner: u8) -> u8 {
    let p = i32::from(left) + i32::from(up) - i32::from(corner);
    let a = (p - i32::from(left)).abs();
    let b = (p - i32::from(up)).abs();
    let c = (p - i32::from(corner)).abs();
    if a <= b && a <= c {
        left
    } else if b <= c {
        up
    } else {
        corner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_rows_are_rejected() {
        for (data, width, height, bpp) in [
            (&[5, 1][..], 1, 1, 1),
            (&[0][..], 1, 1, 1),
            (&[0, 1, 2][..], 1, 1, 1),
            (&[][..], 0, 1, 1),
            (&[][..], 1, 0, 1),
            (&[][..], 1, 1, 0),
            (&[][..], usize::MAX, 2, 1),
        ] {
            assert!(decode(data, width, height, bpp).is_none());
        }
    }
    #[test]
    fn parameter_shapes_and_defaults_are_strict() {
        let mut stream = Stream::new(lopdf::dictionary! {"Filter" => "FlateDecode"}, vec![]);
        assert_eq!(parameters(&stream, 1, 1), Some(false));
        stream
            .dict
            .set("DecodeParms", lopdf::dictionary! {"Predictor" => 15});
        assert_eq!(parameters(&stream, 1, 1), Some(true));
        assert_eq!(parameters(&stream, 2, 1), None);
        assert_eq!(parameters(&stream, 1, 3), None);
        for params in [
            lopdf::dictionary! {"Predictor" => 1},
            lopdf::dictionary! {"Predictor" => 14},
            lopdf::dictionary! {"Predictor" => 16},
            lopdf::dictionary! {"Predictor" => 15, "Colors" => 0},
            lopdf::dictionary! {"Predictor" => 15, "Columns" => -1},
            lopdf::dictionary! {"Predictor" => 15, "BitsPerComponent" => 16},
            lopdf::dictionary! {"Predictor" => 15, "Unexpected" => 1},
            lopdf::dictionary! {"Predictor" => Object::Real(15.0)},
        ] {
            stream.dict.set("DecodeParms", params);
            assert_eq!(parameters(&stream, 1, 1), None);
        }
        for value in [
            Object::Null,
            Object::Array(vec![]),
            Object::Reference((1, 0)),
        ] {
            stream.dict.set("DecodeParms", value);
            assert_eq!(parameters(&stream, 1, 1), None);
        }
    }
    #[test]
    fn all_five_filters_gray_and_rgb_match_independent_vectors() {
        let cases: &[(usize, usize, &[u8], &[u8])] = &[
            (
                3,
                1,
                &[0, 250, 10, 80, 0, 5, 220, 40],
                &[250, 10, 80, 5, 220, 40],
            ),
            (
                3,
                1,
                &[1, 250, 16, 70, 1, 5, 215, 76],
                &[250, 10, 80, 5, 220, 40],
            ),
            (
                3,
                1,
                &[2, 250, 10, 80, 2, 11, 210, 216],
                &[250, 10, 80, 5, 220, 40],
            ),
            (
                3,
                1,
                &[3, 250, 141, 75, 3, 136, 213, 146],
                &[250, 10, 80, 5, 220, 40],
            ),
            (
                3,
                1,
                &[4, 250, 16, 70, 4, 11, 215, 76],
                &[250, 10, 80, 5, 220, 40],
            ),
            (
                6,
                3,
                &[0, 250, 10, 80, 5, 220, 40, 0, 15, 200, 255, 100, 30, 10],
                &[250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10],
            ),
            (
                6,
                3,
                &[1, 250, 10, 80, 11, 210, 216, 1, 15, 200, 255, 85, 86, 11],
                &[250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10],
            ),
            (
                6,
                3,
                &[2, 250, 10, 80, 5, 220, 40, 2, 21, 190, 175, 95, 66, 226],
                &[250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10],
            ),
            (
                6,
                3,
                &[3, 250, 10, 80, 136, 215, 0, 3, 146, 195, 215, 90, 76, 119],
                &[250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10],
            ),
            (
                6,
                3,
                &[4, 250, 10, 80, 11, 210, 216, 4, 21, 190, 175, 95, 66, 11],
                &[250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10],
            ),
        ];
        for &(row_bytes, bpp, encoded, expected) in cases {
            assert_eq!(decode(encoded, row_bytes, 2, bpp).unwrap(), expected);
        }
    }
}
