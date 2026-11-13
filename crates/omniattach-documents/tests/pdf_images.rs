#![cfg(feature = "pdf")]
use lopdf::{
    Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use omniattach_documents::*;
use sha2::{Digest, Sha256};

fn jpeg() -> Vec<u8> {
    let hex = include_str!("fixtures/lfe-rgb.jpg.hex").trim();
    assert!(hex.len().is_multiple_of(2));
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture(filter: &str, special: Option<&str>, form: bool, page_count: usize) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let font = pdf.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let mut image_dict = dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => filter};
    if let Some(key) = special {
        image_dict.set(key, Object::Null);
    }
    let image = pdf.add_object(Stream::new(image_dict, jpeg()));
    let target = if form {
        pdf.add_object(Stream::new(dictionary! {"Type" => "XObject", "Subtype" => "Form", "BBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(2), Object::Integer(1)], "Resources" => dictionary! {"XObject" => dictionary! {"Im" => image}}}, b"/Im Do".to_vec()))
    } else {
        image
    };
    let resources = pdf.add_object(dictionary! {"Font" => dictionary! {"F1" => font}, "XObject" => dictionary! {"Target" => target}});
    let show = |s: &str| Operation::new("Tj", vec![Object::string_literal(s)]);
    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        show("be"),
        show("fore"),
        Operation::new("ET", vec![]),
        Operation::new("Do", vec![Object::Name(b"Target".to_vec())]),
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        show("after"),
        Operation::new("ET", vec![]),
        Operation::new("Do", vec![Object::Name(b"Target".to_vec())]),
    ];
    let data = Content { operations }.encode().unwrap();
    let mut kids: Vec<Object> = Vec::new();
    for _ in 0..page_count {
        let content = pdf.add_object(Stream::new(dictionary! {}, data.clone()));
        let page = pdf
            .add_object(dictionary! {"Type" => "Page", "Parent" => pages, "Contents" => content});
        kids.push(Object::Reference(page));
    }
    pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => page_count as i64, "Resources" => resources, "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)]}));
    let root = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    pdf.trailer.set("Root", root);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}
#[test]
fn real_jpeg_bytes_hash_and_markdown_references_are_preserved() {
    let doc = extract_pdf_with_images(
        "book.pdf",
        &fixture("DCTDecode", None, false, 1),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(doc.assets.len(), 1);
    assert_eq!(doc.assets[0].bytes, jpeg());
    assert_eq!(
        doc.assets[0].sha256,
        "fda4cc9e77660582cae5623b185dcc9ace4b860ac5bb00f1c573e3bd1bc3ad7f"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(jpeg())),
        doc.assets[0].sha256
    );
    assert_eq!(
        doc.assets[0].filename,
        format!("images/book-{}-fe-000001.jpg", &doc.source.sha256[..8])
    );
    assert_eq!(
        doc.to_markdown()
            .matches(&format!("]({})", doc.assets[0].filename))
            .count(),
        2
    );
    assert_eq!(doc.schema_version, "omniattach-documents/0.2");
}
#[test]
fn fragmented_text_joins_without_crossing_image_occurrences() {
    let doc = extract_pdf_with_images(
        "book.pdf",
        &fixture("DCTDecode", None, false, 1),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(doc.blocks.len(), 4);
    assert_eq!(doc.blocks[0].text, "before");
    assert!(matches!(doc.blocks[1].kind, BlockKind::Image { .. }));
    assert_eq!(doc.blocks[2].text, "after");
    assert!(matches!(doc.blocks[3].kind, BlockKind::Image { .. }));
    assert!(
        doc.blocks[0]
            .source
            .path
            .as_deref()
            .unwrap()
            .contains("content-operations")
    );
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "LFE_IMAGE_EXPORT_EXPERIMENTAL")
    );
}
#[test]
fn nested_form_image_is_emitted_with_form_provenance() {
    let doc = extract_pdf_with_images(
        "form.pdf",
        &fixture("DCTDecode", None, true, 1),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(doc.assets.len(), 1);
    assert_eq!(
        doc.blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Image { .. }))
            .count(),
        2
    );
    assert!(
        doc.blocks[1]
            .source
            .path
            .as_deref()
            .unwrap()
            .contains("forms [(")
    );
}
#[test]
fn reuse_across_pages_shares_asset_and_keeps_occurrence_pages() {
    let doc = extract_pdf_with_images(
        "pages.pdf",
        &fixture("DCTDecode", None, false, 2),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(doc.assets.len(), 1);
    let images: Vec<_> = doc
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Image { .. }))
        .collect();
    assert_eq!(
        images
            .iter()
            .map(|b| b.source.page.unwrap())
            .collect::<Vec<_>>(),
        vec![1, 1, 2, 2]
    );
    for block in images {
        assert!(
            matches!(&block.kind, BlockKind::Image { asset_id } if asset_id == &doc.assets[0].id)
        );
        assert!(block.keep_together);
    }
}
#[test]
fn unsupported_interpretation_is_diagnosed_without_links_or_cross_image_joining() {
    for special in ["Mask", "SMask", "Decode", "DecodeParms", "ImageMask"] {
        let doc = extract_pdf_with_images(
            "masked.pdf",
            &fixture("DCTDecode", Some(special), false, 1),
            &Options::default(),
        )
        .unwrap();
        assert!(doc.assets.is_empty(), "{special}");
        assert!(!doc.to_markdown().contains("!["));
        assert_eq!(
            doc.blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>(),
            vec!["before", "after"]
        );
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == "PDF_IMAGE_SKIPPED")
        );
    }
    let doc = extract_pdf_with_images(
        "raw.pdf",
        &fixture("FlateDecode", None, false, 1),
        &Options::default(),
    )
    .unwrap();
    assert!(doc.assets.is_empty());
    assert_eq!(
        doc.blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>(),
        vec!["before", "after"]
    );
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_IMAGE_SKIPPED")
    );
}
#[test]
fn asset_count_byte_and_input_limits_are_enforced() {
    let bytes = fixture("DCTDecode", None, false, 1);
    let mut options = Options::default();
    options.limits.archive_entries = 0;
    assert!(
        matches!(extract_pdf_with_images("count.pdf", &bytes, &options), Err(Error::LimitExceeded(message)) if message.contains("image"))
    );
    let mut options = Options::default();
    options.limits.archive_bytes = (jpeg().len() - 1) as u64;
    assert!(
        matches!(extract_pdf_with_images("bytes.pdf", &bytes, &options), Err(Error::LimitExceeded(message)) if message.contains("image"))
    );
    let mut options = Options::default();
    options.limits.input_bytes = bytes.len() - 1;
    assert!(matches!(
        extract_pdf_with_images("input.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
fn ordinary_extraction_remains_separate_and_ocr_is_not_called() {
    let bytes = fixture("DCTDecode", None, false, 1);
    let normal = extract_as("book.pdf", &bytes, Format::Pdf, &Options::default()).unwrap();
    assert_eq!(normal.schema_version, "omniattach-documents/0.1");
    assert_eq!(normal.to_text(), "beforeafter");
    assert!(
        !normal
            .blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Image { .. }))
    );
    let options = Options {
        pdf_ocr: Some(PdfOcr(std::sync::Arc::new(|_| panic!("OCR must not run")))),
        ..Default::default()
    };
    assert!(extract_pdf_with_images("book.pdf", &bytes, &options).is_ok());
}
