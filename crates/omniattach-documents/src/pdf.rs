use crate::{BlockKind, Diagnostic, Document, Error, Options, Result, SourceSpan};
use std::collections::{BTreeMap, BTreeSet};

// The old TJ-rewriting shim is replaced in place by the line-aware reader.
#[path = "pdf_spacing.rs"]
mod lines;

const MAX_OUTLINE_ITEMS: usize = 10_000;
const MAX_OUTLINE_DEPTH: u32 = 32;

pub(crate) fn extract(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(
                options.limits.archive_bytes.min(usize::MAX as u64) as usize
            ),
            ..Default::default()
        },
    )
    .map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    if pdf.is_encrypted() {
        return Err(Error::InvalidInput(
            "encrypted PDF is unsupported; decrypt explicitly before extraction".into(),
        ));
    }
    let pages = pdf.get_pages();
    if pages.len() > options.limits.blocks {
        return Err(Error::LimitExceeded("PDF pages".into()));
    }
    outline_headings(doc, &pdf, &pages);
    let mut extracted = Vec::new();
    let mut ocr_pages = Vec::new();
    let mut missing_pages = Vec::new();
    let mut total_text = 0u64;
    let mut real_gaps = 0usize;
    for (page, id) in &pages {
        let span = SourceSpan {
            page: Some(*page),
            ..Default::default()
        };
        let mut text = lines::page(&pdf, *id, options)?;
        real_gaps += text.real_gaps;
        if text.undecoded > 0 {
            doc.diagnostics.push(Diagnostic {
                code: "PDF_UNDECODED_TEXT".into(),
                message: format!(
                    "{} PDF text strings could not be fully decoded on page {page}",
                    text.undecoded
                ),
                source: Some(span.clone()),
            });
        }
        let mut ocr_source = false;
        if text.lines.iter().all(|l| l.trim().is_empty()) {
            if let Some(ocr) = &options.pdf_ocr {
                match (ocr.0)(*page) {
                    Ok(recognized) if !recognized.trim().is_empty() => {
                        text.lines = recognized.lines().map(str::to_owned).collect();
                        ocr_source = true;
                    }
                    Ok(_) => doc.diagnostics.push(Diagnostic {
                        code: "PAGE_WITHOUT_TEXT".into(),
                        message: format!("Page {page} has no text after OCR"),
                        source: Some(span.clone()),
                    }),
                    Err(e) => doc.diagnostics.push(Diagnostic {
                        code: "PDF_PAGE_OCR_FAILED".into(),
                        message: format!("Page {page} OCR failed: {e}"),
                        source: Some(span.clone()),
                    }),
                }
            } else {
                doc.diagnostics.push(Diagnostic {
                    code: "PAGE_WITHOUT_TEXT".into(),
                    message: format!(
                        "Page {page} contains no extractable text; OCR is not included"
                    ),
                    source: Some(span),
                });
            }
        }
        missing_pages.push(text.lines.iter().all(|l| l.trim().is_empty()));
        for line in &text.lines {
            total_text = total_text
                .checked_add(line.len() as u64)
                .ok_or_else(|| Error::LimitExceeded("PDF text bytes".into()))?;
        }
        if total_text > options.limits.archive_bytes {
            return Err(Error::LimitExceeded("PDF text bytes".into()));
        }
        extracted.push(text);
        ocr_pages.push(ocr_source);
    }
    if real_gaps > 0 {
        doc.warn("PDF_TJ_SPACING_NORMALIZED",format!("Interpreted {real_gaps} real-valued TJ word-gap adjustments directly; no PDF streams or source bytes were rewritten"));
    }
    let removed = lines::remove_furniture(&mut extracted);
    if removed > 0 {
        doc.warn("PDF_PAGE_FURNITURE_REMOVED",format!("Removed {removed} page-number or repeated running-header/footer lines using page-edge heuristics"));
    }
    let mut paragraph = String::new();
    let mut source = SourceSpan::default();
    for (index, ((page, _), text)) in pages.iter().zip(&extracted).enumerate() {
        if missing_pages[index]
            || (!paragraph.is_empty()
                && (source.path.as_deref() == Some("ocr")) != ocr_pages[index])
        {
            flush_paragraph(doc, &mut paragraph, &source, options)?;
        }
        let start = text.lines.iter().position(|l| !l.trim().is_empty());
        let end = text.lines.iter().rposition(|l| !l.trim().is_empty());
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        for line in &text.lines[start..=end] {
            if line.trim().is_empty() {
                flush_paragraph(doc, &mut paragraph, &source, options)?;
            } else {
                if paragraph.is_empty() {
                    source = SourceSpan {
                        page: Some(*page),
                        path: ocr_pages[index].then(|| "ocr".into()),
                        ..Default::default()
                    };
                } else {
                    paragraph.push('\n');
                }
                paragraph.push_str(line.trim_end());
            }
        }
    }
    flush_paragraph(doc, &mut paragraph, &source, options)?;
    let mut seen = BTreeSet::new();
    for (page, page_id) in &pages {
        let images = match pdf.get_page_images(*page_id) {
            Ok(images) => images,
            Err(e) => {
                skip_image(
                    doc,
                    *page,
                    *page_id,
                    format!("page {page}: image listing failed: {e}"),
                );
                continue;
            }
        };
        for image in images {
            if seen.insert(image.id) {
                attach_image(doc, &pdf, *page, image.id, options)?;
            }
        }
    }
    doc.warn("PDF_TEXT_ONLY", "Line-aware PDF text in content-stream order, with geometric line/paragraph heuristics and page-edge furniture removal. Paragraphs may span pages; source.page is the first page. No de-hyphenation, column sorting, vertical-writing or general rotated-layout reconstruction, tables or equations. Bookmarks become headings; empty pages use only a supplied PdfOcr callback. Embedded JPEG/JP2 assets are retained; other image encodings are reported as skipped.");
    Ok(())
}
fn flush_paragraph(
    doc: &mut Document,
    text: &mut String,
    span: &SourceSpan,
    options: &Options,
) -> Result<()> {
    if !text.is_empty() {
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("PDF blocks".into()));
        }
        doc.paragraph(BlockKind::Paragraph, std::mem::take(text), span.clone());
    }
    Ok(())
}

