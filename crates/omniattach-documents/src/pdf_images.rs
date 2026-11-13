//! Explicit image-bearing projection; independent of canonical PDF extraction.
use crate::{BlockKind, Diagnostic, Document, Error, ExtractedAsset, Options, PdfElementKind, Result, SourceSpan};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[path = "pdf_raster.rs"]
mod raster;

/// Experimental raw text-show and image-occurrence projection, enabled by `pdf`.
/// Calling this function is the opt-in; ordinary extraction is unchanged.
/// Returns in-memory assets and writes no files. Repeated indirect XObjects share
/// bytes. Every text-show run is a separate paragraph, not canonical paragraph
/// reconstruction. No OCR, furniture removal, outlines or dehyphenation is applied.
/// Single DCT/JPX streams are retained; unfiltered/single-Flate 8-bit DeviceGray/RGB
/// rasters become PNG. Masks, custom Decode/DecodeParms, other raster color spaces
/// and filter chains are diagnosed and omitted. Traversal order is not visual order;
/// clipping, captions, inline images and vector/composite figures are not reconstructed.
/// Image-bearing output uses the provisional omniattach-documents/0.2 schema.
pub fn extract_pdf_with_images(name: &str, bytes: &[u8], options: &Options) -> Result<Document> {
    let inspection = crate::inspect_pdf_bytes(name, bytes, options)?;
    let plan = inspection.naming_plan()?;
    let pdf = lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions {
        max_decompressed_size: Some(options.limits.archive_bytes.min(usize::MAX as u64) as usize),
        ..Default::default()
    }).map_err(|e| Error::Parse(format!("PDF image projection: {e}")))?;
    if pdf.is_encrypted() {
        return Err(Error::InvalidInput("encrypted PDF is unsupported".into()));
    }
    let mut doc = Document {
        schema_version: "omniattach-documents/0.2".into(),
        source: inspection.source.clone(), title: None, format: "pdf".into(),
        extractor: format!("omniattach-documents/{}/pdf-images", env!("CARGO_PKG_VERSION")),
        blocks: Vec::new(), tables: Vec::new(), assets: Vec::new(),
        diagnostics: inspection.diagnostics.clone(),
    };
    let names: BTreeMap<_, _> = plan.elements().iter().map(|n| (n.element_sequence(), n)).collect();
    let mut asset_ids: BTreeMap<lopdf::ObjectId, String> = BTreeMap::new();
    let mut asset_bytes = 0u64;
    let mut decoded_left = options.limits.archive_bytes;
    for element in &inspection.elements {
        let span = SourceSpan {
            page: Some(element.anchor.page),
            path: Some(format!("content-operation {} forms {:?}", element.anchor.operation_visit, element.anchor.form_path)),
            ..Default::default()
        };
        let (kind, text) = match element.kind {
            PdfElementKind::Text => {
                let Some(text) = element.text.as_ref().filter(|s| !s.is_empty()) else { continue; };
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
                            skipped(&mut doc, &span, "image XObject did not resolve to a stream".into());
                            continue;
                        }
                    };
                    if doc.assets.len() >= options.limits.archive_entries {
                        return Err(Error::LimitExceeded("PDF image count".into()));
                    }
                    let available = options.limits.archive_bytes.checked_sub(asset_bytes)
                        .ok_or_else(|| Error::LimitExceeded("PDF image bytes".into()))?;
                    let encoded = match raster::extract(stream, &mut decoded_left, available)? {
                        raster::ImageData::Encoded(image) => image,
                        raster::ImageData::Skipped(reason) => {
                            skipped(&mut doc, &span, reason);
                            continue;
                        }
                    };
                    let next_bytes = asset_bytes.checked_add(encoded.bytes.len() as u64)
                        .filter(|n| *n <= options.limits.archive_bytes)
                        .ok_or_else(|| Error::LimitExceeded("PDF image bytes".into()))?;
                    let named = names.get(&element.sequence)
                        .ok_or_else(|| Error::InvalidInput("missing image naming entry".into()))?;
                    let asset_id = format!("{}:a{}", doc.source.sha256, doc.assets.len() + 1);
                    let source = SourceSpan {
                        path: Some(format!("object {} {}; {}", id.0, id.1, span.path.as_deref().unwrap_or(""))),
                        ..span.clone()
                    };
                    doc.assets.push(ExtractedAsset {
                        id: asset_id.clone(), filename: named.image_path(encoded.extension)?,
                        media_type: encoded.media_type.into(),
                        sha256: format!("{:x}", Sha256::digest(&encoded.bytes)),
                        byte_length: encoded.bytes.len(), bytes: encoded.bytes, source,
                    });
                    asset_bytes = next_bytes;
                    asset_ids.insert(id, asset_id.clone());
                    asset_id
                };
                (BlockKind::Image { asset_id }, format!("Image occurrence {}", element.sequence))
            }
            _ => continue,
        };
        if doc.blocks.len() >= options.limits.blocks {
            return Err(Error::LimitExceeded("PDF image projection blocks".into()));
        }
        doc.paragraph(kind, text, span);
    }
    doc.diagnostics.push(Diagnostic {
        code: "LFE_IMAGE_EXPORT_EXPERIMENTAL".into(),
        message: "Raw text-show runs and image occurrences follow content-stream traversal, not visual reading order. Each text run is a separate paragraph; canonical paragraph joining, furniture removal, outlines and dehyphenation are not applied. JPEG/JP2 bytes are copied without codec validation/color conversion; basic 8-bit Gray/RGB rasters become PNG. Clipping, visibility, inline images, composite/vector figures and captions are not reconstructed. No OCR is invoked.".into(),
        source: None,
    });
    Ok(doc)
}
fn skipped(doc: &mut Document, span: &SourceSpan, message: String) {
    doc.diagnostics.push(Diagnostic { code: "PDF_IMAGE_SKIPPED".into(), message, source: Some(span.clone()) });
}
