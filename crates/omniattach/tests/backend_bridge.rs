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

#[cfg(feature = "documents")]
mod legacy_office {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for &(name, data) in entries {
            let offset = out.len() as u32;
            let crc = crc32(data);
            let len = data.len() as u32;
            let nlen = name.len() as u16;
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);

            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&len.to_le_bytes());
            central.extend_from_slice(&len.to_le_bytes());
            central.extend_from_slice(&nlen.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        let count = entries.len() as u16;
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn minimal_docx(text: &str) -> Vec<u8> {
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
        );
        stored_zip(&[("word/document.xml", xml.as_bytes())])
    }

    fn minimal_pptx(text: &str) -> Vec<u8> {
        let pres = r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="1" r:id="rA"/></p:sldIdLst></p:presentation>"#;
        let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rA" Target="slides/slide1.xml"/></Relationships>"#;
        let slide = format!(
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
        );
        stored_zip(&[
            ("ppt/presentation.xml", pres.as_bytes()),
            ("ppt/_rels/presentation.xml.rels", rels.as_bytes()),
            ("ppt/slides/slide1.xml", slide.as_bytes()),
        ])
    }

    struct FakeLibreOfficeRunner;
    impl backends::Runner for FakeLibreOfficeRunner {
        fn run(
            &self,
            spec: &backends::CommandSpec,
            _: &backends::RunOptions,
        ) -> backends::Result<backends::ProcessOutput> {
            assert_eq!(spec.program, std::path::Path::new("libreoffice"));
            let args: Vec<String> = spec
                .args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let target = args
                .windows(2)
                .find(|w| w[0] == "--convert-to")
                .map(|w| w[1].as_str())
                .expect("--convert-to argument");
            match target {
                "docx" => {
                    std::fs::write(
                        spec.cwd.join("out/input.docx"),
                        minimal_docx("Legacy Word body"),
                    )?;
                }
                "pptx" => {
                    std::fs::write(
                        spec.cwd.join("out/input.pptx"),
                        minimal_pptx("Legacy Slide body"),
                    )?;
                }
                other => panic!("unexpected LibreOffice target {other}"),
            }
            Ok(backends::ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                elapsed: Duration::ZERO,
            })
        }
    }

    #[test]
    fn legacy_office_pipelines_convert_doc_rtf_and_ppt_through_documents() {
        let engine = OmniAttach::builder()
            .with_legacy_office_pipelines_runner(Arc::new(FakeLibreOfficeRunner))
            .unwrap()
            .build();
        let ids: Vec<String> = engine.capabilities().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["legacy-doc-rtf", "legacy-ppt"]);

        let temp = tempfile::tempdir().unwrap();
        let doc = temp.path().join("legacy.doc");
        std::fs::write(&doc, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1word").unwrap();
        let rtf = temp.path().join("legacy.rtf");
        std::fs::write(&rtf, b"{\\rtf1\\ansi hello}").unwrap();
        let ppt = temp.path().join("legacy.ppt");
        std::fs::write(&ppt, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1slides").unwrap();

        assert!(matches!(
            engine.convert(
                &ConversionRequest::new(&doc, Goal::Extract),
                &FacadeContext::default()
            ),
            Err(Error::ExternalDisabled)
        ));

        let ctx = FacadeContext::default().with_external(true);
        for source in [&doc, &rtf] {
            let bundle = engine
                .convert(&ConversionRequest::new(source, Goal::Markdown), &ctx)
                .unwrap();
            assert_eq!(bundle.manifest().route.id, "legacy-doc-rtf");
            assert_eq!(bundle.plain_text(), "Legacy Word body");
            assert!(bundle.payload("omniattach/pipeline/0.1").is_some());
            assert!(bundle.payload("omniattach-documents/0.1").is_some());
            assert!(
                bundle
                    .manifest()
                    .artifacts
                    .iter()
                    .any(|a| a.path == "stage-1/backend-000000.docx")
            );
            assert!(
                bundle
                    .manifest()
                    .artifacts
                    .iter()
                    .any(|a| a.path == "stage-2/document.md")
            );
        }

        let ppt_bundle = engine
            .convert(&ConversionRequest::new(&ppt, Goal::Extract), &ctx)
            .unwrap();
        assert_eq!(ppt_bundle.manifest().route.id, "legacy-ppt");
        assert_eq!(ppt_bundle.plain_text(), "Legacy Slide body");
        assert!(ppt_bundle.payload("omniattach/pipeline/0.1").is_some());
        assert!(ppt_bundle.payload("omniattach-documents/0.1").is_some());
        assert!(
            ppt_bundle
                .manifest()
                .artifacts
                .iter()
                .any(|a| a.path == "stage-1/backend-000000.pptx")
        );
    }
}
