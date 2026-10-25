#![cfg(feature = "pdf")]
use omniattach_documents::*;
use std::sync::Arc;

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

fn two_page_blank_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
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
fn pdf_pages_without_text_use_ocr_when_supplied() {
    let options = Options {
        pdf_ocr: Some(PdfOcr(Arc::new(|page: u32| -> Result<String> {
            Ok(format!("Scanned page {page}"))
        }))),
        ..Default::default()
    };
    let doc = extract_bytes("a.pdf", &pdf_with_outline(), &options).unwrap();
    let last = doc.blocks.last().unwrap();
    assert_eq!(last.text, "Scanned page 1");
    assert_eq!(last.source.path.as_deref(), Some("ocr"));
    assert_eq!(last.source.page, Some(1));
}

#[test]
fn pdf_pages_without_text_are_reported_without_ocr() {
    let doc = extract_bytes("a.pdf", &pdf_with_outline(), &Options::default()).unwrap();
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "PAGE_WITHOUT_TEXT")
    );
}

#[test]
fn pdf_page_ocr_failure_records_diagnostic_and_continues_remaining_pages() {
    let options = Options {
        pdf_ocr: Some(PdfOcr(Arc::new(|page: u32| -> Result<String> {
            if page == 1 {
                Err(Error::Parse("tesseract failed on page 1".into()))
            } else {
                Ok(format!("Recovered page {page}"))
            }
        }))),
        ..Default::default()
    };
    let doc = extract_bytes("two.pdf", &two_page_blank_pdf(), &options).unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].text, "Recovered page 2");
    assert_eq!(doc.blocks[0].source.page, Some(2));
    assert_eq!(doc.blocks[0].source.path.as_deref(), Some("ocr"));
    let diag = doc
        .diagnostics
        .iter()
        .find(|d| d.code == "PDF_PAGE_OCR_FAILED")
        .expect("PDF_PAGE_OCR_FAILED diagnostic");
    assert_eq!(diag.source.as_ref().and_then(|s| s.page), Some(1));
    assert!(diag.message.contains("page 1"));
}
