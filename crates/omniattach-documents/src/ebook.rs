//! Bounded best-effort projection of DRM-free MOBI text through the HTML reader.
use crate::{Document, Error, Format, Options, Result};

#[path = "ebook_huff.rs"]
mod huff;
#[path = "ebook_flows.rs"]
mod flows;

fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("MOBI/AZW3: {message}"))
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

fn strip_trailers(mut record: &[u8], flags: usize) -> Result<&[u8]> {
    // Entries for bits 1..15 end in a backwards variable-width length,
    // including the bytes encoding that length. Bit 0 is the overlap suffix.
    for bit in 1..16 {
        if flags & (1 << bit) == 0 {
            continue;
        }
        let mut size = 0usize;
        let mut used = 0usize;
        let mut terminated = false;
        for &byte in record.iter().rev().take(4) {
            size |= usize::from(byte & 0x7f) << (used * 7);
            used += 1;
            if byte & 0x80 != 0 {
                terminated = true;
                break;
            }
        }
        if !terminated || size < used || size > record.len() {
            return Err(invalid("invalid trailing record-data length"));
        }
        record = &record[..record.len() - size];
    }
    if flags & 1 != 0 {
        let last = *record
            .last()
            .ok_or_else(|| invalid("missing overlap trailer"))?;
        let size = usize::from(last & 3) + 1;
        if size > record.len() {
            return Err(invalid("overlap trailer exceeds record"));
        }
        record = &record[..record.len() - size];
    }
    Ok(record)
}

