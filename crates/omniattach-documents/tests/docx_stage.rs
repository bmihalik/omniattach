#![cfg(feature = "office")]
use omniattach_documents::*;
use std::io::{Cursor, Write};

const DOC_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

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

fn document(body: &str) -> String {
    format!(r#"<w:document xmlns:w="{DOC_NS}"><w:body>{body}</w:body></w:document>"#)
}

#[test]
fn docx_heading_inherits_level_through_based_on() {
    let styles = format!(
        r#"<w:styles xmlns:w="{DOC_NS}"><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/></w:style><w:style w:type="paragraph" w:styleId="Custom"><w:basedOn w:val="Heading2"/></w:style></w:styles>"#
    );
    let body = document(
        r#"<w:p><w:pPr><w:pStyle w:val="Custom"/></w:pPr><w:r><w:t>Inherited</w:t></w:r></w:p>"#,
    );
    let doc = read(
        "a.docx",
        &[("word/document.xml", &body), ("word/styles.xml", &styles)],
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading { level: 2 }));
    assert_eq!(doc.blocks[0].text, "Inherited");
}

#[test]
fn docx_list_items_keep_level_in_source_path() {
    let body = document(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="3"/></w:numPr></w:pPr><w:r><w:t>Nested item</w:t></w:r></w:p>"#,
    );
    let doc = read("a.docx", &[("word/document.xml", &body)]);
    assert!(matches!(
        doc.blocks[0].kind,
        BlockKind::ListItem { ordered: false }
    ));
    assert_eq!(doc.blocks[0].source.path.as_deref(), Some("list-level:1"));
}

#[test]
fn docx_footnotes_headers_and_footers_follow_body() {
    let body = document(r#"<w:p><w:r><w:t>Body</w:t></w:r></w:p>"#);
    let notes = format!(
        r#"<w:footnotes xmlns:w="{DOC_NS}"><w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:t>sep</w:t></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:t>Note text</w:t></w:r></w:p></w:footnote></w:footnotes>"#
    );
    let header = format!(
        r#"<w:hdr xmlns:w="{DOC_NS}"><w:p><w:r><w:t>Header text</w:t></w:r></w:p></w:hdr>"#
    );
    let footer = format!(
        r#"<w:ftr xmlns:w="{DOC_NS}"><w:p><w:r><w:t>Footer text</w:t></w:r></w:p></w:ftr>"#
    );
    let doc = read(
        "a.docx",
        &[
            ("word/document.xml", &body),
            ("word/footnotes.xml", &notes),
            ("word/header1.xml", &header),
            ("word/footer1.xml", &footer),
        ],
    );
    assert_eq!(doc.blocks[0].text, "Body");
    assert_eq!(doc.blocks[0].source.path, None);
    let rest: Vec<(&str, Option<&str>)> = doc.blocks[1..]
        .iter()
        .map(|b| (b.text.as_str(), b.source.path.as_deref()))
        .collect();
    assert_eq!(rest.len(), 3);
    assert!(rest.contains(&("Note text", Some("footnote:1"))));
    assert!(rest.contains(&("Header text", Some("header"))));
    assert!(rest.contains(&("Footer text", Some("footer"))));
}

#[test]
fn docx_numbered_paragraphs_use_numbering_format() {
    let numbering = format!(
        r#"<w:numbering xmlns:w="{DOC_NS}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let body = document(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>Sub</w:t></w:r></w:p>"#,
    );
    let doc = read(
        "a.docx",
        &[("word/document.xml", &body), ("word/numbering.xml", &numbering)],
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::ListItem { ordered: true }));
    assert!(matches!(doc.blocks[1].kind, BlockKind::ListItem { ordered: false }));
    assert_eq!(doc.blocks[1].source.path.as_deref(), Some("list-level:1"));
}

#[test]
fn docx_list_markers_count_and_restart() {
    let numbering = format!(
        r#"<w:numbering xmlns:w="{DOC_NS}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2)"/></w:lvl></w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    );
    let item = |ilvl: u8, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = document(&format!(
        "{}{}{}{}",
        item(0, "First"),
        item(0, "Second"),
        item(1, "Sub"),
        item(0, "Third")
    ));
    let doc = read(
        "a.docx",
        &[("word/document.xml", &body), ("word/numbering.xml", &numbering)],
    );
    let paths: Vec<Option<&str>> = doc
        .blocks
        .iter()
        .map(|b| b.source.path.as_deref())
        .collect();
    assert_eq!(
        paths,
        vec![
            Some("list-level:0;marker:1."),
            Some("list-level:0;marker:2."),
            Some("list-level:1;marker:a)"),
            Some("list-level:0;marker:3."),
        ]
    );
}
