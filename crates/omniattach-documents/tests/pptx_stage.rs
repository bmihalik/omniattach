#![cfg(feature = "office")]
use omniattach_documents::*;
use std::io::{Cursor, Write};

const PRES_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAW_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn package(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn pptx_shapes_read_in_slide_order_not_position() {
    let presentation = format!(
        r#"<p:presentation xmlns:p="{PRES_NS}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="1" r:id="rA"/></p:sldIdLst></p:presentation>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rA" Target="slides/slide1.xml"/></Relationships>"#;
    let slide = format!(
        r#"<p:sld xmlns:p="{PRES_NS}" xmlns:a="{DRAW_NS}"><p:cSld><p:spTree><p:sp><p:spPr><a:xfrm><a:off x="0" y="5000"/></a:xfrm></p:spPr><p:txBody><a:p><a:r><a:t>Lower</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:spPr><a:xfrm><a:off x="0" y="100"/></a:xfrm></p:spPr><p:txBody><a:p><a:r><a:t>Upper</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
    );
    let bytes = package(&[
        ("ppt/presentation.xml", &presentation),
        ("ppt/_rels/presentation.xml.rels", rels),
        ("ppt/slides/slide1.xml", &slide),
    ]);
    let doc = extract_bytes("a.pptx", &bytes, &Options::default()).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].text, "Lower");
    assert_eq!(doc.blocks[1].text, "Upper");
    assert_eq!(doc.blocks[0].source.slide, Some(1));
}
