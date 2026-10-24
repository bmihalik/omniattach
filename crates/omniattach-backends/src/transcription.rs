use crate::{
    ArtifactBundle, AssetKind, Backend, BackendContext, Capability, FfmpegAudio, Location,
    Manifest, Result, Tool, TranscriptFormat, parse_transcript, workspace::Workspace,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WhisperCpp {
    pub tool: Tool,
    pub audio: FfmpegAudio,
    pub model: PathBuf,
    pub language: String,
    pub keep_audio: bool,
}
impl WhisperCpp {
    pub fn new(model: impl Into<PathBuf>) -> Self {
        Self {
            tool: Tool::new("whisper-cli"),
            audio: FfmpegAudio::default(),
            model: model.into(),
            language: "auto".into(),
            keep_audio: false,
        }
    }
}
impl Backend for WhisperCpp {
    fn id(&self) -> &str {
        "whisper-cpp"
    }
    fn capability(&self) -> Capability {
        Capability::Transcription
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let model = self.model.canonicalize()?;
        if !model.is_file() {
            return Err(crate::Error::InvalidInput("model is not a file".into()));
        }
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        self.audio.prepare(&ws, ctx, &mut manifest)?;
        let command = self
            .tool
            .command(ws.root())
            .args([
                "-f",
                "audio.wav",
                "--output-json",
                "--output-file",
                "transcript",
                "-l",
                &self.language,
                "-m",
            ])
            .arg(model);
        ctx.run(&command, &mut manifest)?;
        manifest.text = parse_transcript(
            &ws.read("transcript.json", &ctx.limits)?,
            TranscriptFormat::WhisperCpp,
        )?;
        manifest.assets.push(ws.asset(
            "transcript.json",
            "application/json",
            AssetKind::Attachment,
            Location::default(),
            &ctx.limits,
        )?);
        if self.keep_audio {
            manifest.assets.push(ws.asset(
                "audio.wav",
                "audio/wav",
                AssetKind::Audio,
                Location::default(),
                &ctx.limits,
            )?);
        }
        if manifest.text.is_empty() {
            manifest.warn(
                "NO_SPEECH",
                "No nonempty timestamped speech segments returned",
            );
        }
        manifest.metadata = serde_json::json!({"language_requested":self.language, "timestamp_origin":"normalized_audio_start", "model_path":self.model.to_string_lossy()});
        ws.finish(manifest, &ctx.limits)
    }
}
