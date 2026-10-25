#![cfg(feature = "pdf")]
use omniattach_documents::*;

fn pdf_with_outline() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>",
        "<< /Title (Chapter One) /Parent 4 0 R /Dest [3 0 R /Fit] >>",
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in &offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[test]
fn pdf_bookmarks_become_headings_with_page_numbers() {
    let doc = extract_bytes("a.pdf", &pdf_with_outline(), &Options::default()).unwrap();
    assert!(matches!(
        doc.blocks[0].kind,
        BlockKind::Heading { level: 1 }
    ));
    assert_eq!(doc.blocks[0].text, "Chapter One");
    assert_eq!(doc.blocks[0].source.page, Some(1));
}