fn append(out: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<()> {
    if bytes.len() > limit.saturating_sub(out.len()) {
        return Err(Error::LimitExceeded("ebook decoded bytes".into()));
    }
    out.extend_from_slice(bytes);
    Ok(())
}

fn palmdoc(record: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<()> {
    let base = out.len();
    let mut pos = 0;
    while pos < record.len() {
        let byte = record[pos];
        pos += 1;
        match byte {
            1..=8 => {
                let end = pos
                    .checked_add(usize::from(byte))
                    .filter(|&n| n <= record.len())
                    .ok_or_else(|| invalid("truncated PalmDOC literal run"))?;
                append(out, &record[pos..end], limit)?;
                pos = end;
            }
            0 | 9..=127 => append(out, &[byte], limit)?,
            128..=191 => {
                let next = *record
                    .get(pos)
                    .ok_or_else(|| invalid("truncated PalmDOC back-reference"))?;
                pos += 1;
                let pair = (usize::from(byte) << 8) | usize::from(next);
                let distance = (pair >> 3) & 0x7ff;
                let count = (pair & 7) + 3;
                if distance == 0 || distance > out.len() - base {
                    return Err(invalid("invalid PalmDOC back-reference distance"));
                }
                for _ in 0..count {
                    let value = out[out.len() - distance];
                    append(out, &[value], limit)?;
                }
            }
            _ => append(out, &[b' ', byte ^ 0x80], limit)?,
        }
    }
    Ok(())
}

fn decode_text(bytes: &[u8], encoding: usize) -> Result<String> {
    match encoding {
        65001 => String::from_utf8(bytes.to_vec()).map_err(|e| invalid(&format!("UTF-8: {e}"))),
        1252 => encoding_rs::WINDOWS_1252
            .decode_without_bom_handling_and_without_replacement(bytes)
            .map(|s| s.into_owned())
            .ok_or_else(|| invalid("invalid Windows-1252 text")),
        _ => Err(invalid(&format!("unsupported text encoding {encoding}"))),
    }
}

pub(crate) fn extract(
    doc: &mut Document,
    bytes: &[u8],
    format: Format,
    options: &Options,
) -> Result<()> {
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
    let mut offsets = Vec::with_capacity(count + 1);
    let mut previous = table_end;
    for index in 0..count {
        let offset = u32_at(bytes, 78 + index * 8)?;
        if offset < previous || offset >= bytes.len() || (index > 0 && offset == previous) {
            return Err(invalid("invalid record offsets"));
        }
        offsets.push(offset);
        previous = offset;
    }
    offsets.push(bytes.len());
    let record = |index: usize| -> &[u8] { &bytes[offsets[index]..offsets[index + 1]] };
    let header = record(0);
    if header.get(16..20) != Some(b"MOBI") {
        return Err(invalid("missing MOBI header"));
    }
    if u16_at(header, 12)? != 0 {
        return Err(invalid(
            "DRM/encrypted content is unsupported; no decryption is attempted",
        ));
    }
    let compression = u16_at(header, 0)?;
    if !matches!(compression, 1 | 2 | 17480) {
        return Err(invalid(&format!("unsupported compression {compression}")));
    }
    let length = u32_at(header, 20)?;
    if length < 24 || length.checked_add(16).is_none_or(|n| n > header.len()) {
        return Err(invalid("invalid MOBI header length"));
    }
    let version = u32_at(header, 36)?;
    if format == Format::Mobi && version >= 8 {
        return Err(invalid(
            "KF8 content requires explicit Format::Azw3 and the azw3 feature",
        ));
    }
    let flags = if length >= 228 {
        u16_at(header, 242)?
    } else {
        0
    };
    let limit = options.limits.archive_bytes.min(usize::MAX as u64) as usize;
    let expected = u32_at(header, 4)?;
    if expected > limit {
        return Err(Error::LimitExceeded("ebook declared decoded bytes".into()));
    }
    let text_count = u16_at(header, 8)?;
    if text_count == 0 || text_count >= count {
        return Err(invalid("invalid text record count"));
    }
    let mut output = Vec::new();
    if compression == 17480 {
        if length < 104 {
            return Err(invalid("missing HUFF/CDIC references"));
        }
        let first = u32_at(header, 112)?;
        let number = u32_at(header, 116)?;
        let end = first
            .checked_add(number)
            .filter(|&n| n <= count)
            .ok_or_else(|| invalid("HUFF/CDIC record range outside file"))?;
        if number < 2 || first <= text_count {
            return Err(invalid("invalid HUFF/CDIC record range"));
        }
        let dictionaries: Vec<&[u8]> = (first..end).map(record).collect();
        let decoder = huff::Decoder::new(&dictionaries, limit)?;
        let mut work = limit.saturating_mul(32).min(128 * 1024 * 1024);
        for index in 1..=text_count {
            decoder.decode(
                strip_trailers(record(index), flags)?,
                &mut output,
                limit,
                &mut work,
            )?;
        }
    } else {
        for index in 1..=text_count {
            let data = strip_trailers(record(index), flags)?;
            if compression == 1 {
                append(&mut output, data, limit)?;
            } else {
                palmdoc(data, &mut output, limit)?;
            }
        }
    }
    let (primary, ignored_flows) = if version == 8 && length >= 184 && u32_at(header, 196)? > 1 {
        let index = u32_at(header, 192)?;
        if index <= text_count || index >= count {
            return Err(invalid("FDST record reference outside non-text records"));
        }
        flows::first_flow(
            &output,
            expected,
            record(index),
            u32_at(header, 196)?,
            options.limits.archive_entries,
        )?
    } else {
        if output.len() != expected {
            return Err(invalid(&format!(
                "decoded text length mismatch: declared {expected}, decoded {}",
                output.len()
            )));
        }
        (output.as_slice(), 0)
    };
    let encoding = u32_at(header, 28)?;
    let html = decode_text(primary, encoding)?;
    if html.len() > limit {
        return Err(Error::LimitExceeded("ebook decoded UTF-8 bytes".into()));
    }
    if html.trim().is_empty() {
        return Err(invalid("no readable text"));
    }
    crate::html::extract(doc, &html, None, options)?;
    if ignored_flows > 0 {
        doc.warn("AZW3_SECONDARY_FLOWS_SKIPPED", format!("Validated FDST table covers {} decoded bytes; only the {expected}-byte primary flow was projected. {ignored_flows} secondary flows ({} bytes) were not rendered or exported.", output.len(), output.len() - primary.len()));
    }
    if length >= 76 {
        let start = u32_at(header, 84)?;
        let title_length = u32_at(header, 88)?;
        if title_length > 0 {
            let end = start
                .checked_add(title_length)
                .filter(|&n| n <= header.len())
                .ok_or_else(|| invalid("title outside record zero"))?;
            let title = decode_text(&header[start..end], encoding)?;
            if !title.trim().is_empty() {
                doc.title = Some(title);
            }
        }
    }
    doc.warn("MOBI_TEXT_PROJECTION", "DRM-free raw MOBI text projected through HTML; images, fonts, navigation, links and page layout are not preserved. Source spans do not identify original compressed record offsets.");
    if compression == 17480 {
        doc.warn("MOBI_HUFF_CDIC", "HUFF/CDIC text decompressed with bounded output, dictionary depth and work; full KF8 reconstruction is not performed.");
    }
    if format == Format::Azw3 {
        doc.warn("AZW3_EXPERIMENTAL_PROJECTION", "KF8 skeletons/fragments and hybrid boundaries are not reconstructed; completeness and reading order are not guaranteed.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trailer_flags_one_and_three() {
        assert_eq!(strip_trailers(b"abc\0", 1).unwrap(), b"abc");
        assert_eq!(strip_trailers(b"abc\0\x81", 3).unwrap(), b"abc");
        assert!(strip_trailers(b"\x84", 2).is_err());
        assert!(strip_trailers(b"", 1).is_err());
        assert!(strip_trailers(b"\0\0\0\0", 2).is_err());
    }
    #[test]
    fn palmdoc_literals_backrefs_and_limits() {
        let mut out = Vec::new();
        palmdoc(b"abc\x80\x18", &mut out, 6).unwrap();
        assert_eq!(out, b"abcabc");
        assert!(palmdoc(b"\x80\x18", &mut Vec::new(), 100).is_err());
        assert!(palmdoc(b"abcdef", &mut Vec::new(), 5).is_err());
    }
}
