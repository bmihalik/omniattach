//! Narrow native raster conversion; never guesses filters, bit depth or color space.
use crate::{Error, Result};
use lopdf::{Object, Stream};
use std::io::{self, Write};
#[path = "pdf_predictor.rs"]
mod predictor;

pub(super) struct EncodedImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
    pub media_type: &'static str,
}
pub(super) enum ImageData {
    Encoded(EncodedImage),
    Skipped(String),
}
fn skip(reason: &str) -> ImageData {
    ImageData::Skipped(reason.into())
}

pub(super) fn extract(
    stream: &Stream,
    decoded_left: &mut u64,
    output_limit: u64,
) -> Result<ImageData> {
    if [
        b"Mask".as_slice(),
        b"SMask".as_slice(),
        b"Decode".as_slice(),
    ]
    .iter()
    .any(|key| stream.dict.get(key).is_ok())
        || stream
            .dict
            .get(b"ImageMask")
            .is_ok_and(|v| !matches!(v, Object::Boolean(false)))
    {
        return Ok(skip("image masks or custom Decode are unsupported"));
    }
    let filter = match stream.dict.get(b"Filter") {
        Err(_) => None,
        Ok(Object::Name(n)) => Some(n.as_slice()),
        Ok(Object::Array(a)) if a.len() == 1 => match a[0].as_name() {
            Ok(n) => Some(n),
            Err(_) => return Ok(skip("indirect or invalid image filter is unsupported")),
        },
        _ => {
            return Ok(skip(
                "image filter chains or indirect filters are unsupported",
            ));
        }
    };
    if stream.dict.get(b"DecodeParms").is_ok() && filter != Some(b"FlateDecode") {
        return Ok(skip(
            "DecodeParms is supported only for direct Flate Predictor 15 rasters",
        ));
    }
    if let Some((extension, media_type)) = match filter {
        Some(b"DCTDecode") => Some(("jpg", "image/jpeg")),
        Some(b"JPXDecode") => Some(("jp2", "image/jp2")),
        _ => None,
    } {
        if stream.content.len() as u64 > output_limit {
            return Err(Error::LimitExceeded("PDF image bytes".into()));
        }
        return Ok(ImageData::Encoded(EncodedImage {
            bytes: stream.content.clone(),
            extension,
            media_type,
        }));
    }
    if filter.is_some_and(|f| f != b"FlateDecode") {
        return Ok(skip(
            "unsupported image filter; expected DCTDecode, JPXDecode, FlateDecode or no filter",
        ));
    }
    if stream
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .ok()
        != Some(8)
    {
        return Ok(skip("raw raster requires direct BitsPerComponent 8"));
    }
    let (channels, color) = match stream
        .dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .ok()
    {
        Some(b"DeviceGray") => (1u64, png::ColorType::Grayscale),
        Some(b"DeviceRGB") => (3u64, png::ColorType::Rgb),
        _ => {
            return Ok(skip(
                "raw raster requires direct DeviceGray or DeviceRGB; indexed/ICC/CMYK/indirect color spaces are unsupported",
            ));
        }
    };
    let dimension = |key: &[u8]| {
        stream
            .dict
            .get(key)
            .and_then(Object::as_i64)
            .ok()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0 && *n <= i32::MAX as u32)
    };
    let (Some(width), Some(height)) = (dimension(b"Width"), dimension(b"Height")) else {
        return Ok(skip(
            "raw raster dimensions must be positive direct integers within PNG bounds",
        ));
    };
    let predicted = match predictor::parameters(stream, width, channels) {
        Some(value) => value,
        None => {
            return Ok(skip(
                "unsupported DecodeParms; expected direct Flate Predictor 15 with matching 8-bit Gray/RGB geometry",
            ));
        }
    };
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(channels))
        .ok_or_else(|| Error::LimitExceeded("PDF decoded raster size overflow".into()))?;
    let inflated = expected
        .checked_add(if predicted { u64::from(height) } else { 0 })
        .ok_or_else(|| Error::LimitExceeded("PDF predictor row size overflow".into()))?;
    let charge = inflated
        .checked_add(if predicted { expected } else { 0 })
        .ok_or_else(|| Error::LimitExceeded("PDF predictor byte budget overflow".into()))?;
    if charge > *decoded_left || usize::try_from(inflated).is_err() {
        return Err(Error::LimitExceeded(
            "PDF decoded raster byte budget".into(),
        ));
    }
    *decoded_left -= charge;
    let expected = expected as usize;
    let raw = if predicted {
        // Decode only Flate here; our validated predictor is applied exactly once.
        let mut compressed = stream.clone();
        compressed.dict.remove(b"DecodeParms");
        let filtered = match compressed.get_plain_content_with_limit(inflated as usize) {
            Ok(data) if data.len() == inflated as usize => data,
            _ => {
                return Ok(skip(
                    "Predictor 15 Flate bytes violate the exact row-data bound",
                ));
            }
        };
        match predictor::decode(
            &filtered,
            expected / height as usize,
            height as usize,
            channels as usize,
        ) {
            Some(raw) => raw,
            None => {
                return Ok(skip(
                    "Predictor 15 row data has an invalid length or filter tag",
                ));
            }
        }
    } else if filter.is_none() {
        if stream.content.len() != expected {
            return Ok(skip(
                "raw raster byte length disagrees with dimensions/channels",
            ));
        }
        stream.content.clone()
    } else {
        match stream.get_plain_content_with_limit(expected) {
            Ok(data) if data.len() == expected => data,
            Ok(_) => {
                return Ok(skip(
                    "decoded Flate raster byte length disagrees with dimensions/channels",
                ));
            }
            Err(_) => {
                return Ok(skip(
                    "Flate raster could not be decoded within the exact expected byte bound",
                ));
            }
        }
    };
    let mut output = CappedOutput {
        bytes: Vec::new(),
        limit: output_limit.min(usize::MAX as u64) as usize,
        exceeded: false,
    };
    let result = (|| -> std::result::Result<(), png::EncodingError> {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&raw)?;
        writer.finish()
    })();
    if output.exceeded {
        return Err(Error::LimitExceeded("PDF PNG image byte budget".into()));
    }
    result.map_err(|e| Error::Parse(format!("PDF raster PNG encoding: {e}")))?;
    Ok(ImageData::Encoded(EncodedImage {
        bytes: output.bytes,
        extension: "png",
        media_type: "image/png",
    }))
}
struct CappedOutput {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Write for CappedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("PNG output exceeds byte budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn predicted_stream() -> Stream {
        Stream::new(
            lopdf::dictionary! {
                "Width" => 2, "Height" => 2, "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8, "Filter" => "FlateDecode",
                "DecodeParms" => lopdf::dictionary! {"Predictor" => 15, "Columns" => 2}
            },
            vec![120, 156, 99, 224, 18, 97, 18, 17, 1, 0, 0, 202, 0, 73],
        )
    }
    #[test]
    fn predictor_stream_to_png_round_trips_and_preserves_input() {
        let stream = predicted_stream();
        let original = stream.clone();
        let mut left = 10;
        let ImageData::Encoded(image) = extract(&stream, &mut left, 4096).unwrap() else {
            panic!("predictor image skipped");
        };
        assert_eq!(left, 0);
        assert_eq!(stream, original);
        assert_eq!(image.media_type, "image/png");
        let mut reader = png::Decoder::new(std::io::Cursor::new(image.bytes))
            .read_info()
            .unwrap();
        let mut out = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut out).unwrap();
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(&out[..frame.buffer_size()], &[10, 20, 30, 40]);
    }
    #[test]
    fn predictor_budgets_and_unsupported_parameters_are_enforced() {
        let stream = predicted_stream();
        assert!(matches!(
            extract(&stream, &mut 9, 4096),
            Err(Error::LimitExceeded(_))
        ));
        assert!(matches!(
            extract(&stream, &mut 10, 0),
            Err(Error::LimitExceeded(_))
        ));
        let mut left = 20;
        for _ in 0..2 {
            assert!(matches!(
                extract(&stream, &mut left, 4096),
                Ok(ImageData::Encoded(_))
            ));
        }
        assert!(matches!(
            extract(&stream, &mut left, 4096),
            Err(Error::LimitExceeded(_))
        ));
        for key in ["Mask", "SMask", "Decode"] {
            let mut bad = stream.clone();
            bad.dict.set(key, Object::Null);
            assert!(matches!(
                extract(&bad, &mut 100, 4096),
                Ok(ImageData::Skipped(_))
            ));
        }
        let mut bad = stream.clone();
        bad.dict.set("Height", 1);
        assert!(matches!(
            extract(&bad, &mut 100, 4096),
            Ok(ImageData::Skipped(_))
        ));
    }
    #[test]
    fn encoded_output_budget_is_enforced() {
        let mut output = CappedOutput {
            bytes: Vec::new(),
            limit: 2,
            exceeded: false,
        };
        assert!(output.write_all(&[1, 2, 3]).is_err());
        assert!(output.exceeded);
        assert!(output.bytes.is_empty());
    }
}
