#![cfg(feature = "pdf")]
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use omniattach_documents::{Options, extract_bytes};

fn fixture(items: Vec<Object>) -> Vec<u8> {
    let mut pdf = Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let font = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"
    });
    let resources = pdf.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 11.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("TJ", vec![Object::Array(items)]),
            Operation::new("ET", vec![]),
        ],
    };
    let stream = pdf.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page =
        pdf.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "Contents" => stream });
    pdf.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
            "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()]
        }),
    );
    let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}

fn words(gap: Object) -> Vec<Object> {
    let mut items = Vec::new();
    for word in ["William", "Blanchard", "who", "said", "he", "possessed"] {
        if !items.is_empty() {
            items.push(gap.clone());
        }
        items.push(Object::string_literal(word));
    }
    items
}

#[test]
fn real_word_gaps_survive_in_text_and_markdown() {
    let bytes = fixture(words(Object::Real(-249.102)));
    let doc = extract_bytes("spacing.pdf", &bytes, &Options::default()).unwrap();
    let expected = "William Blanchard who said he possessed";
    assert!(doc.to_text().contains(expected), "{}", doc.to_text());
    assert!(
        doc.to_markdown().contains(expected),
        "{}",
        doc.to_markdown()
    );
    assert_eq!(doc.source.byte_length, bytes.len());
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_TJ_SPACING_NORMALIZED")
    );
}

#[test]
fn integer_and_real_word_gaps_extract_equally() {
    let options = Options::default();
    let a = extract_bytes("a.pdf", &fixture(words(Object::Integer(-249))), &options).unwrap();
    let b = extract_bytes("b.pdf", &fixture(words(Object::Real(-249.102))), &options).unwrap();
    assert_eq!(a.to_text(), b.to_text());
}

#[test]
fn small_kerning_and_positive_adjustments_do_not_split_words() {
    let bytes = fixture(vec![
        Object::string_literal("W"),
        Object::Real(40.861),
        Object::string_literal("ill"),
        Object::Real(-20.5),
        Object::string_literal("iam"),
    ]);
    let doc = extract_bytes("kern.pdf", &bytes, &Options::default()).unwrap();
    assert!(doc.to_text().contains("William"), "{}", doc.to_text());
    assert!(
        !doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_TJ_SPACING_NORMALIZED")
    );
}

#[test]
fn explicit_spaces_are_preserved_without_normalization() {
    let bytes = fixture(vec![Object::string_literal(
        "William Blanchard who said he possessed",
    )]);
    let doc = extract_bytes("literal.pdf", &bytes, &Options::default()).unwrap();
    assert!(
        doc.to_text()
            .contains("William Blanchard who said he possessed")
    );
    assert!(
        !doc.diagnostics
            .iter()
            .any(|d| d.code == "PDF_TJ_SPACING_NORMALIZED")
    );
}

#[test]
#[ignore = "requires an explicitly supplied local regression PDF"]
fn supplied_timeline_pdf_recovers_the_reported_phrase() {
    let path = std::env::var("OMNIATTACH_SPACING_PDF").expect("set OMNIATTACH_SPACING_PDF");
    let doc = omniattach_documents::extract(path, &Options::default()).unwrap();
    assert!(
        doc.to_text()
            .contains("William Blanchard who said he possessed")
    );
    assert!(
        doc.to_markdown()
            .contains("William Blanchard who said he possessed")
    );
}
