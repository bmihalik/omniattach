//! Narrow native raster conversion; never guesses filters, bit depth or color space.
use crate::{Error, Result};
use lopdf::{Object, Stream};
use std::io::{self, Write};

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
        b"DecodeParms".as_slice(),
    ]
    .iter()
    .any(|key| stream.dict.get(key).is_ok())
        || stream
            .dict
            .get(b"ImageMask")
            .is_ok_and(|v| !matches!(v, Object::Boolean(false)))
    {
        return Ok(skip(
            "image masks, custom Decode or DecodeParms/predictors are unsupported",
        ));
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
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(channels))
        .ok_or_else(|| Error::LimitExceeded("PDF decoded raster size overflow".into()))?;
    if expected > *decoded_left || usize::try_from(expected).is_err() {
        return Err(Error::LimitExceeded(
            "PDF decoded raster byte budget".into(),
        ));
    }
    *decoded_left -= expected;
    let expected = expected as usize;
    let raw = if filter.is_none() {
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
