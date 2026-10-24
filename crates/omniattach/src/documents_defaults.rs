//! Document routes backed by omniattach-documents. Compiled only with the `documents` feature.
use crate::*;
use omniattach_documents::{Format as DocFormat, Options};

struct DocumentConverter;

fn document_error(error: impl std::fmt::Display) -> Error {
    Error::Converter {
        route: "documents".into(),
        message: error.to_string(),
    }
}

fn document_format(format: Format) -> Option<DocFormat> {
    Some(match format {
        Format::Docx => DocFormat::Docx,
        Format::Pptx => DocFormat::Pptx,
        Format::Odt => DocFormat::Odt,
        Format::Odp => DocFormat::Odp,
        Format::Xlsx | Format::Ods => DocFormat::Workbook,
        Format::Epub => DocFormat::Epub,
        Format::Pdf => DocFormat::Pdf,
        Format::Html => DocFormat::Html,
        Format::Csv => DocFormat::Csv,
        Format::Tsv => DocFormat::Tsv,
        Format::Json => DocFormat::Json,
        Format::JsonLines => DocFormat::JsonLines,
        _ => return None,
    })
}

/// Accepts only `{}`, `null`, or `{"ocr": true|false}`. OCR is off unless requested.
fn parse_ocr_option(options: &serde_json::Value) -> Result<bool> {
    match options {
        serde_json::Value::Null => Ok(false),
        serde_json::Value::Object(map) if map.keys().all(|k| k == "ocr") => match map.get("ocr") {
            None => Ok(false),
            Some(serde_json::Value::Bool(enabled)) => Ok(*enabled),
            Some(_) => Err(Error::Configuration("ocr option must be a boolean".into())),
        },
        _ => Err(Error::Configuration(
            "document routes accept only the ocr boolean option in 0.2.0; call omniattach-documents directly for mappings".into(),
        )),
    }
}

impl Converter for DocumentConverter {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData> {
        context.check()?;
        let ocr = parse_ocr_option(&request.options)?;
        if input.format == Format::Xml {
            return Err(Error::Configuration(
                "XML needs an explicit column mapping; records are not guessed. Use omniattach-documents::extract_xml_table".into(),
            ));
        }
        let format = document_format(input.format).ok_or_else(|| {
            Error::Configuration(format!("no document adapter for {:?}", input.format))
        })?;
        if ocr && format != DocFormat::Pdf {
            return Err(Error::Configuration(
                "ocr option applies only to PDF documents".into(),
            ));
        }
        let mut options = Options::default();
        options.limits.input_bytes =
            usize::try_from(context.policy().max_input_bytes).unwrap_or(usize::MAX);
        if ocr {
            #[cfg(feature = "backends")]
            {
                let path = input.path.clone();
                options.pdf_ocr = Some(omniattach_documents::PdfOcr(std::sync::Arc::new(
                    move |page: u32| -> omniattach_documents::Result<String> {
                        let runner = omniattach_backends::ProcessRunner;
                        let ctx = omniattach_backends::BackendContext::new(&runner);
                        omniattach_backends::PdfPageOcr::default()
                            .recognize_page(&path, page, &ctx)
                            .map_err(|e| omniattach_documents::Error::Parse(format!("OCR: {e}")))
                    },
                )));
            }
            #[cfg(not(feature = "backends"))]
            return Err(Error::Configuration(
                "ocr option requires the backends feature".into(),
            ));
        }
        let bytes = std::fs::read(&input.path)?;
        context.check()?;
        let document =
            omniattach_documents::extract_as(&input.source.name, &bytes, format, &options)
                .map_err(document_error)?;
        context.check()?;
        let mut data = ConversionData::default().payload("omniattach-documents/0.1", &document)?;
        data.text.push(TextFragment {
            text: document.to_text(),
            location: serde_json::json!({"kind": "document_text"}),
        });
        std::fs::write(
            input.output_dir.join("document.json"),
            document.to_json().map_err(document_error)?,
        )?;
        data.artifacts
            .push(PendingArtifact::new("document.json", "application/json"));
        match request.goal {
            Goal::Markdown => {
                std::fs::write(input.output_dir.join("document.md"), document.to_markdown())?;
                data.artifacts
                    .push(PendingArtifact::new("document.md", "text/markdown"));
            }
            Goal::Tables => {
                for (index, table) in document.tables.iter().enumerate() {
                    let file = format!("table-{:04}.csv", index + 1);
                    let csv = table.to_csv().map_err(document_error)?;
                    std::fs::write(input.output_dir.join(&file), csv)?;
                    data.artifacts
                        .push(
                            PendingArtifact::new(&file, "text/csv").located(serde_json::json!({
                                "table_id": table.id,
                                "name": table.name,
                            })),
                        );
                }
            }
            _ => {}
        }
        for diagnostic in &document.diagnostics {
            data.warn(diagnostic.code.clone(), diagnostic.message.clone());
        }
        context.check()?;
        Ok(data)
    }
}

impl OmniAttachBuilder {
    /// Registers omniattach-documents routes. XML is accepted only to fail with a mapping error.
    pub fn with_document_defaults(self) -> Result<Self> {
        self.register(
            Route::new(
                "documents",
                vec![
                    Format::Docx,
                    Format::Pptx,
                    Format::Odt,
                    Format::Odp,
                    Format::Xlsx,
                    Format::Ods,
                    Format::Epub,
                    Format::Pdf,
                    Format::Html,
                    Format::Csv,
                    Format::Tsv,
                    Format::Json,
                    Format::JsonLines,
                    Format::Xml,
                ],
                vec![Goal::Extract, Goal::Markdown, Goal::Tables],
                ExecutionKind::InProcess,
            )
            .described("omniattach-documents: Office, ODF, EPUB, PDF, HTML, CSV, JSON; XML needs a mapping"),
            DocumentConverter,
        )
    }
}
