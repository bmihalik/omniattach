#![cfg(feature = "pdf")]
//! Opt-in integration checks against the independently generated LFE-4A corpus.
use omniattach_documents::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::PathBuf, sync::Arc};

struct Corpus {
    root: PathBuf,
    manifest: Value,
}
impl Corpus {
    fn load() -> Self {
        let root = PathBuf::from(std::env::var_os("OMNIATTACH_EQUATION_CORPUS")
            .expect("set OMNIATTACH_EQUATION_CORPUS to a validated LFE-4A directory"));
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["schema_version"], "omniattach/lfe-equation-fixtures/0.1");
        Self { root, manifest }
    }
    fn bytes(&self, name: &str) -> Vec<u8> {
        let entry = self.manifest["documents"].as_array().unwrap().iter()
            .find(|d| d["path"] == name).expect("missing corpus PDF");
        let bytes = std::fs::read(self.root.join(name)).unwrap();
        assert_eq!(entry["sha256"].as_str().unwrap(), format!("{:x}", Sha256::digest(&bytes)));
        bytes
    }
    fn occurrences(&self, name: &str) -> Vec<&Value> {
        self.manifest["occurrences"].as_array().unwrap().iter()
            .filter(|o| o["pdf"] == name).collect()
    }
}
fn no_ocr() -> Options {
    Options {
        pdf_ocr: Some(PdfOcr(Arc::new(|_| panic!("LFE inspection/projection invoked OCR")))),
        ..Default::default()
    }
}
fn image_blocks(doc: &Document) -> Vec<&Block> {
    doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Image { .. })).collect()
}
fn rgb_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgb);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    pixels.truncate(frame.buffer_size());
    (frame.width, frame.height, pixels)
}
fn asset_for<'a>(doc: &'a Document, block: &Block) -> &'a ExtractedAsset {
    let BlockKind::Image { asset_id } = &block.kind else { panic!("not an image") };
    doc.assets.iter().find(|a| &a.id == asset_id).expect("dangling image reference")
}
fn check_bounds(actual: PdfBounds, expected: &Value) {
    let coords = [actual.left, actual.bottom, actual.right, actual.top];
    for (got, want) in coords.iter().zip(expected.as_array().unwrap()) {
        assert!((got - want.as_f64().unwrap()).abs() < 0.01, "{got} != {want}");
    }
    assert!(!actual.estimated);
}

