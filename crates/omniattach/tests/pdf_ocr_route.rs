#![cfg(all(feature = "documents", feature = "backends"))]
use omniattach::{ConversionRequest, Error, FacadeContext, Goal, OmniAttach};
use std::path::PathBuf;

fn blank_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
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
fn pdf_without_ocr_option_yields_no_text_and_page_without_text_warning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pdf = dir.path().join("scan.pdf");
    std::fs::write(&pdf, blank_pdf()).expect("write blank pdf");
    let engine = OmniAttach::builder()
        .with_document_defaults()
        .expect("document routes register")
        .build();
    let request = ConversionRequest::new(&pdf, Goal::Extract).with_route("documents");
    let bundle = engine
        .convert(&request, &FacadeContext::default())
        .expect("default conversion succeeds");
    assert!(bundle.plain_text().trim().is_empty());
    assert!(
        bundle
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "PAGE_WITHOUT_TEXT")
    );
}

#[test]
fn pdf_ocr_validates_language_and_dpi_options() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pdf = dir.path().join("scan.pdf");
    std::fs::write(&pdf, blank_pdf()).expect("write blank pdf");
    let engine = OmniAttach::builder()
        .with_document_defaults()
        .expect("document routes register")
        .build();
    for bad in [
        serde_json::json!({"ocr_dpi": 200}),
        serde_json::json!({"ocr": true, "ocr_dpi": 10}),
        serde_json::json!({"ocr": true, "ocr_dpi": 5000}),
        serde_json::json!({"ocr": true, "ocr_language": ""}),
        serde_json::json!({"ocr": true, "ocr_language": "eng;rm"}),
    ] {
        let req = ConversionRequest::new(&pdf, Goal::Extract)
            .with_route("documents")
            .with_options(bad.clone());
        assert!(
            matches!(
                engine.convert(&req, &FacadeContext::default()),
                Err(Error::Configuration(_))
            ),
            "expected Configuration error for {bad}"
        );
    }
}

/// Needs pdftoppm and tesseract on PATH and an image-only PDF.
/// Run: OMNIATTACH_SCANNED_PDF=/path/scan.pdf OMNIATTACH_EXPECTED_OCR_TEXT=word \
///   cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored
#[test]
#[ignore = "needs pdftoppm, tesseract, OMNIATTACH_SCANNED_PDF and OMNIATTACH_EXPECTED_OCR_TEXT"]
fn pdf_ocr_option_recognizes_scanned_pages() {
    let pdf =
        PathBuf::from(std::env::var("OMNIATTACH_SCANNED_PDF").expect("set OMNIATTACH_SCANNED_PDF"));
    let expected =
        std::env::var("OMNIATTACH_EXPECTED_OCR_TEXT").expect("set OMNIATTACH_EXPECTED_OCR_TEXT");
    let engine = OmniAttach::builder()
        .with_document_defaults()
        .expect("document routes register")
        .build();
    let without_ocr = ConversionRequest::new(&pdf, Goal::Extract).with_route("documents");
    let unocrd = engine
        .convert(&without_ocr, &FacadeContext::default())
        .expect("non-OCR conversion succeeds");
    assert!(
        unocrd.plain_text().trim().is_empty(),
        "expected no text without ocr=true, got: {:?}",
        unocrd.plain_text()
    );
    assert!(
        unocrd
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "PAGE_WITHOUT_TEXT"),
        "expected PAGE_WITHOUT_TEXT warning without ocr=true"
    );

    let request = ConversionRequest::new(pdf, Goal::Extract)
        .with_route("documents")
        .with_options(serde_json::json!({"ocr": true, "ocr_language": "eng", "ocr_dpi": 300}));
    let bundle = engine
        .convert(&request, &FacadeContext::default())
        .expect("OCR conversion succeeds");
    let text = bundle.plain_text().to_lowercase();
    assert!(
        text.contains(&expected.to_lowercase()),
        "OCR text did not contain {expected:?}; got: {text}"
    );
    assert!(
        !bundle
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "PAGE_WITHOUT_TEXT"),
        "PAGE_WITHOUT_TEXT should be cleared when OCR recognizes text"
    );
}
