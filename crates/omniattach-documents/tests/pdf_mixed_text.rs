#![cfg(feature = "pdf")]
use lopdf::{Object, Stream, dictionary};
use omniattach_documents::*;

fn fixture(content: &str, supported_image: bool) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let font = pdf.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let mut image_dict = dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8};
    if !supported_image { image_dict.set("Filter", "LZWDecode"); }
    let image = pdf.add_object(Stream::new(image_dict, vec![255, 0, 0]));
    let resources = pdf.add_object(dictionary! {"Font" => dictionary! {"F1" => font}, "XObject" => dictionary! {"Im" => image}});
    let stream = pdf.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = pdf.add_object(dictionary! {"Type" => "Page", "Parent" => pages, "Contents" => stream, "Resources" => resources, "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)]});
    pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1}));
    let root = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    pdf.trailer.set("Root", root);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}
fn projected(content: &str) -> Document {
    extract_pdf_with_images("mixed.pdf", &fixture(content, true), &Options::default()).unwrap()
}
fn paragraphs(doc: &Document) -> Vec<&str> {
    doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Paragraph)).map(|b| b.text.as_str()).collect()
}
#[test]
fn actual_tj_fragments_and_integer_real_tj_gaps_keep_words() {
    for gap in ["-250", "-250.5"] {
        let doc = projected(&format!("BT /F1 12 Tf 1 0 0 1 72 700 Tm (be) Tj (fore) Tj [(word) {gap} (gap)] TJ ET"));
        assert_eq!(paragraphs(&doc), vec!["beforeword gap"]);
    }
}
#[test]
fn small_tj_kerning_does_not_invent_a_word_gap() {
    let doc = projected("BT /F1 12 Tf 1 0 0 1 72 700 Tm [(ker) 40 (ning)] TJ ET");
    assert_eq!(paragraphs(&doc), vec!["kerning"]);
}
#[test]
fn actual_text_movement_distinguishes_lines_and_paragraphs() {
    let bytes = fixture("BT /F1 12 Tf 1 0 0 1 72 700 Tm (first) Tj 0 -14 Td (second) Tj 0 -40 Td (third) Tj ET", true);
    let mixed = extract_pdf_with_images("lines.pdf", &bytes, &Options::default()).unwrap();
    let ordinary = extract_as("lines.pdf", &bytes, Format::Pdf, &Options::default()).unwrap();
    assert_eq!(paragraphs(&mixed), vec!["first\nsecond", "third"]);
    assert_eq!(paragraphs(&mixed), paragraphs(&ordinary));
    assert!(!ordinary.blocks.iter().any(|b| matches!(b.kind, BlockKind::Image { .. })));
}
#[test]
fn leading_and_next_line_operator_are_interpreted_before_assembly() {
    let doc = projected("BT /F1 12 Tf 14 TL 1 0 0 1 72 700 Tm (first) Tj T* (second) Tj ET");
    assert_eq!(paragraphs(&doc), vec!["first\nsecond"]);
}
#[test]
fn geometric_horizontal_gap_becomes_a_space() {
    let doc = projected("BT /F1 12 Tf 1 0 0 1 72 700 Tm (left) Tj 1 0 0 1 120 700 Tm (right) Tj ET");
    assert_eq!(paragraphs(&doc), vec!["left right"]);
}
#[test]
fn supported_and_skipped_images_both_split_surrounding_text() {
    let content = "BT /F1 12 Tf 1 0 0 1 72 700 Tm (be) Tj (fore) Tj ET /Im Do BT /F1 12 Tf 1 0 0 1 72 686 Tm (af) Tj (ter) Tj ET";
    for supported in [true, false] {
        let doc = extract_pdf_with_images("boundary.pdf", &fixture(content, supported), &Options::default()).unwrap();
        assert_eq!(paragraphs(&doc), vec!["before", "after"]);
        if supported {
            assert_eq!(doc.blocks.len(), 3);
            assert!(matches!(doc.blocks[1].kind, BlockKind::Image { .. }));
            assert_eq!(doc.assets[0].media_type, "image/png");
            assert!(doc.to_markdown().contains(&format!("]({})", doc.assets[0].filename)));
        } else {
            assert_eq!(doc.blocks.len(), 2);
            assert!(doc.assets.is_empty());
            assert!(doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
        }
    }
}
