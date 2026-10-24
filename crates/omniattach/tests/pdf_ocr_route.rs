#![cfg(all(feature = "documents", feature = "backends"))]
use omniattach::{ConversionRequest, FacadeContext, Goal, OmniAttach};
use std::path::PathBuf;

/// Needs pdftoppm and tesseract on PATH and an image-only PDF.
/// Run: OMNIATTACH_SCANNED_PDF=/path/scan.pdf OMNIATTACH_EXPECTED_OCR_TEXT=word \
///   cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored
#[test]
#[ignore = "needs pdftoppm, tesseract, OMNIATTACH_SCANNED_PDF and OMNIATTACH_EXPECTED_OCR_TEXT"]
fn pdf_ocr_option_recognizes_scanned_pages() {
    let pdf = PathBuf::from(
        std::env::var("OMNIATTACH_SCANNED_PDF").expect("set OMNIATTACH_SCANNED_PDF"),
    );
    let expected = std::env::var("OMNIATTACH_EXPECTED_OCR_TEXT")
        .expect("set OMNIATTACH_EXPECTED_OCR_TEXT");
    let engine = OmniAttach::builder()
        .with_document_defaults()
        .expect("document routes register")
        .build();
    let request = ConversionRequest::new(pdf, Goal::Extract)
        .with_route("documents")
        .with_options(serde_json::json!({"ocr": true}));
    let bundle = engine
        .convert(&request, &FacadeContext::default())
        .expect("OCR conversion succeeds");
    let text = bundle.plain_text().to_lowercase();
    assert!(
        text.contains(&expected.to_lowercase()),
        "OCR text did not contain {expected:?}; got: {text}"
    );
}
