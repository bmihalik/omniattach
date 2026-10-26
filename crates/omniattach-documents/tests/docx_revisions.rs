#![cfg(feature = "office")]
use omniattach_documents::*;
use std::io::{Cursor, Write};

const DOC_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx_body(body: &str) -> Vec<u8> {
    let xml = format!(r#"<w:document xmlns:w="{DOC_NS}"><w:body>{body}</w:body></w:document>"#);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn docx_moved_and_tracked_text_follows_accepted_changes() {
    let body = r#"<w:p><w:r><w:t>Kept </w:t></w:r><w:del w:id="1"><w:r><w:delText>Removed</w:delText></w:r></w:del><w:ins w:id="2"><w:r><w:t>Inserted</w:t></w:r></w:ins></w:p><w:moveFrom w:id="3"><w:p><w:r><w:t>Moved away</w:t></w:r></w:p></w:moveFrom><w:moveTo w:id="4"><w:p><w:r><w:t>Moved here</w:t></w:r></w:p></w:moveTo>"#;
    let doc = extract_bytes("a.docx", &docx_body(body), &Options::default()).unwrap();
    let texts: Vec<&str> = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("Inserted")));
    assert!(texts.iter().any(|t| t.contains("Moved here")));
    assert!(!texts.iter().any(|t| t.contains("Removed")));
    assert!(!texts.iter().any(|t| t.contains("Moved away")));
}
