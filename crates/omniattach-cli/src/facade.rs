//! Targets the omniattach facade public API.
//! This module is compiled by the real workspace, not by the no-sibling verification harness.
use crate::{CliError, Result, Runtime, args::ProgressMode, service::*};
use omniattach::{ConversionRequest, FacadeContext, Format, Goal, NdjsonProgress, OmniAttach};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub struct FacadeEngine {
    engine: OmniAttach,
    base_context: FacadeContext,
}
impl FacadeEngine {
    pub fn new(config: &EngineConfig) -> Result<Self> {
        #[allow(unused_mut)]
        let mut builder = OmniAttach::builder()
            .with_portable_defaults()
            .map_err(map_error)?;
        #[cfg(feature = "documents")]
        {
            builder = builder.with_document_defaults().map_err(map_error)?;
        }
        if config.external_backends {
            #[cfg(feature = "backends")]
            {
                builder = builder.with_external_defaults().map_err(map_error)?;
                #[cfg(feature = "documents")]
                {
                    builder = builder.with_legacy_office_pipelines().map_err(map_error)?;
                }
                if let Some(model) = &config.whisper_model {
                    builder = builder
                        .with_whisper_cpp(model, &config.language)
                        .map_err(map_error)?;
                }
            }
            #[cfg(not(feature = "backends"))]
            {
                return Err(CliError::unsupported(
                    "--external-backends requires the backends Cargo feature",
                ));
            }
        }
        Ok(Self {
            engine: builder.build(),
            base_context: FacadeContext::default(),
        })
    }
    /// Embed a real preconfigured engine, e.g. register the actual document extractor.
    pub fn from_engine(engine: OmniAttach) -> Self {
        Self {
            engine,
            base_context: FacadeContext::default(),
        }
    }
    pub fn runtime(&self) -> Runtime {
        let cancellation = self.base_context.cancellation.clone();
        Runtime::with_cancel_callback(move || cancellation.cancel())
    }
    fn context(&self, request: &EngineRequest, progress: ProgressMode) -> FacadeContext {
        let mut context = self.base_context.clone();
        context.policy.allow_external = request.allow_external;
        context.policy.max_input_bytes = request.max_input_bytes;
        context.policy.max_export_bytes = request.max_export_bytes;
        context.policy.max_export_files = request.max_export_files;
        context.policy.timeout = request.timeout_ms.map(Duration::from_millis);
        context.policy.scratch_parent = request.scratch_parent.clone();
        if progress == ProgressMode::Ndjson {
            context = context.with_progress(NdjsonProgress);
        }
        context
    }
}
fn request(input: &EngineRequest) -> Result<ConversionRequest> {
    let goal = match input.goal.as_str() {
        "extract" => Goal::Extract,
        "markdown" => Goal::Markdown,
        "tables" => Goal::Tables,
        "pdf-layout" => Goal::PdfLayout,
        "pdf-scientific" => Goal::PdfScientific,
        "ocr" => Goal::Ocr,
        "transcribe" => Goal::Transcribe,
        "video-frames" => Goal::VideoFrames,
        "media-probe" => Goal::MediaProbe,
        "normalize" => Goal::Normalize,
        custom if custom.starts_with("custom:") => Goal::Custom(custom[7..].into()),
        other => return Err(CliError::usage(format!("unknown goal {other}"))),
    };
    let mut converted =
        ConversionRequest::new(&input.input, goal).with_options(input.options.clone());
    if let Some(format) = &input.format {
        let format = match format.as_str() {
            "text" => Format::Text,
            "markdown" => Format::Markdown,
            "html" => Format::Html,
            "xml" => Format::Xml,
            "csv" => Format::Csv,
            "tsv" => Format::Tsv,
            "json" => Format::Json,
            "json-lines" | "json_lines" => Format::JsonLines,
            "pdf" => Format::Pdf,
            "docx" => Format::Docx,
            "pptx" => Format::Pptx,
            "xlsx" => Format::Xlsx,
            "odt" => Format::Odt,
            "odp" => Format::Odp,
            "ods" => Format::Ods,
            "epub" => Format::Epub,
            "doc" => Format::Doc,
            "ppt" => Format::Ppt,
            "xls" => Format::Xls,
            "rtf" => Format::Rtf,
            "audio" => Format::Audio,
            "video" => Format::Video,
            "image" => Format::Image,
            "zip" => Format::Zip,
            other => return Err(CliError::usage(format!("unknown format {other}"))),
        };
        converted = converted.with_format(format);
    }
    if let Some(route) = &input.route {
        converted = converted.with_route(route);
    }
    Ok(converted)
}
impl Engine for FacadeEngine {
    fn capabilities(&self) -> Result<Value> {
        Ok(
            json!({"routes":self.engine.capabilities(),"conversion_enabled":true,"installed_tools_probed":false,"documents_auto_registered":cfg!(feature = "documents")}),
        )
    }
    fn plan(&self, input: &EngineRequest) -> Result<Value> {
        let plan = self
            .engine
            .plan(&request(input)?, &self.context(input, ProgressMode::None))
            .map_err(map_error)?;
        Ok(serde_json::to_value(plan)?)
    }
    fn convert(
        &self,
        input: &EngineRequest,
        output: &Path,
        progress: ProgressMode,
    ) -> Result<ConversionOutcome> {
        let context = self.context(input, progress);
        let bundle = self
            .engine
            .convert(&request(input)?, &context)
            .map_err(map_error)?;
        // Export while the bundle owns its temporary directory.
        bundle.export(output).map_err(map_error)?;
        let manifest = serde_json::to_value(bundle.manifest())?;
        let text = bundle.plain_text();
        Ok(ConversionOutcome { manifest, text })
    }
}
pub fn map_error(error: omniattach::Error) -> CliError {
    use omniattach::Error as E;
    let (code, exit) = match &error {
        E::Configuration(_) => ("INVALID_ARGUMENT", 2),
        E::UnknownFormat | E::NoRoute { .. } | E::UnknownRoute(_) => ("NO_ROUTE", 4),
        E::AmbiguousRoute(_) => ("AMBIGUOUS_ROUTE", 4),
        E::RouteMismatch { .. } => ("ROUTE_MISMATCH", 4),
        E::ExternalDisabled => ("EXTERNAL_DISABLED", 5),
        E::Cancelled => ("CANCELLED", 130),
        E::Deadline => ("TIMEOUT", 124),
        E::Limit(_) => ("LIMIT_EXCEEDED", 6),
        E::Io(_) => ("IO_ERROR", 3),
        E::InvalidInput(_) => ("INVALID_INPUT", 3),
        E::Json(_) | E::InvalidOutput(_) | E::Converter { .. } => ("CONVERSION_FAILED", 1),
        #[allow(unreachable_patterns)]
        _ => ("CONVERSION_FAILED", 1),
    };
    CliError::new(code, error.to_string(), exit)
}
