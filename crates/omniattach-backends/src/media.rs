use crate::{
    ArtifactBundle, AssetKind, Backend, BackendContext, Capability, Error, Location, Manifest,
    Result, Tool, parse_frame_times, workspace::Workspace,
};
use std::path::Path;

fn input(tool: &Tool, ws: &Workspace, verbose: bool) -> crate::CommandSpec {
    tool.command(ws.root())
        .args([
            "-hide_banner",
            "-nostdin",
            "-y",
            "-loglevel",
            if verbose { "info" } else { "error" },
            "-protocol_whitelist",
            "file,pipe",
            "-i",
        ])
        .arg(ws.input.as_os_str())
}

#[derive(Debug, Clone)]
pub struct FfmpegAudio {
    pub tool: Tool,
}
impl Default for FfmpegAudio {
    fn default() -> Self {
        Self {
            tool: Tool::new("ffmpeg"),
        }
    }
}
impl FfmpegAudio {
    pub(crate) fn prepare(
        &self,
        ws: &Workspace,
        ctx: &BackendContext<'_>,
        manifest: &mut Manifest,
    ) -> Result<()> {
        let command = input(&self.tool, ws, false).args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
            "audio.wav",
        ]);
        ctx.run(&command, manifest)?;
        let bytes = ws.read("audio.wav", &ctx.limits)?;
        if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(Error::InvalidOutput(
                "FFmpeg did not produce a WAV file".into(),
            ));
        }
        Ok(())
    }
}
impl Backend for FfmpegAudio {
    fn id(&self) -> &str {
        "ffmpeg-audio"
    }
    fn capability(&self) -> Capability {
        Capability::Audio
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        self.prepare(&ws, ctx, &mut manifest)?;
        manifest.assets.push(ws.asset(
            "audio.wav",
            "audio/wav",
            AssetKind::Audio,
            Location::default(),
            &ctx.limits,
        )?);
        manifest.metadata = serde_json::json!({"sample_rate":16000, "channels":1, "sample_format":"s16le", "audio_stream":"0:a:0"});
        ws.finish(manifest, &ctx.limits)
    }
}

#[derive(Debug, Clone)]
pub struct Ffprobe {
    pub tool: Tool,
}
impl Default for Ffprobe {
    fn default() -> Self {
        Self {
            tool: Tool::new("ffprobe"),
        }
    }
}
impl Backend for Ffprobe {
    fn id(&self) -> &str {
        "ffprobe"
    }
    fn capability(&self) -> Capability {
        Capability::MediaProbe
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let command = self
            .tool
            .command(ws.root())
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(ws.input.as_os_str());
        let output = ctx.run(&command, &mut manifest)?;
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if !metadata.is_object() {
            return Err(Error::InvalidOutput(
                "ffprobe metadata must be an object".into(),
            ));
        }
        manifest.metadata = metadata;
        ws.finish(manifest, &ctx.limits)
    }
}

#[derive(Debug, Clone)]
pub enum FrameSelection {
    /// First frame plus cuts passing FFmpeg's native scene score threshold.
    SceneCuts { threshold: f64 },
    /// First frame plus frames at least interval_ms apart (uses decoded PTS, no assumed FPS).
    Interval { interval_ms: u64 },
}
#[derive(Debug, Clone)]
pub struct FfmpegFrames {
    pub tool: Tool,
    pub selection: FrameSelection,
    pub max_frames: usize,
    pub width: u32,
}
impl Default for FfmpegFrames {
    fn default() -> Self {
        Self {
            tool: Tool::new("ffmpeg"),
            selection: FrameSelection::SceneCuts { threshold: 0.35 },
            max_frames: 1000,
            width: 1280,
        }
    }
}
impl Backend for FfmpegFrames {
    fn id(&self) -> &str {
        "ffmpeg-frames"
    }
    fn capability(&self) -> Capability {
        Capability::VideoFrames
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        if self.max_frames == 0 || self.max_frames > ctx.limits.artifact_count || self.width == 0 {
            return Err(Error::Configuration(
                "positive bounded frame count and width required".into(),
            ));
        }
        let select = match self.selection {
            FrameSelection::SceneCuts { threshold } => {
                if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
                    return Err(Error::Configuration("scene threshold must be 0..1".into()));
                }
                format!("eq(n\\,0)+gt(scene\\,{threshold})")
            }
            FrameSelection::Interval { interval_ms } => {
                if interval_ms == 0 {
                    return Err(Error::Configuration(
                        "positive frame interval required".into(),
                    ));
                }
                format!(
                    "isnan(prev_selected_t)+gte(t-prev_selected_t\\,{})",
                    interval_ms as f64 / 1000.0
                )
            }
        };
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        std::fs::create_dir(ws.root().join("frames"))?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let filter = format!(
            "setpts=PTS-STARTPTS,select={select},scale=w='min({},iw)':h=-2,showinfo",
            self.width
        );
        let command = input(&self.tool, &ws, true).args([
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            &filter,
            "-fps_mode",
            "vfr",
            "-frames:v",
            &self.max_frames.to_string(),
            "-threads",
            "1",
            "frames/frame-%06d.png",
        ]);
        let output = ctx.run(&command, &mut manifest)?;
        let times = parse_frame_times(output.stderr_text()?)?;
        let entries = std::fs::read_dir(ws.root().join("frames"))?.count();
        if entries == 0 {
            return Err(Error::InvalidOutput("no video frames produced".into()));
        }
        if times.len() < entries {
            return Err(Error::InvalidOutput(
                "frame/image timestamp count mismatch".into(),
            ));
        }
        if entries > self.max_frames {
            return Err(Error::InvalidOutput(
                "unexpected number of video frames".into(),
            ));
        }
        for (index, time) in times.iter().take(entries).enumerate() {
            let path = format!("frames/frame-{:06}.png", index + 1);
            let bytes = ws.read(&path, &ctx.limits)?;
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(Error::InvalidOutput("invalid frame PNG signature".into()));
            }
            manifest.assets.push(ws.asset(
                &path,
                "image/png",
                AssetKind::Frame,
                Location {
                    start_ms: Some(*time),
                    ..Default::default()
                },
                &ctx.limits,
            )?);
        }
        if times.len() > entries {
            manifest.warn(
                "FILTER_LOOKAHEAD",
                "FFmpeg evaluated extra selected frames beyond encoded output limit",
            );
        }
        if entries == self.max_frames {
            manifest.warn(
                "FRAME_LIMIT_REACHED",
                "Frame cap reached; later scenes may be omitted",
            );
        }
        manifest.warn("HEURISTIC_FRAMES", "Selected-frame timestamps are normalized relative to the first decoded video frame; scene boundaries/semantic completeness are not guaranteed");
        manifest.metadata = serde_json::json!({"timestamp_origin":"first_decoded_video_frame", "frame_count":entries, "selection":match self.selection { FrameSelection::SceneCuts{..}=>"scene_cuts", FrameSelection::Interval{..}=>"interval" }});
        ws.finish(manifest, &ctx.limits)
    }
}
