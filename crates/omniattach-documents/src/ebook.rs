//! Bounded best-effort projection of DRM-free MOBI text through the HTML reader.
use crate::{Document, Error, Format, Options, Result};

fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("MOBI/AZW3: {message}"))
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes.get(offset..offset + 2).ok_or_else(|| invalid("truncated header"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]) as usize)
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes.get(offset..offset + 4).ok_or_else(|| invalid("truncated header"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]) as usize)
}

pub(crate) fn extract(doc: &mut Document, bytes: &[u8], format: Format, options: &Options) -> Result<()> {
    if bytes.get(60..68) != Some(b"BOOKMOBI") {
        return Err(invalid("expected BOOKMOBI Palm database signature"));
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
        if offset < previous || offset >= bytes.len() || (index > 0 && offset == previous) {
            return Err(invalid("invalid record offsets"));
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
        return Err(invalid("DRM/encrypted content is unsupported; no decryption is attempted"));
    }
    let compression = u16_at(header, 0)?;
    if !matches!(compression, 1 | 2) {
        return Err(invalid("HUFF/CDIC and unknown compression are unsupported"));
    }
    let length = u32_at(header, 20)?;
    if length < 24 || length.checked_add(16).is_none_or(|n| n > header.len()) {
        return Err(invalid("invalid MOBI header length"));
    }
    let version = u32_at(header, 36)?;
    if format == Format::Mobi && version >= 8 {
        return Err(invalid("KF8 content requires explicit Format::Azw3 and the azw3 feature"));
    }
    // Extra-record-data flags are a u16 at absolute record-zero offset 242.
    if length >= 228 && u16_at(header, 242)? != 0 {
        return Err(invalid("record trailers are unsupported"));
    }
    if u32_at(header, 4)? as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded("MOBI declared decoded bytes".into()));
    }
    let text_count = u16_at(header, 8)?;
    if text_count == 0 || text_count >= count {
        return Err(invalid("invalid text record count"));
    }
    let last = text_count + 1;
    let start = second;
    let end = if last == count { bytes.len() } else { u32_at(bytes, 78 + last * 8)? };
    let factor = if compression == 2 { 8 } else { 1 };
    let bound = (end - start).checked_mul(factor).and_then(|n| n.checked_mul(3))
        .ok_or_else(|| Error::LimitExceeded("MOBI decoded size overflow".into()))?;
    if bound as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded("MOBI conservative decoded byte bound".into()));
    }
    let book = mobi::Mobi::from_read(bytes).map_err(|e| invalid(&format!("parser: {e}")))?;
    let range = book.readable_records_range();
    if range.start != 1 || range.end != last {
        return Err(invalid("unsupported text range or hybrid container"));
    }
    let html = book.content_as_string().map_err(|e| invalid(&format!("decoding: {e}")))?;
    if html.trim().is_empty() {
        return Err(invalid("no readable text"));
    }
    if html.len() as u64 > options.limits.archive_bytes {
        return Err(Error::LimitExceeded("MOBI decoded HTML bytes".into()));
    }
    crate::html::extract(doc, &html, None, options)?;
    let title = book.title();
    if !title.trim().is_empty() {
        doc.title = Some(title);
    }
    doc.warn("MOBI_TEXT_PROJECTION", "DRM-free raw MOBI text projected through HTML; images, fonts, navigation, links and page layout are not preserved. Source spans do not identify original compressed record offsets.");
    if format == Format::Azw3 {
        doc.warn("AZW3_EXPERIMENTAL_PROJECTION", "KF8 skeletons/fragments and hybrid boundaries are not reconstructed; completeness and reading order are not guaranteed.");
    }
    Ok(())
}
