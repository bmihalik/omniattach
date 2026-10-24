use crate::{BlockKind, Diagnostic, Document, Error, Options, Result, SourceSpan};
use std::collections::BTreeSet;

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
    for page in pages.keys() {
        let span = SourceSpan {
            page: Some(*page),
            ..Default::default()
        };
        let text = pdf
            .extract_text_with_limit(&[*page], options.limits.archive_bytes as usize)
            .map_err(|e| Error::Parse(format!("PDF page {page}: {e}")))?;
        if text.trim().is_empty() {
            doc.diagnostics.push(Diagnostic {
                code: "PAGE_WITHOUT_TEXT".into(),
                message: format!("Page {page} contains no extractable text; OCR is not included"),
                source: Some(span),
            });
        } else {
            doc.paragraph(BlockKind::Paragraph, text.trim_end().into(), span);
        }
    }
    let mut seen = BTreeSet::new();
    for (page, page_id) in &pages {
        let images = match pdf.get_page_images(*page_id) {
            Ok(images) => images,
            Err(e) => {
                skip_image(doc, *page, *page_id, format!("page {page}: image listing failed: {e}"));
                continue;
            }
        };
        for image in images {
            if seen.insert(image.id) {
                attach_image(doc, &pdf, *page, image.id, options)?;
            }
        }
    }
    doc.warn("PDF_TEXT_ONLY", "PDF text/content-stream order only; no OCR, layout reconstruction, headings, tables or equations. Embedded JPEG and JPEG 2000 images are extracted as assets; other image encodings are reported as skipped");
    Ok(())
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
