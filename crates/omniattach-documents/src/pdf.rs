use crate::{BlockKind, Diagnostic, Document, Error, Options, Result, SourceSpan};
use std::collections::{BTreeMap, BTreeSet};

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
    for page in pages.keys() {
        let span = SourceSpan {
            page: Some(*page),
            ..Default::default()
        };
        let text = pdf
            .extract_text_with_limit(&[*page], options.limits.archive_bytes as usize)
            .map_err(|e| Error::Parse(format!("PDF page {page}: {e}")))?;
        if !text.trim().is_empty() {
            doc.paragraph(BlockKind::Paragraph, text.trim_end().into(), span);
        } else if let Some(ocr) = &options.pdf_ocr {
            let recognized = (ocr.0)(*page)?;
            if recognized.trim().is_empty() {
                doc.diagnostics.push(Diagnostic {
                    code: "PAGE_WITHOUT_TEXT".into(),
                    message: format!("Page {page} has no text after OCR"),
                    source: Some(span),
                });
            } else {
                let mut span = span;
                span.path = Some("ocr".into());
                doc.paragraph(BlockKind::Paragraph, recognized.trim_end().into(), span);
            }
        } else {
            doc.diagnostics.push(Diagnostic {
                code: "PAGE_WITHOUT_TEXT".into(),
                message: format!("Page {page} contains no extractable text; OCR is not included"),
                source: Some(span),
            });
        }
    }
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
    doc.warn("PDF_TEXT_ONLY", "PDF text/content-stream order only; bookmarks become headings; pages without text are OCR'd only when a PdfOcr callback is supplied; no layout reconstruction, tables or equations. Embedded JPEG and JPEG 2000 images are extracted as assets; other image encodings are reported as skipped");
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

/// PDF text strings are UTF-16BE when they start with the FE FF byte order mark.
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

/// Bookmarks become headings in outline order, before page text. Named destinations are not resolved.
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
            doc.warn(
                "PDF_OUTLINE_TRUNCATED",
                "bookmark tree exceeds the depth or item budget; remaining bookmarks are not extracted",
            );
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
            let page = destination_page(pdf, item, page_numbers);
            let span = SourceSpan {
                page,
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

/// Page number of a bookmark's destination, from /Dest or /A /GoTo /D. Named destinations give None.
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

/// Filter names of a stream dictionary, whether /Filter is a name or an array of names.
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
