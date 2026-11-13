//! Opt-in LFE image export prototype; canonical extraction is unchanged.
#[cfg(not(feature = "pdf"))]
fn main() {
    eprintln!("pdf-images requires --features pdf");
    std::process::exit(2);
}

#[cfg(feature = "pdf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::Read, path::PathBuf};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: pdf-images INPUT.pdf NEW_EXPORT_DIRECTORY".into());
    }
    let input = PathBuf::from(&args[0]);
    let options = omniattach_documents::Options::default();
    let file = std::fs::File::open(&input)?;
    if !file.metadata()?.is_file() {
        return Err("input is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(options.limits.input_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    let doc = native::project(&input.to_string_lossy(), &bytes, &options)?;
    native::write_new(&doc, &PathBuf::from(&args[1]))?;
    for diagnostic in &doc.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    Ok(())
}

#[cfg(feature = "pdf")]
mod native {
    use omniattach_documents::{Block, BlockKind, Diagnostic, Document, Error, ExtractedAsset, Options, PdfElementKind, Result, SourceSpan};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, fs, io::Write, path::Path};

    pub(super) fn project(name: &str, bytes: &[u8], options: &Options) -> Result<Document> {
        let inspection = omniattach_documents::inspect_pdf_bytes(name, bytes, options)?;
        let plan = inspection.naming_plan()?;
        let pdf = lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions {
            max_decompressed_size: Some(options.limits.archive_bytes.min(usize::MAX as u64) as usize),
            ..Default::default()
        }).map_err(|e| Error::Parse(format!("PDF image export: {e}")))?;
        if pdf.is_encrypted() {
            return Err(Error::InvalidInput("encrypted PDF is unsupported".into()));
        }
        let mut doc = Document {
            schema_version: "omniattach-documents/0.2".into(),
            source: inspection.source.clone(),
            title: None,
            format: "pdf".into(),
            extractor: format!("omniattach-documents/{}/pdf-images-example", env!("CARGO_PKG_VERSION")),
            blocks: Vec::new(), tables: Vec::new(), assets: Vec::new(),
            diagnostics: inspection.diagnostics.clone(),
        };
        let names: BTreeMap<_, _> = plan.elements().iter()
            .map(|n| (n.element_sequence(), n)).collect();
        let mut asset_ids: BTreeMap<lopdf::ObjectId, String> = BTreeMap::new();
        let mut asset_bytes = 0u64;
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
                        // Reject unsupported interpretation rather than silently dropping transparency.
                        if [b"Mask".as_slice(), b"SMask".as_slice(), b"Decode".as_slice()].iter()
                            .any(|key| stream.dict.get(key).is_ok())
                            || stream.dict.get(b"ImageMask").is_ok_and(|v| !matches!(v, lopdf::Object::Boolean(false))) {
                            skipped(&mut doc, &span, "image masks or custom Decode are unsupported".into());
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
                                skipped(&mut doc, &span, "only a single DCTDecode or JPXDecode filter is supported".into());
                                continue;
                            }
                        };
                        let next_bytes = asset_bytes.checked_add(stream.content.len() as u64)
                            .ok_or_else(|| Error::LimitExceeded("PDF image byte overflow".into()))?;
                        if next_bytes > options.limits.archive_bytes || doc.assets.len() >= options.limits.archive_entries {
                            return Err(Error::LimitExceeded("PDF image bytes or count".into()));
                        }
                        let named = names.get(&element.sequence).ok_or_else(|| Error::InvalidInput("missing image naming entry".into()))?;
                        let asset_id = format!("{}:a{}", doc.source.sha256, doc.assets.len() + 1);
                        doc.assets.push(ExtractedAsset {
                            id: asset_id.clone(), filename: named.image_path(ext)?, media_type: media.into(),
                            sha256: format!("{:x}", Sha256::digest(&stream.content)),
                            byte_length: stream.content.len(), bytes: stream.content.clone(), source: span.clone(),
                        });
                        asset_bytes = next_bytes;
                        asset_ids.insert(id, asset_id.clone());
                        asset_id
                    };
                    (BlockKind::Image { asset_id }, format!("Image occurrence {}", element.sequence))
                }
                // Equation/table hints overlap source runs; never duplicate their text.
                _ => continue,
            };
            if doc.blocks.len() >= options.limits.blocks {
                return Err(Error::LimitExceeded("PDF image projection blocks".into()));
            }
            let keep_together = matches!(kind, BlockKind::Image { .. });
            doc.blocks.push(Block {
                id: format!("{}:b{}", doc.source.sha256, doc.blocks.len() + 1),
                parent_id: None, kind, text, section_path: Vec::new(), source: span, keep_together,
            });
        }
        doc.diagnostics.push(Diagnostic {
            code: "LFE_IMAGE_EXPORT_EXPERIMENTAL".into(),
            message: "Raw text-show runs and image occurrences follow content-stream traversal, not visual reading order. Each text run is a separate paragraph; canonical paragraph joining, furniture removal, outlines and dehyphenation are not applied. JPEG/JP2 bytes are copied without codec validation or color conversion; clipping, visibility, inline images, vector/composite figures and captions are not reconstructed. No OCR is invoked.".into(),
            source: None,
        });
        Ok(doc)
    }

    fn skipped(doc: &mut Document, span: &SourceSpan, message: String) {
        doc.diagnostics.push(Diagnostic { code: "PDF_IMAGE_SKIPPED".into(), message, source: Some(span.clone()) });
    }

    pub(super) fn write_new(doc: &Document, destination: &Path) -> Result<()> {
        // Validate every asset before creating anything; this writer accepts only flat generated image names.
        let mut names = std::collections::BTreeSet::new();
        for asset in &doc.assets {
            let Some(leaf) = asset.filename.strip_prefix("images/") else {
                return Err(Error::InvalidInput("image asset must be below images/".into()));
            };
            if leaf.is_empty() || leaf.starts_with('.') || !leaf.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                || !names.insert(asset.filename.clone()) {
                return Err(Error::InvalidInput("unsafe or duplicate image filename".into()));
            }
        }
        let markdown = doc.to_markdown();
        let json = doc.to_json()?;
        fs::create_dir(destination)?;
        // Never recursively remove a directory on error; leave partial output for inspection.
        fs::create_dir(destination.join("images"))?;
        for asset in &doc.assets {
            fs::OpenOptions::new().write(true).create_new(true)
                .open(destination.join(&asset.filename))?.write_all(&asset.bytes)?;
        }
        for (name, bytes) in [("document.md", markdown.as_bytes()), ("document.json", json.as_bytes())] {
            fs::OpenOptions::new().write(true).create_new(true)
                .open(destination.join(name))?.write_all(bytes)?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use lopdf::{content::{Content, Operation}, dictionary, Object, Stream};
        fn fixture(filter: &str) -> Vec<u8> {
            let mut pdf = lopdf::Document::with_version("1.5");
            let pages = pdf.new_object_id();
            let font = pdf.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
            // Structural payload fixture, not a decodable JPEG. The exporter intentionally does not decode it.
            let image = pdf.add_object(Stream::new(dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => filter}, vec![255, 216, 255, 217]));
            let resources = pdf.add_object(dictionary! {"Font" => dictionary! {"F1" => font}, "XObject" => dictionary! {"Im" => image}});
            let show = |s: &str| Operation::new("Tj", vec![Object::string_literal(s)]);
            let operations = vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
                show("before"), Operation::new("ET", vec![]),
                Operation::new("Do", vec![Object::Name(b"Im".to_vec())]),
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
                show("after"), Operation::new("ET", vec![]),
                Operation::new("Do", vec![Object::Name(b"Im".to_vec())]),
            ];
            let content = pdf.add_object(Stream::new(dictionary! {}, Content { operations }.encode().unwrap()));
            let page = pdf.add_object(dictionary! {"Type" => "Page", "Parent" => pages, "Contents" => content});
            pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1, "Resources" => resources, "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)]}));
            let root = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
            pdf.trailer.set("Root", root);
            let mut bytes = Vec::new();
            pdf.save_to(&mut bytes).unwrap();
            bytes
        }
        #[test]
        fn occurrences_share_an_asset_and_keep_traversal_order() {
            let doc = project("book.pdf", &fixture("DCTDecode"), &Options::default()).unwrap();
            assert_eq!(doc.assets.len(), 1);
            assert_eq!(doc.blocks.len(), 4);
            assert_eq!(doc.blocks[0].text, "before");
            assert_eq!(doc.blocks[2].text, "after");
            for index in [1, 3] {
                assert!(matches!(&doc.blocks[index].kind, BlockKind::Image { asset_id } if asset_id == &doc.assets[0].id));
            }
            let md = doc.to_markdown();
            assert_eq!(md.matches(&format!("]({})", doc.assets[0].filename)).count(), 2);
        }
        #[test]
        fn unsupported_filter_has_no_dangling_links() {
            let doc = project("book.pdf", &fixture("FlateDecode"), &Options::default()).unwrap();
            assert!(doc.assets.is_empty());
            assert!(!doc.to_markdown().contains("!["));
            assert!(doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
        }
        #[test]
        fn export_writes_referenced_bytes_and_refuses_overwrite() {
            let doc = project("book.pdf", &fixture("DCTDecode"), &Options::default()).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("export");
            write_new(&doc, &output).unwrap();
            assert_eq!(fs::read(output.join(&doc.assets[0].filename)).unwrap(), doc.assets[0].bytes);
            assert_eq!(fs::read_to_string(output.join("document.md")).unwrap(), doc.to_markdown());
            assert!(write_new(&doc, &output).is_err());
        }
        #[test]
        fn unsafe_asset_is_rejected_before_creating_directory() {
            let mut doc = project("book.pdf", &fixture("DCTDecode"), &Options::default()).unwrap();
            doc.assets[0].filename = "images/../../escape.jpg".into();
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("bad");
            assert!(write_new(&doc, &output).is_err());
            assert!(!output.exists());
        }
    }
}