#[test]
#[ignore = "requires generated LFE-4A corpus; run the dedicated workflow or documented command"]
fn corpus_inspection_retains_source_pages_sequences_and_hint_references() {
    let c = Corpus::load();
    for entry in c.manifest["documents"].as_array().unwrap() {
        let name = entry["path"].as_str().unwrap();
        let bytes = c.bytes(name);
        let report = inspect_pdf_bytes(name, &bytes, &no_ocr()).unwrap();
        assert_eq!(report.page_count as u64, entry["pages"].as_u64().unwrap());
        assert_eq!(report.source.byte_length, bytes.len());
        assert_eq!(report.source.sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(report.source.name, name);
        for (i, element) in report.elements.iter().enumerate() {
            assert_eq!(element.sequence, i + 1);
            assert!(element.anchor.page > 0 && element.anchor.page as usize <= report.page_count);
            for source in &element.derived_from {
                assert!(*source > 0 && *source < element.sequence);
                assert_eq!(report.elements[*source-1].kind, PdfElementKind::Text);
            }
        }
        let plan = report.naming_plan().unwrap();
        assert_eq!(plan.elements().len(), report.elements.iter()
            .filter(|e| e.kind != PdfElementKind::Text).count());
        let names: std::collections::BTreeSet<_> = plan.elements().iter()
            .map(|e| e.stem().to_owned()).collect();
        assert_eq!(names.len(), plan.elements().len());
        let restored: PdfInspection = serde_json::from_str(&report.to_json().unwrap()).unwrap();
        assert_eq!(report, restored);
        eprintln!("{name}: {} text runs, {} image occurrences, {} equation hints (not recognition)",
            report.elements.iter().filter(|e| e.kind == PdfElementKind::Text).count(),
            report.elements.iter().filter(|e| e.kind == PdfElementKind::Image).count(),
            report.elements.iter().filter(|e| e.kind == PdfElementKind::EquationCandidate).count());
    }
}

#[test]
#[ignore = "requires generated LFE-4A corpus"]
fn raster_equations_match_independent_pixels_bounds_and_prose_order() {
    let c = Corpus::load();
    let bytes = c.bytes("raster.pdf");
    let report = inspect_pdf_bytes("raster.pdf", &bytes, &no_ocr()).unwrap();
    let images: Vec<_> = report.elements.iter().filter(|e| e.kind == PdfElementKind::Image).collect();
    let expected = c.occurrences("raster.pdf");
    assert_eq!(images.len(), expected.len());
    let doc = extract_pdf_with_images("raster.pdf", &bytes, &no_ocr()).unwrap();
    assert_eq!(doc.assets.len(), expected.len());
    let blocks = image_blocks(&doc);
    assert_eq!(blocks.len(), expected.len());
    assert!(!doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
    for ((image, occurrence), block) in images.iter().zip(expected).zip(blocks) {
        let page = occurrence["page"].as_u64().unwrap() as u32;
        assert_eq!(image.anchor.page, page);
        assert_eq!(block.source.page, Some(page));
        check_bounds(image.bounds.unwrap(), &occurrence["bbox_pdf"]);
        let asset = asset_for(&doc, block);
        let png = std::fs::read(c.root.join(occurrence["formula_image"].as_str().unwrap())).unwrap();
        assert_eq!(asset.media_type, "image/png");
        assert_eq!(rgb_png(&asset.bytes), rgb_png(&png));
        assert_eq!(asset.sha256, format!("{:x}", Sha256::digest(&asset.bytes)));
        let page_blocks: Vec<_> = doc.blocks.iter().filter(|b| b.source.page == Some(page)).collect();
        let image_pos = page_blocks.iter().position(|b| matches!(b.kind, BlockKind::Image { .. })).unwrap();
        assert!(page_blocks[..image_pos].iter().any(|b| b.text.contains("Synthetic prose before")));
        assert!(page_blocks[image_pos+1..].iter().any(|b| b.text.contains("Synthetic prose after")));
        assert_eq!(doc.to_markdown().matches(&format!("]({})", asset.filename)).count(), 1);
    }
}

#[test]
#[ignore = "requires generated LFE-4A corpus"]
fn reuse_and_transforms_keep_occurrences_distinct_without_rotating_asset_pixels() {
    let c = Corpus::load();
    for name in ["repeated.pdf", "transformed.pdf", "page-transform.pdf"] {
        let bytes = c.bytes(name);
        let report = inspect_pdf_bytes(name, &bytes, &no_ocr()).unwrap();
        let images: Vec<_> = report.elements.iter().filter(|e| e.kind == PdfElementKind::Image).collect();
        let expected = c.occurrences(name);
        assert_eq!(images.len(), expected.len());
        let doc = extract_pdf_with_images(name, &bytes, &no_ocr()).unwrap();
        let blocks = image_blocks(&doc);
        assert_eq!(blocks.len(), expected.len());
        assert_eq!(doc.assets.len(), 1);
        assert!(!doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
        for (image, occurrence) in images.iter().zip(&expected) {
            check_bounds(image.bounds.unwrap(), &occurrence["bbox_pdf"]);
            let png = std::fs::read(c.root.join(occurrence["formula_image"].as_str().unwrap())).unwrap();
            assert_eq!(rgb_png(&doc.assets[0].bytes), rgb_png(&png));
        }
        for block in &blocks {
            assert_eq!(asset_for(&doc, block).id, doc.assets[0].id);
        }
        assert_eq!(doc.to_markdown().matches(&format!("]({})", doc.assets[0].filename)).count(), expected.len());
        if name == "repeated.pdf" {
            assert_eq!(images[0].anchor.object_id, images[1].anchor.object_id);
            assert_ne!(images[0].anchor.operation_visit, images[1].anchor.operation_visit);
            assert_ne!(images[0].sequence, images[1].sequence);
        }
    }
}

#[test]
#[ignore = "requires generated LFE-4A corpus"]
fn scans_are_page_images_and_outlines_are_not_fabricated_formula_assets() {
    let c = Corpus::load();
    for name in ["scanned.pdf", "outlines.pdf"] {
        let bytes = c.bytes(name);
        let report = inspect_pdf_bytes(name, &bytes, &no_ocr()).unwrap();
        let doc = extract_pdf_with_images(name, &bytes, &no_ocr()).unwrap();
        assert!(!report.elements.iter().any(|e| e.kind == PdfElementKind::EquationCandidate));
        if name == "scanned.pdf" {
            assert_eq!(image_blocks(&doc).len(), report.page_count);
            assert_eq!(doc.assets.len(), report.page_count);
            assert!(report.elements.iter().all(|e| e.kind == PdfElementKind::Image));
        } else {
            assert!(doc.assets.is_empty());
            assert!(image_blocks(&doc).is_empty());
        }
    }
}

#[test]
#[ignore = "requires generated LFE-4A corpus"]
fn ordinary_extraction_stays_independent_and_corpus_pdf_bytes_are_preserved() {
    let c = Corpus::load();
    for name in ["raster.pdf", "repeated.pdf", "negatives.pdf"] {
        let bytes = c.bytes(name);
        let options = Options::default();
        let before = extract_as(name, &bytes, Format::Pdf, &options).unwrap();
        inspect_pdf_bytes(name, &bytes, &no_ocr()).unwrap();
        extract_pdf_with_images(name, &bytes, &no_ocr()).unwrap();
        let after = extract_as(name, &bytes, Format::Pdf, &options).unwrap();
        assert_eq!(before, after);
        assert!(before.assets.is_empty());
        assert!(image_blocks(&before).is_empty());
    }
    // Verify corpus PDF bytes remain unchanged; this is not a general filesystem audit.
    for entry in c.manifest["documents"].as_array().unwrap() {
        c.bytes(entry["path"].as_str().unwrap());
    }
}
