//! Document routes backed by omniattach-documents. Compiled only with the `documents` feature.
use crate::*;
use omniattach_documents::{Format as DocFormat, Limits, Options};

pub(crate) struct DocumentConverter {
    #[cfg(feature = "backends")]
    runner: std::sync::Arc<dyn omniattach_backends::Runner>,
}

impl DocumentConverter {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(feature = "backends")]
            runner: std::sync::Arc::new(omniattach_backends::ProcessRunner),
        }
    }

    #[cfg(feature = "backends")]
    pub(crate) fn with_runner(runner: std::sync::Arc<dyn omniattach_backends::Runner>) -> Self {
        Self { runner }
    }
}

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
        Format::Mobi => DocFormat::Mobi,
        Format::Azw3 => DocFormat::Azw3,
        Format::Pdf => DocFormat::Pdf,
        Format::Html => DocFormat::Html,
        Format::Csv => DocFormat::Csv,
        Format::Tsv => DocFormat::Tsv,
        Format::Json => DocFormat::Json,
        Format::JsonLines => DocFormat::JsonLines,
        _ => return None,
    })
}

struct OcrConfig {
    enabled: bool,
    language: String,
    dpi: u32,
}

/// Accepts `{}`, `null`, or `{"ocr": bool, "ocr_language": "eng", "ocr_dpi": 300}`.
fn parse_ocr_options(options: &serde_json::Value) -> Result<OcrConfig> {
    let mut cfg = OcrConfig {
        enabled: false,
        language: "eng".into(),
        dpi: 300,
    };
    let mut language_or_dpi_set = false;
    match options {
        serde_json::Value::Null => Ok(cfg),
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                match key.as_str() {
                    "ocr" => {
                        cfg.enabled = value.as_bool().ok_or_else(|| {
                            Error::Configuration("ocr option must be a boolean".into())
                        })?;
                    }
                    "ocr_language" => {
                        let lang = value.as_str().ok_or_else(|| {
                            Error::Configuration("ocr_language option must be a string".into())
                        })?;
                        if lang.trim().is_empty()
                            || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'+')
                        {
                            return Err(Error::Configuration("ocr_language must be a nonempty ASCII Tesseract language code".into()));
                        }
                        cfg.language = lang.into();
                        language_or_dpi_set = true;
                    }
                    "ocr_dpi" => {
                        let dpi = value.as_u64().and_then(|n| u32::try_from(n).ok())
                            .filter(|d| (72..=1200).contains(d))
                            .ok_or_else(|| Error::Configuration("ocr_dpi option must be an integer in 72..=1200".into()))?;
                        cfg.dpi = dpi;
                        language_or_dpi_set = true;
                    }
                    _ => return Err(Error::Configuration("document routes accept only ocr, ocr_language and ocr_dpi options in 0.2.0; call omniattach-documents directly for mappings".into())),
                }
            }
            if language_or_dpi_set && !cfg.enabled {
                return Err(Error::Configuration(
                    "ocr_language and ocr_dpi require ocr: true".into(),
                ));
            }
            Ok(cfg)
        }
        _ => Err(Error::Configuration(
            "document routes accept only an options object in 0.2.0".into(),
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
        let ocr = parse_ocr_options(&request.options)?;
        if input.format == Format::Xml {
            return Err(Error::Configuration("XML needs an explicit column mapping; records are not guessed. Use omniattach-documents::extract_xml_table".into()));
        }
        let format = document_format(input.format).ok_or_else(|| {
            Error::Configuration(format!("no document adapter for {:?}", input.format))
        })?;
        if ocr.enabled && format != DocFormat::Pdf {
            return Err(Error::Configuration(
                "ocr option applies only to PDF documents".into(),
            ));
        }
        #[cfg(feature = "backends")]
        let ocr_words: std::sync::Arc<
            std::sync::Mutex<Vec<omniattach_backends::TextUnit>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        #[cfg(feature = "backends")]
        let pdf_ocr = if ocr.enabled {
            let path = input.path.clone();
            let language = ocr.language.clone();
            let dpi = ocr.dpi;
            let words_out = ocr_words.clone();
            let runner = self.runner.clone();
            Some(omniattach_documents::PdfOcr(std::sync::Arc::new(
                move |page: u32| -> omniattach_documents::Result<String> {
                    let ctx = omniattach_backends::BackendContext::new(runner.as_ref());
                    let units = omniattach_backends::PdfPageOcr::default()
                        .with_language(language.clone())
                        .with_dpi(dpi)
                        .recognize_page_words(&path, page, &ctx)
                        .map_err(|e| omniattach_documents::Error::Parse(format!("OCR: {e}")))?;
                    let text = units
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    words_out.lock().expect("ocr words lock").extend(units);
                    Ok(text)
                },
            )))
        } else {
            None
        };
        #[cfg(not(feature = "backends"))]
        let pdf_ocr = if ocr.enabled {
            return Err(Error::Configuration(
                "ocr option requires the backends feature".into(),
            ));
        } else {
            None
        };
        let options = Options {
            limits: Limits {
                input_bytes: usize::try_from(context.policy().max_input_bytes)
                    .unwrap_or(usize::MAX),
                ..Default::default()
            },
            pdf_ocr,
            ..Default::default()
        };
        let bytes = std::fs::read(&input.path)?;
        context.check()?;
        let document =
            omniattach_documents::extract_as(&input.source.name, &bytes, format, &options)
                .map_err(document_error)?;
        context.check()?;
        let ocr_pages: Vec<u32> = document
            .blocks
            .iter()
            .filter(|b| b.source.path.as_deref() == Some("ocr"))
            .filter_map(|b| b.source.page)
            .collect();
        let mut data = ConversionData::default().payload("omniattach-documents/0.1", &document)?;
        #[cfg(feature = "backends")]
        if !ocr_pages.is_empty() {
            let collected = ocr_words.lock().expect("ocr words lock").clone();
            data = data.payload("omniattach-backends/pdf-ocr/0.1", serde_json::json!({
                "language": ocr.language, "dpi": ocr.dpi, "confidence_scale": "tesseract_native_0_100",
                "coordinates": "rasterized_page_pixels", "words": collected,
            }))?;
        }
        data.text.push(TextFragment {
            text: document.to_text(),
            location: serde_json::json!({"kind": "document_text", "ocr_pages": ocr_pages}),
        });
        if !ocr_pages.is_empty() {
            data.warn("OCR_TEXT", format!("Text on pages {ocr_pages:?} was recognized by local Tesseract OCR and may contain recognition errors"));
        }
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
                        .push(PendingArtifact::new(&file, "text/csv").located(
                            serde_json::json!({"table_id": table.id, "name": table.name}),
                        ));
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
        self.with_document_converter(DocumentConverter::new())
    }
    /// Same registration, with an explicit runner for OCR subprocesses.
    #[cfg(feature = "backends")]
    pub fn with_document_defaults_runner(
        self,
        runner: std::sync::Arc<dyn omniattach_backends::Runner>,
    ) -> Result<Self> {
        self.with_document_converter(DocumentConverter::with_runner(runner))
    }
    fn with_document_converter(self, converter: DocumentConverter) -> Result<Self> {
        self.register(Route::new(
            "documents",
            vec![Format::Docx, Format::Pptx, Format::Odt, Format::Odp, Format::Xlsx, Format::Ods,
                 Format::Epub, Format::Mobi, Format::Azw3, Format::Pdf, Format::Html, Format::Csv,
                 Format::Tsv, Format::Json, Format::JsonLines, Format::Xml],
            vec![Goal::Extract, Goal::Markdown, Goal::Tables],
            ExecutionKind::InProcess,
        ).described("omniattach-documents: Office, ODF, EPUB, MOBI, experimental AZW3, PDF, HTML, CSV, JSON; XML needs a mapping"), converter)
    }
}
