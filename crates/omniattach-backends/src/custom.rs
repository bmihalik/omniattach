use crate::{
    ArtifactBundle, AssetKind, Backend, BackendContext, Capability, Error, Location, Manifest,
    Result, TextKind, TextUnit, Tool, TranscriptFormat, parse_transcript, workspace::Workspace,
};
use std::{ffi::OsString, path::Path};

#[derive(Debug, Clone)]
pub enum CustomOutput {
    MarkdownStdout,
    MarkdownFile {
        path: String,
    },
    TranscriptStdout {
        format: TranscriptFormat,
    },
    TranscriptFile {
        path: String,
        format: TranscriptFormat,
    },
    File {
        path: String,
        media_type: String,
    },
}

/// Explicit wrapper contract; no assumptions about changing third-party CLI layouts.
#[derive(Debug, Clone)]
pub struct CustomBackend {
    pub name: String,
    pub tool: Tool,
    /// Exact-token substitutions only: {input}, {output_dir}, {work_dir}.
    pub args: Vec<String>,
    pub output: CustomOutput,
}
impl Backend for CustomBackend {
    fn id(&self) -> &str {
        &self.name
    }
    fn capability(&self) -> Capability {
        match self.output {
            CustomOutput::TranscriptStdout { .. } | CustomOutput::TranscriptFile { .. } => {
                Capability::Transcription
            }
            CustomOutput::File { .. } => Capability::FileConversion,
            _ => Capability::MarkdownConversion,
        }
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        std::fs::create_dir(ws.root().join("out"))?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let mut command = self.tool.command(ws.root());
        for arg in &self.args {
            let value = match arg.as_str() {
                "{input}" => ws.input.as_os_str().to_owned(),
                "{output_dir}" => ws.root().join("out").into_os_string(),
                "{work_dir}" => ws.root().as_os_str().to_owned(),
                _ => OsString::from(arg),
            };
            command.args.push(value);
        }
        let output = ctx.run(&command, &mut manifest)?;
        match &self.output {
            CustomOutput::MarkdownStdout => add_markdown(&mut manifest, output.stdout_text()?)?,
            CustomOutput::MarkdownFile { path } => {
                let bytes = ws.read(path, &ctx.limits)?;
                add_markdown(
                    &mut manifest,
                    std::str::from_utf8(&bytes).map_err(|e| Error::InvalidOutput(e.to_string()))?,
                )?;
                manifest.assets.push(ws.asset(
                    path,
                    "text/markdown",
                    AssetKind::Attachment,
                    Location::default(),
                    &ctx.limits,
                )?);
            }
            CustomOutput::TranscriptStdout { format } => {
                manifest.text = parse_transcript(&output.stdout, *format)?
            }
            CustomOutput::TranscriptFile { path, format } => {
                manifest.text = parse_transcript(&ws.read(path, &ctx.limits)?, *format)?;
                manifest.assets.push(ws.asset(
                    path,
                    "application/json",
                    AssetKind::Attachment,
                    Location::default(),
                    &ctx.limits,
                )?);
            }
            CustomOutput::File { path, media_type } => manifest.assets.push(ws.asset(
                path,
                media_type,
                AssetKind::Attachment,
                Location::default(),
                &ctx.limits,
            )?),
        }
        manifest.warn("EXPLICIT_CUSTOM_CONTRACT", "Custom command configuration is trusted code; only declared output is retained, relative Markdown images are not copied automatically");
        ws.finish(manifest, &ctx.limits)
    }
}
fn add_markdown(manifest: &mut Manifest, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::InvalidOutput("empty Markdown output".into()));
    }
    manifest.text.push(TextUnit {
        kind: TextKind::Markdown,
        text: text.into(),
        location: Location::default(),
        confidence: None,
    });
    manifest.warn(
        "NO_SOURCE_LAYOUT",
        "Generated Markdown does not provide authoritative original page or region provenance",
    );
    Ok(())
}
