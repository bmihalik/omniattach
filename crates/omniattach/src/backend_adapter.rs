//! Integration targets the omniattach-backends 0.1 API from the preceding delivery.
//! Sibling sources are required to compile/test this optional module.
use crate::*;
use omniattach_backends::{Backend, BackendContext, Runner};
use std::{path::PathBuf, sync::Arc};

pub struct ExternalAdapter<B> {
    backend: B,
    runner: Arc<dyn Runner>,
}
impl<B> ExternalAdapter<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            runner: Arc::new(omniattach_backends::ProcessRunner),
        }
    }
    pub fn with_runner(mut self, runner: Arc<dyn Runner>) -> Self {
        self.runner = runner;
        self
    }
}
impl<B: Backend + 'static> Converter for ExternalAdapter<B> {
    fn convert(
        &self,
        input: &StagedInput,
        request: &ConversionRequest,
        job: &JobContext,
    ) -> Result<ConversionData> {
        job.check()?;
        if !job.policy().allow_external {
            return Err(Error::ExternalDisabled);
        }
        if request.options != serde_json::json!({}) && request.options != serde_json::Value::Null {
            return Err(Error::Configuration("external adapter options belong in its configured backend instance, not request.options".into()));
        }
        let mut context = BackendContext::new(self.runner.as_ref());
        context.run_options.cancellation = job.cancellation().clone();
        if let Some(remaining) = job.remaining() {
            context.run_options.timeout = context.run_options.timeout.min(remaining);
        }
        context.limits.input_bytes = job.policy().max_input_bytes;
        context.limits.artifact_bytes = job.policy().max_export_bytes;
        context.limits.artifact_count = job.policy().max_export_files;
        context.scratch_parent = job.policy().scratch_parent.clone();
        let converted = self.backend.convert(&input.path, &context)?;
        job.check()?;
        if converted.manifest.assets.len() > job.policy().max_export_files {
            return Err(Error::Limit("backend artifact count".into()));
        }
        let mut data = ConversionData::default();
        for text in &converted.manifest.text {
            data.text.push(TextFragment {
                text: text.text.clone(),
                location: serde_json::to_value(&text.location)?,
            });
        }
        for warning in &converted.manifest.diagnostics {
            data.warn(&warning.code, &warning.message);
        }
        let mut remaining = job.policy().max_export_bytes;
        for (index, artifact) in converted.manifest.assets.iter().enumerate() {
            job.check()?;
            let source = converted.asset_path(index)?;
            let safe_extension = source
                .extension()
                .and_then(|s| s.to_str())
                .filter(|s| s.len() <= 16 && s.bytes().all(|c| c.is_ascii_alphanumeric()));
            let name = safe_extension
                .map(|e| format!("backend-{index:06}.{e}"))
                .unwrap_or_else(|| format!("backend-{index:06}"));
            let bytes = crate::bundle::copy_checked(
                &source,
                &input.output_dir.join(&name),
                remaining,
                job,
            )?;
            remaining -= bytes;
            data.artifacts.push(
                PendingArtifact::new(&name, &artifact.media_type)
                    .located(serde_json::to_value(&artifact.location)?),
            );
        }
        data = data.payload("omniattach-backends/0.1", &converted.manifest)?;
        data.warn("BACKEND_SOURCE_IS_STAGED","Facade source identity refers to the original request; nested backend source.name identifies the staged copy");
        data.warn("BACKEND_ASSET_PATHS_REMAPPED","Nested backend manifest paths describe the original backend bundle and are not export paths; use facade artifacts in matching index order");
        // The backend bundle may now drop; copies in input.output_dir remain owned by facade result.
        Ok(data)
    }
}
impl OmniAttachBuilder {
    /// Adds installed-tool contracts, WITHOUT probing or starting any tool.
    /// Model-dependent ASR registration is deliberately separate.
    pub fn with_external_defaults(self) -> Result<Self> {
        use omniattach_backends::*;
        let b = self.register(
            Route::new(
                "poppler",
                vec![Format::Pdf],
                vec![Goal::Extract],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(Poppler::default()),
        )?;
        let b = b.register(
            Route::new(
                "tesseract",
                vec![Format::Image],
                vec![Goal::Ocr],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(Tesseract::default()),
        )?;
        let b = b.register(
            Route::new(
                "ffmpeg-audio",
                vec![Format::Audio, Format::Video],
                vec![Goal::Normalize],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(FfmpegAudio::default()),
        )?;
        let b = b.register(
            Route::new(
                "ffmpeg-frames",
                vec![Format::Video],
                vec![Goal::VideoFrames],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(FfmpegFrames::default()),
        )?;
        let b = b.register(
            Route::new(
                "ffprobe",
                vec![Format::Audio, Format::Video],
                vec![Goal::MediaProbe],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(Ffprobe::default()),
        )?;
        let b = b.register(
            Route::new(
                "office-to-docx",
                vec![Format::Doc, Format::Rtf],
                vec![Goal::Normalize],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(LibreOffice::new(OfficeFormat::Docx)),
        )?;
        let b = b.register(
            Route::new(
                "office-to-pptx",
                vec![Format::Ppt],
                vec![Goal::Normalize],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(LibreOffice::new(OfficeFormat::Pptx)),
        )?;
        b.register(
            Route::new(
                "office-to-xlsx",
                vec![Format::Xls],
                vec![Goal::Normalize],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(LibreOffice::new(OfficeFormat::Xlsx)),
        )
    }
    pub fn with_whisper_cpp(
        self,
        model: impl Into<PathBuf>,
        language: impl Into<String>,
    ) -> Result<Self> {
        let mut backend = omniattach_backends::WhisperCpp::new(model);
        backend.language = language.into();
        self.register(
            Route::new(
                "whisper-cpp",
                vec![Format::Audio, Format::Video],
                vec![Goal::Transcribe],
                ExecutionKind::External,
            ),
            ExternalAdapter::new(backend),
        )
    }
}

#[cfg(feature = "documents")]
impl OmniAttachBuilder {
    /// Legacy Office extraction: LibreOffice converts to OOXML, then omniattach-documents extracts it.
    /// Registration does not probe LibreOffice.
    pub fn with_legacy_office_pipelines(self) -> Result<Self> {
        self.with_legacy_office_pipelines_runner(Arc::new(omniattach_backends::ProcessRunner))
    }

    /// Registers legacy Office extraction pipelines with an injected [`Runner`].
    pub fn with_legacy_office_pipelines_runner(self, runner: Arc<dyn Runner>) -> Result<Self> {
        use omniattach_backends::{LibreOffice, OfficeFormat};
        let goals = vec![Goal::Extract, Goal::Markdown, Goal::Tables];
        let b = self.register(
            Route::new(
                "legacy-doc-rtf",
                vec![Format::Doc, Format::Rtf],
                goals.clone(),
                ExecutionKind::External,
            )
            .described("LibreOffice .doc/.rtf to DOCX, then omniattach-documents"),
            ArtifactPipeline::new(
                ExternalAdapter::new(LibreOffice::new(OfficeFormat::Docx))
                    .with_runner(runner.clone()),
                crate::documents_defaults::DocumentConverter::new(),
                Goal::Normalize,
                Format::Docx,
            ),
        )?;
        b.register(
            Route::new(
                "legacy-ppt",
                vec![Format::Ppt],
                goals,
                ExecutionKind::External,
            )
            .described("LibreOffice .ppt to PPTX, then omniattach-documents"),
            ArtifactPipeline::new(
                ExternalAdapter::new(LibreOffice::new(OfficeFormat::Pptx)).with_runner(runner),
                crate::documents_defaults::DocumentConverter::new(),
                Goal::Normalize,
                Format::Pptx,
            ),
        )
    }
}
