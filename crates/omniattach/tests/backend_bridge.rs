//! Requires the actual sibling omniattach-backends crate. Not run by the portable harness.
#![cfg(feature = "backends")]
use omniattach::*;
use std::{sync::Arc, time::Duration};
struct PdfRunner;
impl backends::Runner for PdfRunner {
    fn run(
        &self,
        spec: &backends::CommandSpec,
        options: &backends::RunOptions,
    ) -> backends::Result<backends::ProcessOutput> {
        assert_eq!(spec.program, std::path::Path::new("pdftotext"));
        assert!(!options.cancellation.is_cancelled());
        Ok(backends::ProcessOutput {
            stdout: b"One\x0cTwo\x0c".to_vec(),
            stderr: vec![],
            elapsed: Duration::ZERO,
        })
    }
}
#[test]
fn bridge_maps_pdf_text_without_inventing_page_locations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.pdf");
    std::fs::write(&source, b"%PDF-1.4").unwrap();
    let bridge =
        ExternalAdapter::new(backends::Poppler::default()).with_runner(Arc::new(PdfRunner));
    let engine = OmniAttach::builder()
        .register(
            Route::new(
                "pdf",
                vec![Format::Pdf],
                vec![Goal::Extract],
                ExecutionKind::External,
            ),
            bridge,
        )
        .unwrap()
        .build();
    let bundle = engine
        .convert(
            &ConversionRequest::new(source, Goal::Extract),
            &FacadeContext::default().with_external(true),
        )
        .unwrap();
    assert_eq!(bundle.manifest().text.len(), 2);
    assert_eq!(bundle.manifest().text[1].location["page"], 2);
    assert!(bundle.payload("omniattach-backends/0.1").is_some());
}
#[test]
fn explicit_external_defaults_are_registration_only() {
    let engine = OmniAttach::builder()
        .with_external_defaults()
        .unwrap()
        .build();
    assert_eq!(engine.capabilities().len(), 8);
    assert!(
        engine
            .capabilities()
            .iter()
            .all(|r| r.execution == ExecutionKind::External)
    );
}
#[test]
fn model_configuration_does_not_load_model_until_conversion() {
    let engine = OmniAttach::builder()
        .with_whisper_cpp("/not-installed/model.bin", "hu")
        .unwrap()
        .build();
    assert_eq!(engine.capabilities()[0].id, "whisper-cpp");
}
