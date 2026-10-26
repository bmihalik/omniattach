#![cfg(all(feature = "documents", feature = "backends"))]
use omniattach::{ConversionRequest, Error, FacadeContext, Goal, OmniAttach};
use std::{path::PathBuf, sync::Arc, time::Duration};

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

const FAKE_TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
5\t1\t1\t1\t1\t1\t10\t20\t30\t40\t95.5\tScanned\n\
5\t1\t1\t1\t1\t2\t50\t20\t35\t40\t92.0\twords\n";

struct FakeOcrRunner;
impl omniattach::backends::Runner for FakeOcrRunner {
    fn run(
        &self,
        spec: &omniattach::backends::CommandSpec,
        _: &omniattach::backends::RunOptions,
    ) -> omniattach::backends::Result<omniattach::backends::ProcessOutput> {
        let program = spec
            .program
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stdout = match program.as_str() {
            "pdftoppm" => {
                std::fs::write(spec.cwd.join("page.png"), b"png")?;
                Vec::new()
            }
            "tesseract" => FAKE_TSV.as_bytes().to_vec(),
            other => panic!("unexpected program {other}"),
        };
        Ok(omniattach::backends::ProcessOutput {
            stdout,
            stderr: Vec::new(),
            elapsed: Duration::ZERO,
        })
    }
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
fn pdf_ocr_route_with_fake_runner_recognizes_words_without_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pdf = dir.path().join("scan.pdf");
    std::fs::write(&pdf, blank_pdf()).expect("write blank pdf");
    let engine = OmniAttach::builder()
        .with_document_defaults_runner(Arc::new(FakeOcrRunner))
        .expect("document routes register")
        .build();
    let request = ConversionRequest::new(&pdf, Goal::Extract)
        .with_route("documents")
        .with_options(serde_json::json!({"ocr": true}));
    let bundle = engine
        .convert(&request, &FacadeContext::default())
        .expect("fake OCR conversion succeeds");
    assert!(bundle.plain_text().contains("Scanned words"));
    assert!(
        bundle
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "OCR_TEXT")
    );
    assert!(
        !bundle
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "PAGE_WITHOUT_TEXT")
    );
    assert!(
        bundle
            .manifest()
            .text
            .iter()
            .any(|t| t.location["ocr_pages"] == serde_json::json!([1]))
    );
    let payload = bundle
        .payload("omniattach-backends/pdf-ocr/0.1")
        .expect("pdf-ocr payload");
    let words = payload["words"].as_array().expect("words array");
    assert_eq!(words.len(), 2);
    assert_eq!(words[0]["location"]["page"], 1);
    assert_eq!(words[0]["location"]["bbox"]["width"], 30);
    assert_eq!(words[0]["confidence"], 95.5);
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
    assert!(
        bundle
            .manifest()
            .warnings
            .iter()
            .any(|w| w.code == "OCR_TEXT"),
        "expected OCR_TEXT warning when OCR recognized text"
    );
    assert!(
        bundle
            .manifest()
            .text
            .iter()
            .any(|t| t.location["ocr_pages"] == serde_json::json!([1])),
        "expected ocr_pages [1] in text location"
    );
    let ocr_payload = bundle
        .payload("omniattach-backends/pdf-ocr/0.1")
        .expect("expected omniattach-backends/pdf-ocr/0.1 payload");
    let words = ocr_payload["words"]
        .as_array()
        .expect("words array in pdf-ocr payload");
    assert!(!words.is_empty(), "expected non-empty OCR word units");
    assert_eq!(words[0]["location"]["page"], 1);
    assert!(words[0]["location"]["bbox"]["width"].as_u64().unwrap_or(0) > 0);
    assert!(words[0]["confidence"].as_f64().is_some());
}