fn dict_of<'a>(
    pdf: &'a lopdf::Document,
    object: &'a lopdf::Object,
) -> Option<&'a lopdf::Dictionary> {
    match object {
        lopdf::Object::Dictionary(dictionary) => Some(dictionary),
        lopdf::Object::Reference(id) => pdf.get_dictionary(*id).ok(),
        _ => None,
    }
}
fn decode_pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect())
}
/// Bookmarks remain in outline order, before page text; named destinations are not resolved.
fn outline_headings(
    doc: &mut Document,
    pdf: &lopdf::Document,
    pages: &BTreeMap<u32, lopdf::ObjectId>,
) {
    let Ok(root) = pdf.trailer.get(b"Root") else {
        return;
    };
    let Some(catalog) = dict_of(pdf, root) else {
        return;
    };
    let Some(outlines) = catalog.get(b"Outlines").ok().and_then(|o| dict_of(pdf, o)) else {
        return;
    };
    let Ok(first) = outlines.get(b"First") else {
        return;
    };
    let page_numbers: BTreeMap<lopdf::ObjectId, u32> =
        pages.iter().map(|(number, id)| (*id, *number)).collect();
    let mut visited = BTreeSet::new();
    let mut count = 0usize;
    walk_outline(
        doc,
        pdf,
        &page_numbers,
        Some(first),
        1,
        &mut visited,
        &mut count,
    );
}
fn walk_outline<'a>(
    doc: &mut Document,
    pdf: &'a lopdf::Document,
    page_numbers: &BTreeMap<lopdf::ObjectId, u32>,
    mut current: Option<&'a lopdf::Object>,
    depth: u32,
    visited: &mut BTreeSet<lopdf::ObjectId>,
    count: &mut usize,
) {
    while let Some(object) = current {
        if let lopdf::Object::Reference(id) = object
            && !visited.insert(*id)
        {
            return;
        }
        if depth > MAX_OUTLINE_DEPTH || *count >= MAX_OUTLINE_ITEMS {
            doc.warn("PDF_OUTLINE_TRUNCATED","bookmark tree exceeds the depth or item budget; remaining bookmarks are not extracted");
            return;
        }
        let Some(item) = dict_of(pdf, object) else {
            return;
        };
        *count += 1;
        let title = item
            .get(b"Title")
            .ok()
            .and_then(|t| t.as_str().ok())
            .map(decode_pdf_text)
            .unwrap_or_default();
        if !title.trim().is_empty() {
            let span = SourceSpan {
                page: destination_page(pdf, item, page_numbers),
                path: Some("outline".into()),
                ..Default::default()
            };
            let title_before = doc.title.clone();
            doc.heading(depth.clamp(1, 6) as u8, title, span);
            doc.title = title_before;
        }
        if let Ok(child) = item.get(b"First") {
            walk_outline(
                doc,
                pdf,
                page_numbers,
                Some(child),
                depth + 1,
                visited,
                count,
            );
        }
        current = item.get(b"Next").ok();
    }
}
fn destination_page(
    pdf: &lopdf::Document,
    item: &lopdf::Dictionary,
    page_numbers: &BTreeMap<lopdf::ObjectId, u32>,
) -> Option<u32> {
    let dest = item
        .get(b"Dest")
        .ok()
        .or_else(|| dict_of(pdf, item.get(b"A").ok()?)?.get(b"D").ok())?;
    let lopdf::Object::Array(target) = dest else {
        return None;
    };
    let lopdf::Object::Reference(id) = target.first()? else {
        return None;
    };
    page_numbers.get(id).copied()
}
fn skip_image(doc: &mut Document, page: u32, id: lopdf::ObjectId, message: String) {
    doc.diagnostics.push(Diagnostic {
        code: "PDF_IMAGE_SKIPPED".into(),
        message,
        source: Some(SourceSpan {
            page: Some(page),
            path: Some(format!("object {} {}", id.0, id.1)),
            ..Default::default()
        }),
    });
}
fn filter_names(dict: &lopdf::Dictionary) -> Vec<String> {
    match dict.get(b"Filter") {
        Ok(lopdf::Object::Name(name)) => vec![String::from_utf8_lossy(name).into_owned()],
        Ok(lopdf::Object::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_name().ok())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect(),
        _ => Vec::new(),
    }
}
fn attach_image(
    doc: &mut Document,
    pdf: &lopdf::Document,
    page: u32,
    id: lopdf::ObjectId,
    options: &Options,
) -> Result<()> {
    let Ok(lopdf::Object::Stream(stream)) = pdf.get_object(id) else {
        skip_image(doc, page, id, "image object is not a stream".into());
        return Ok(());
    };
    let filters = filter_names(&stream.dict);
    let (extension, media_type) = match filters.as_slice() {
        [f] if f == "DCTDecode" => ("jpg", "image/jpeg"),
        [f] if f == "JPXDecode" => ("jp2", "image/jp2"),
        _ => {
            skip_image(
                doc,
                page,
                id,
                format!("filters {filters:?} produce raw pixels, not a standalone image file"),
            );
            return Ok(());
        }
    };
    let bytes = stream.content.clone();
    let total = doc
        .assets
        .iter()
        .map(|a| a.byte_length as u64)
        .sum::<u64>()
        .saturating_add(bytes.len() as u64);
    if total > options.limits.archive_bytes || doc.assets.len() >= options.limits.archive_entries {
        return Err(Error::LimitExceeded("embedded asset bytes or count".into()));
    }
    let filename = format!("page{page}-object{}-{}.{extension}", id.0, id.1);
    let source = SourceSpan {
        page: Some(page),
        path: Some(format!("object {} {}", id.0, id.1)),
        ..Default::default()
    };
    doc.asset(filename, media_type, bytes, source);
    Ok(())
}
