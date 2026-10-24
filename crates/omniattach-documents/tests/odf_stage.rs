#![cfg(feature = "office")]
use omniattach_documents::*;
use std::io::{Cursor, Write};

const ODF: &str = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body>BODY</office:body></office:document-content>"#;

fn package(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn read(name: &str, entries: &[(&str, &str)]) -> Document {
    extract_bytes(name, &package(entries), &Options::default()).unwrap()
}

#[test]
fn odt_nested_list_items_link_to_parent_item() {
    let input = ODF.replace(
        "BODY",
        r#"<office:text><text:list><text:list-item><text:p>A</text:p><text:list><text:list-item><text:p>B</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>C</text:p></text:list-item></text:list></office:text>"#,
    );
    let doc = read("a.odt", &[("content.xml", &input)]);
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(doc.blocks[0].parent_id, None);
    assert_eq!(
        doc.blocks[1].parent_id.as_deref(),
        Some(doc.blocks[0].id.as_str())
    );
    assert_eq!(doc.blocks[2].parent_id, None);
    assert_eq!(doc.blocks[1].source.path.as_deref(), Some("list-level:2"));
}
