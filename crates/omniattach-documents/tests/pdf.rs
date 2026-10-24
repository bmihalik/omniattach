#![cfg(feature = "pdf")]
use lopdf::{
    Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use omniattach_documents::*;
fn pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![30.into(), 50.into()]),
            Operation::new("Tj", vec![Object::string_literal("Hello PDF")]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(
        dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content_id },
    );
    let empty_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into(), empty_id.into()], "Count" => 2, "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()] }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}
#[test]
fn pdf_text_page_provenance_and_scan_diagnostic() {
    let doc = extract_bytes("a.pdf", &pdf(), &Options::default()).unwrap();
    assert!(doc.to_text().contains("Hello PDF"));
    assert_eq!(doc.blocks[0].source.page, Some(1));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PAGE_WITHOUT_TEXT" && d.source.as_ref().unwrap().page == Some(2))
    );
}
#[test]
fn invalid_pdf_is_error_not_silent_empty_success() {
    assert!(extract_bytes("a.pdf", b"not PDF", &Options::default()).is_err());
}
