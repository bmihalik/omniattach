//! Experimental raw MOBI/KF8 text projection, not a KF8 layout reconstructor.
use mobi::{Mobi, headers::Compression};
use omniattach_documents::{Document, Error, Format, Options, Result, extract_as};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read};

fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("AZW3: {message}"))
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated header"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]) as usize)
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated header"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]) as usize)
}

fn validate(bytes: &[u8], options: &Options) -> Result<()> {
    if bytes.len() > options.limits.input_bytes {
        return Err(Error::LimitExceeded("AZW3 input bytes".into()));
    }
    if bytes.get(60..68) != Some(b"BOOKMOBI") {
        return Err(invalid(
            "expected a BOOKMOBI Palm database; KFX and ZIP files are unsupported",
        ));
    }
    let count = u16_at(bytes, 76)?;
    if count < 2 || count > options.limits.archive_entries {
        return Err(invalid("invalid or excessive record count"));
    }
    let table_end = 78 + count * 8;
    if table_end > bytes.len() {
        return Err(invalid("truncated record table"));
    }
    let mut previous = table_end;
    for index in 0..count {
        let offset = u32_at(bytes, 78 + index * 8)?;
        if offset < previous || offset >= bytes.len() {
            return Err(invalid(
                "record offsets are outside the file or not increasing",
            ));
        }
        if index > 0 && offset == previous {
            return Err(invalid("empty records are unsupported"));
        }
        previous = offset;
    }
    let first = u32_at(bytes, 78)?;
    let second = u32_at(bytes, 86)?;
    let header = &bytes[first..second];
    if header.get(16..20) != Some(b"MOBI") {
        return Err(invalid("missing MOBI header"));
    }
    if u16_at(header, 12)? != 0 {
        return Err(invalid(
            "encrypted/DRM-protected content is unsupported; no decryption is attempted",
        ));
    }
    if !matches!(u16_at(header, 0)?, 1 | 2) {
        return Err(invalid(
            "only uncompressed and PalmDOC records are supported; HUFF/CDIC is unsupported",
        ));
    }
    let header_len = u32_at(header, 20)?;
    if header_len < 24
        || header_len
            .checked_add(16)
            .is_none_or(|end| end > header.len())
    {
        return Err(invalid("invalid MOBI header length"));
    }
    if header_len >= 228 && u32_at(header, 240)? != 0 {
        return Err(invalid(
            "record trailers are unsupported by this experimental adapter",
        ));
    }
    if u32_at(header, 4)? as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded("AZW3 declared decoded bytes".into()));
    }
    Ok(())
}

fn extract(name: &str, bytes: &[u8], options: &Options) -> Result<Document> {
    validate(bytes, options)?;
    let book = Mobi::from_read(bytes).map_err(|e| invalid(&format!("parser: {e}")))?;
    let range = book.readable_records_range();
    let records = u16_at(bytes, 76)?;
    let text_records = u16_at(&bytes[u32_at(bytes, 78)?..], 8)?;
    if range.start != 1
        || range.end <= range.start
        || range.end > records
        || range.len() != text_records
    {
        return Err(invalid(
            "unsupported text record range; hybrid/complex containers are not reconstructed",
        ));
    }
    let start = u32_at(bytes, 78 + range.start * 8)?;
    let end = if range.end == records {
        bytes.len()
    } else {
        u32_at(bytes, 78 + range.end * 8)?
    };
    let expansion = if matches!(book.compression(), Compression::PalmDoc) {
        8
    } else {
        1
    };
    let bound = (end - start)
        .checked_mul(expansion)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| Error::LimitExceeded("AZW3 decoded size overflow".into()))?;
    if bound as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded(
            "AZW3 conservative decoded byte bound".into(),
        ));
    }
    let html = book
        .content_as_string()
        .map_err(|e| invalid(&format!("text decoding: {e}")))?;
    if html.trim().is_empty() {
        return Err(invalid("no readable text was extracted"));
    }
    if html.len() > options.limits.input_bytes || html.len() as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded("AZW3 decoded HTML bytes".into()));
    }
    let mut document = extract_as(name, html.as_bytes(), Format::Html, options)?;
    document.source.name = name.into();
    document.source.sha256 = format!("{:x}", Sha256::digest(bytes));
    document.source.byte_length = bytes.len();
    document.source.encoding = None;
    document.format = "azw3".into();
    let title = book.title();
    if !title.trim().is_empty() {
        document.title = Some(title);
    }
    document.warn("AZW3_EXPERIMENTAL_PROJECTION", "Raw MOBI text projected through HTML; KF8 skeleton/fragment reconstruction, reading order, chapters, styles, images, fonts and links are not preserved or verified. Hybrid books are not guaranteed to be supported.");
    Ok(document)
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: azw3-extract INPUT [json|markdown|text]")?;
    let output = args.next().unwrap_or_else(|| "json".into());
    if args.next().is_some() || !matches!(output.as_str(), "json" | "markdown" | "text") {
        return Err("usage: azw3-extract INPUT [json|markdown|text]".into());
    }
    let options = Options::default();
    let mut bytes = Vec::new();
    File::open(&input)?
        .take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    let document = extract(&input, &bytes, &options)?;
    eprintln!(
        "Experimental AZW3 projection: reading order and completeness are not guaranteed; inspect JSON diagnostics."
    );
    match output.as_str() {
        "markdown" => println!("{}", document.to_markdown()),
        "text" => println!("{}", document.to_text()),
        _ => println!("{}", serde_json::to_string_pretty(&document)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_mobi_and_truncation() {
        let options = Options::default();
        for bytes in [b"".as_slice(), b"not a book", b"PK\x03\x04"] {
            assert!(validate(bytes, &options).is_err());
        }
        let mut bytes = vec![0; 78];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        bytes[76..78].copy_from_slice(&2u16.to_be_bytes());
        assert!(validate(&bytes, &options).is_err());
    }

    #[test]
    fn enforces_input_limit_before_parsing() {
        let mut options = Options::default();
        options.limits.input_bytes = 1;
        assert!(matches!(
            validate(b"too big", &options),
            Err(Error::LimitExceeded(_))
        ));
    }
}
