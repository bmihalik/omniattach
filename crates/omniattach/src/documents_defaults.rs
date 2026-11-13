//! Document routes backed by omniattach-documents.
use crate::*;
use omniattach_documents::{Format as DocFormat, Limits, Options};
pub(crate) struct DocumentConverter {
    #[cfg(feature = "backends")] runner: std::sync::Arc<dyn omniattach_backends::Runner>,
}
impl DocumentConverter {
    pub(crate) fn new() -> Self { Self { #[cfg(feature = "backends")] runner: std::sync::Arc::new(omniattach_backends::ProcessRunner) } }
    #[cfg(feature = "backends")]
    pub(crate) fn with_runner(runner: std::sync::Arc<dyn omniattach_backends::Runner>) -> Self { Self { runner } }
}
fn document_error(error: omniattach_documents::Error) -> Error { Error::Document(error) }
fn document_format(format: Format) -> Option<DocFormat> {
    Some(match format {
        Format::Docx=>DocFormat::Docx, Format::Pptx=>DocFormat::Pptx, Format::Odt=>DocFormat::Odt, Format::Odp=>DocFormat::Odp,
        Format::Xlsx | Format::Ods=>DocFormat::Workbook, Format::Epub=>DocFormat::Epub, Format::Mobi=>DocFormat::Mobi, Format::Azw3=>DocFormat::Azw3,
        Format::Pdf=>DocFormat::Pdf, Format::Html=>DocFormat::Html, Format::Csv=>DocFormat::Csv, Format::Tsv=>DocFormat::Tsv, Format::Json=>DocFormat::Json, Format::JsonLines=>DocFormat::JsonLines, _=>return None,
    })
}
struct DocumentConfig { enabled: bool, language: String, dpi: u32, dehyphenate_max_tokens: usize }
fn parse_document_options(options: &serde_json::Value) -> Result<DocumentConfig> {
    let mut cfg = DocumentConfig { enabled: false, language: "eng".into(), dpi: 300, dehyphenate_max_tokens: Options::default().dehyphenate_max_tokens };
    let mut language_or_dpi_set = false;
    match options {
        serde_json::Value::Null => Ok(cfg),
        serde_json::Value::Object(map) => {
            for (key,value) in map {
                match key.as_str() {
                    "ocr" => cfg.enabled = value.as_bool().ok_or_else(|| Error::Configuration("ocr option must be a boolean".into()))?,
                    "ocr_language" => {
                        let lang = value.as_str().ok_or_else(|| Error::Configuration("ocr_language option must be a string".into()))?;
                        if lang.trim().is_empty() || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'+') { return Err(Error::Configuration("ocr_language must be a nonempty ASCII Tesseract language code".into())); }
                        cfg.language = lang.into(); language_or_dpi_set = true;
                    }
                    "ocr_dpi" => {
                        cfg.dpi = value.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|d| (72..=1200).contains(d)).ok_or_else(|| Error::Configuration("ocr_dpi option must be an integer in 72..=1200".into()))?; language_or_dpi_set = true;
                    }
                    "dehyphenate_max_tokens" => cfg.dehyphenate_max_tokens = value.as_u64().filter(|n| *n <= 64).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| Error::Configuration("dehyphenate_max_tokens option must be an integer in 0..=64".into()))?,
                    _ => return Err(Error::Configuration("document routes accept only ocr, ocr_language, ocr_dpi and dehyphenate_max_tokens; call omniattach-documents directly for mappings".into())),
                }
            }
            if language_or_dpi_set && !cfg.enabled { return Err(Error::Configuration("ocr_language and ocr_dpi require ocr: true".into())); } Ok(cfg)
        }
        _ => Err(Error::Configuration("document routes accept an options object or null".into())),
    }
}
impl Converter for DocumentConverter {
    fn convert(&self, input: &StagedInput, request: &ConversionRequest, context: &JobContext) -> Result<ConversionData> {
        context.check()?; let ocr = parse_document_options(&request.options)?;
        if input.format == Format::Xml { return Err(Error::Configuration("XML needs an explicit column mapping; records are not guessed. Use omniattach-documents::extract_xml_table".into())); }
        let format = document_format(input.format).ok_or_else(|| Error::Configuration(format!("no document adapter for {:?}", input.format)))?;
        if ocr.enabled && format != DocFormat::Pdf { return Err(Error::Configuration("ocr option applies only to PDF documents".into())); }
        #[cfg(feature = "backends")]
        let ocr_words: std::sync::Arc<std::sync::Mutex<Vec<omniattach_backends::TextUnit>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        #[cfg(feature = "backends")]
        let pdf_ocr = if ocr.enabled {
            let path = input.path.clone(); let language = ocr.language.clone(); let dpi = ocr.dpi; let words_out = ocr_words.clone(); let runner = self.runner.clone(); let job = context.clone();
            Some(omniattach_documents::PdfOcr(std::sync::Arc::new(move |page: u32| -> omniattach_documents::Result<String> {
                job.check().map_err(|e| omniattach_documents::Error::Parse(format!("OCR context: {e}")))?;
                let mut ctx = omniattach_backends::BackendContext::new(runner.as_ref());
                ctx.run_options.cancellation = job.cancellation().clone();
                if let Some(remaining) = job.remaining() { ctx.run_options.timeout = ctx.run_options.timeout.min(remaining); }
                let units = omniattach_backends::PdfPageOcr::default().with_language(language.clone()).with_dpi(dpi).recognize_page_words(&path,page,&ctx).map_err(|e| omniattach_documents::Error::Parse(format!("OCR: {e}")))?;
                job.check().map_err(|e| omniattach_documents::Error::Parse(format!("OCR context: {e}")))?;
                let text = units.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
                words_out.lock().expect("ocr words lock").extend(units); Ok(text)
            })))
        } else { None };
        #[cfg(not(feature = "backends"))]
        let pdf_ocr = if ocr.enabled { return Err(Error::Configuration("ocr option requires the backends feature".into())); } else { None };
        let options = Options { limits: Limits { input_bytes: usize::try_from(context.policy().max_input_bytes).unwrap_or(usize::MAX), ..Default::default() }, pdf_ocr, dehyphenate_max_tokens: ocr.dehyphenate_max_tokens, ..Default::default() };
        let bytes = std::fs::read(&input.path)?; context.check()?;
        let document = omniattach_documents::extract_as(&input.source.name,&bytes,format,&options).map_err(document_error)?;
        context.check()?;
        let ocr_pages: Vec<u32> = document.blocks.iter().filter(|b| b.source.path.as_deref() == Some("ocr")).filter_map(|b| b.source.page).collect();
        let mut data = ConversionData::default().payload("omniattach-documents/0.1", &document)?;
        #[cfg(feature = "backends")]
        if !ocr_pages.is_empty() {
            let collected = ocr_words.lock().expect("ocr words lock").clone();
            data = data.payload("omniattach-backends/pdf-ocr/0.1",serde_json::json!({"language":ocr.language,"dpi":ocr.dpi,"confidence_scale":"tesseract_native_0_100","coordinates":"rasterized_page_pixels","words":collected}))?;
        }
        data.text.push(TextFragment { text: document.to_text(), location: serde_json::json!({"kind":"document_text","ocr_pages":ocr_pages}) });
        if !ocr_pages.is_empty() { data.warn("OCR_TEXT",format!("Text on pages {ocr_pages:?} was recognized by local Tesseract OCR and may contain recognition errors")); }
        std::fs::write(input.output_dir.join("document.json"),document.to_json().map_err(document_error)?)?; data.artifacts.push(PendingArtifact::new("document.json","application/json"));
        match request.goal {
            Goal::Markdown => { std::fs::write(input.output_dir.join("document.md"),document.to_markdown())?; data.artifacts.push(PendingArtifact::new("document.md","text/markdown")); }
            Goal::Tables => { for (index,table) in document.tables.iter().enumerate() { let file = format!("table-{:04}.csv",index+1); let csv = table.to_csv().map_err(document_error)?; std::fs::write(input.output_dir.join(&file),csv)?; data.artifacts.push(PendingArtifact::new(&file,"text/csv").located(serde_json::json!({"table_id":table.id,"name":table.name}))); } }
            _ => {},
        }
        for diagnostic in &document.diagnostics { data.warn(diagnostic.code.clone(),diagnostic.message.clone()); }
        context.check()?; Ok(data)
    }
}
impl OmniAttachBuilder {
    pub fn with_document_defaults(self) -> Result<Self> { self.with_document_converter(DocumentConverter::new()) }
    #[cfg(feature = "backends")]
    pub fn with_document_defaults_runner(self,runner: std::sync::Arc<dyn omniattach_backends::Runner>) -> Result<Self> { self.with_document_converter(DocumentConverter::with_runner(runner)) }
    fn with_document_converter(self,converter: DocumentConverter) -> Result<Self> {
        self.register(Route::new("documents",vec![Format::Docx,Format::Pptx,Format::Odt,Format::Odp,Format::Xlsx,Format::Ods,Format::Epub,Format::Mobi,Format::Azw3,Format::Pdf,Format::Html,Format::Csv,Format::Tsv,Format::Json,Format::JsonLines,Format::Xml],vec![Goal::Extract,Goal::Markdown,Goal::Tables],ExecutionKind::InProcess).described("omniattach-documents: Office, ODF, EPUB, MOBI, experimental AZW3, PDF, HTML, CSV, JSON; XML needs a mapping"),converter)
    }
}
#[cfg(test)]
mod option_tests {
    use super::*; use serde_json::json;
    #[test]
    fn document_option_defaults_valid_values_and_zero_disable() {
        assert_eq!(parse_document_options(&json!({})).unwrap().dehyphenate_max_tokens,6);
        assert_eq!(parse_document_options(&serde_json::Value::Null).unwrap().dehyphenate_max_tokens,6);
        for n in [0,1,6,64] { let cfg = parse_document_options(&json!({"dehyphenate_max_tokens":n})).unwrap(); assert_eq!(cfg.dehyphenate_max_tokens,n as usize); assert!(!cfg.enabled); }
        let cfg = parse_document_options(&json!({"ocr":true,"ocr_language":"eng+hun","ocr_dpi":300,"dehyphenate_max_tokens":0})).unwrap(); assert!(cfg.enabled); assert_eq!(cfg.dehyphenate_max_tokens,0);
    }
    #[test]
    fn document_option_types_ranges_unknown_keys_and_ocr_rules() {
        for value in [json!(-1),json!(65),json!("6"),json!(true),json!(null),json!(1.5),json!(6.0)] { assert!(parse_document_options(&json!({"dehyphenate_max_tokens":value})).is_err()); }
        for value in [json!([]),json!(false),json!({"unknown":1}),json!({"ocr_language":"eng"}),json!({"ocr_dpi":300})] { assert!(parse_document_options(&value).is_err()); }
    }
    #[test]
    fn native_errors_are_not_flattened() {
        assert!(matches!(document_error(omniattach_documents::Error::LimitExceeded("fixture".into())), Error::Document(omniattach_documents::Error::LimitExceeded(_))));
        assert!(matches!(document_error(omniattach_documents::Error::InvalidInput("fixture".into())), Error::Document(omniattach_documents::Error::InvalidInput(_))));
        assert!(matches!(document_error(omniattach_documents::Error::Parse("fixture".into())), Error::Document(omniattach_documents::Error::Parse(_))));
    }
}
