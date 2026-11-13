//! Explicit image-bearing projection; independent of canonical PDF extraction.
use crate::{
    BlockKind, Diagnostic, Document, Error, ExtractedAsset, Options, PdfElementKind, Result,
    SourceSpan,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Experimental PDF projection with raw text-show runs and supported image occurrences.
///
/// Calling this function is the opt-in; ordinary extraction is unchanged. Available
/// with the `pdf` feature. Returns in-memory assets and writes no files. Repeated
/// indirect XObjects share bytes, while every supported occurrence gets an image block.
/// Each text-show run is a separate paragraph, NOT canonical paragraph reconstruction.
/// Traversal order is not visual reading order. No OCR, furniture removal, outline
/// insertion, dehyphenation, caption inference, image decoding or color conversion occurs.
/// Only single DCTDecode/JPXDecode streams without masks/custom Decode/DecodeParms
/// are copied. Other images are diagnosed and omitted. The returned image-bearing
/// model uses the provisional omniattach-documents/0.2 schema.
pub fn extract_pdf_with_images(name: &str, bytes: &[u8], options: &Options) -> Result<Document> {
    let inspection = crate::inspect_pdf_bytes(name, bytes, options)?;
    let plan = inspection.naming_plan()?;
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(
                options.limits.archive_bytes.min(usize::MAX as u64) as usize
            ),
            ..Default::default()
        },
    )
    .map_err(|e| Error::Parse(format!("PDF image projection: {e}")))?;
    if pdf.is_encrypted() {
        return Err(Error::InvalidInput("encrypted PDF is unsupported".into()));
    }
    let mut doc = Document {
        schema_version: "omniattach-documents/0.2".into(),
        source: inspection.source.clone(),
        title: None,
        format: "pdf".into(),
        extractor: format!(
            "omniattach-documents/{}/pdf-images",
            env!("CARGO_PKG_VERSION")
        ),
        blocks: Vec::new(),
        tables: Vec::new(),
        assets: Vec::new(),
        diagnostics: inspection.diagnostics.clone(),
    };
    let names: BTreeMap<_, _> = plan
        .elements()
        .iter()
        .map(|n| (n.element_sequence(), n))
        .collect();
    let mut asset_ids: BTreeMap<lopdf::ObjectId, String> = BTreeMap::new();
    let mut asset_bytes = 0u64;
    for element in &inspection.elements {
        let span = SourceSpan {
            page: Some(element.anchor.page),
            path: Some(format!(
                "content-operation {} forms {:?}",
                element.anchor.operation_visit, element.anchor.form_path
            )),
            ..Default::default()
        };
        let (kind, text) = match element.kind {
            PdfElementKind::Text => {
                let Some(text) = element.text.as_ref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                (BlockKind::Paragraph, text.clone())
            }
            PdfElementKind::Image => {
                let Some(id) = element.anchor.object_id else {
                    skipped(&mut doc, &span, "image has no indirect XObject ID".into());
                    continue;
                };
                let asset_id = if let Some(existing) = asset_ids.get(&id) {
                    existing.clone()
                } else {
                    let stream = match pdf.get_object(id) {
                        Ok(lopdf::Object::Stream(s)) => s,
                        _ => {
                            skipped(
                                &mut doc,
                                &span,
                                "image XObject did not resolve to a stream".into(),
                            );
                            continue;
                        }
                    };
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
                            .is_ok_and(|v| !matches!(v, lopdf::Object::Boolean(false)))
                    {
                        skipped(
                            &mut doc,
                            &span,
                            "image masks, custom Decode or DecodeParms are unsupported".into(),
                        );
                        continue;
                    }
                    let filter = match stream.dict.get(b"Filter") {
                        Ok(lopdf::Object::Name(n)) => Some(n.as_slice()),
                        Ok(lopdf::Object::Array(a)) if a.len() == 1 => a[0].as_name().ok(),
                        _ => None,
                    };
                    let (ext, media) = match filter {
                        Some(b"DCTDecode") => ("jpg", "image/jpeg"),
                        Some(b"JPXDecode") => ("jp2", "image/jp2"),
                        _ => {
                            skipped(
                                &mut doc,
                                &span,
                                "only a single DCTDecode or JPXDecode filter is supported".into(),
                            );
                            continue;
                        }
                    };
                    let next_bytes = asset_bytes
                        .checked_add(stream.content.len() as u64)
                        .ok_or_else(|| Error::LimitExceeded("PDF image byte overflow".into()))?;
                    if next_bytes > options.limits.archive_bytes
                        || doc.assets.len() >= options.limits.archive_entries
                    {
                        return Err(Error::LimitExceeded("PDF image bytes or count".into()));
                    }
                    let named = names
                        .get(&element.sequence)
                        .ok_or_else(|| Error::InvalidInput("missing image naming entry".into()))?;
                    let asset_id = format!("{}:a{}", doc.source.sha256, doc.assets.len() + 1);
                    let source = SourceSpan {
                        path: Some(format!(
                            "object {} {}; {}",
                            id.0,
                            id.1,
                            span.path.as_deref().unwrap_or("")
                        )),
                        ..span.clone()
                    };
                    doc.assets.push(ExtractedAsset {
                        id: asset_id.clone(),
                        filename: named.image_path(ext)?,
                        media_type: media.into(),
                        sha256: format!("{:x}", Sha256::digest(&stream.content)),
                        byte_length: stream.content.len(),
                        bytes: stream.content.clone(),
                        source,
                    });
                    asset_bytes = next_bytes;
                    asset_ids.insert(id, asset_id.clone());
                    asset_id
                };
                (
                    BlockKind::Image { asset_id },
                    format!("Image occurrence {}", element.sequence),
                )
            }
            // Hints overlap source runs; do not duplicate source text.
            _ => continue,
        };
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("PDF image projection blocks".into()));
        }
        doc.paragraph(kind, text, span);
    }
    doc.diagnostics.push(Diagnostic {
        code: "LFE_IMAGE_EXPORT_EXPERIMENTAL".into(),
        message: "Raw text-show runs and image occurrences follow content-stream traversal, not visual reading order. Each text run is a separate paragraph; canonical paragraph joining, furniture removal, outlines and dehyphenation are not applied. JPEG/JP2 bytes are copied without codec validation or color conversion; clipping, visibility, inline images, vector/composite figures and captions are not reconstructed. No OCR is invoked.".into(),
        source: None,
    });
    Ok(doc)
}
fn skipped(doc: &mut Document, span: &SourceSpan, message: String) {
    doc.diagnostics.push(Diagnostic {
        code: "PDF_IMAGE_SKIPPED".into(),
        message,
        source: Some(span.clone()),
    });
}
