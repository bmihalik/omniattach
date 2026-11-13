//! Explicit MinerU/Marker command contracts and relocatable output packages.
use crate::{
    ArtifactBundle, AssetKind, Backend, BackendContext, Capability, Error, Location, Manifest,
    Result, TextKind, TextUnit, Tool, workspace::Workspace,
};
use pulldown_cmark::{Event, Parser, Tag};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Paths are relative to the converter's out/ directory. None requires exactly one Markdown file.
#[derive(Debug, Clone, Default)]
pub struct LayoutOutputConfig {
    pub markdown_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinerUProfile {
    /// Python with the documented mineru.parser parse/save API installed.
    SdkSave,
    /// Historical mineru -p INPUT -o OUT -b pipeline command contract.
    LegacyPipeline,
}
#[derive(Debug, Clone)]
pub struct MinerU {
    pub tool: Tool,
    pub profile: MinerUProfile,
    pub output: LayoutOutputConfig,
}
impl MinerU {
    pub fn new(profile: MinerUProfile) -> Self {
        Self {
            tool: Tool::new(match profile {
                MinerUProfile::SdkSave => "python3",
                MinerUProfile::LegacyPipeline => "mineru",
            }),
            profile,
            output: LayoutOutputConfig::default(),
        }
    }
}
impl Default for MinerU {
    fn default() -> Self {
        Self::new(MinerUProfile::SdkSave)
    }
}
const MINERU_SAVE: &str = "import sys\nfrom mineru.parser import parse\nfrom mineru.parser.writer import FileBasedDataWriter\nparse(sys.argv[1]).save(FileBasedDataWriter(sys.argv[2]))\n";
impl Backend for MinerU {
    fn id(&self) -> &str {
        "mineru"
    }
    fn capability(&self) -> Capability {
        Capability::MarkdownConversion
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = prepare(source, ctx)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let out = ws.root().join("out");
        let (command, profile) = match self.profile {
            MinerUProfile::SdkSave => (
                self.tool
                    .command(ws.root())
                    .args(["-c", MINERU_SAVE])
                    .arg(ws.input.as_os_str())
                    .arg(out.as_os_str()),
                "mineru-sdk-save",
            ),
            MinerUProfile::LegacyPipeline => (
                self.tool
                    .command(ws.root())
                    .arg("-p")
                    .arg(ws.input.as_os_str())
                    .arg("-o")
                    .arg(out.as_os_str())
                    .args(["-b", "pipeline"]),
                "mineru-legacy-pipeline",
            ),
        };
        ctx.run(&command, &mut manifest)?;
        package(ws, manifest, &self.output, profile, ctx)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerMode {
    Fast,
    Balanced,
}
#[derive(Debug, Clone)]
pub struct Marker {
    pub tool: Tool,
    pub mode: Option<MarkerMode>,
    pub force_ocr: bool,
    pub output: LayoutOutputConfig,
}
impl Default for Marker {
    fn default() -> Self {
        Self {
            tool: Tool::new("marker_single"),
            mode: None,
            force_ocr: false,
            output: LayoutOutputConfig::default(),
        }
    }
}
impl Backend for Marker {
    fn id(&self) -> &str {
        "marker"
    }
    fn capability(&self) -> Capability {
        Capability::MarkdownConversion
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = prepare(source, ctx)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let mut command = self
            .tool
            .command(ws.root())
            .arg(ws.input.as_os_str())
            .arg("--output_dir")
            .arg(ws.root().join("out").as_os_str())
            .args(["--output_format", "markdown"]);
        if let Some(mode) = self.mode {
            command = command.args([
                "--mode",
                match mode {
                    MarkerMode::Fast => "fast",
                    MarkerMode::Balanced => "balanced",
                },
            ]);
        }
        if self.force_ocr {
            command = command.arg("--force_ocr");
        }
        ctx.run(&command, &mut manifest)?;
        package(ws, manifest, &self.output, "marker-single-markdown", ctx)
    }
}
fn check(ctx: &BackendContext<'_>) -> Result<()> {
    if ctx.run_options.cancellation.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn prepare(source: &Path, ctx: &BackendContext<'_>) -> Result<Workspace> {
    check(ctx)?;
    let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
    let mut prefix = Vec::new();
    fs::File::open(&ws.input)?
        .take(5)
        .read_to_end(&mut prefix)?;
    if prefix != b"%PDF-" {
        return Err(Error::InvalidInput(
            "layout backends currently accept PDF input only".into(),
        ));
    }
    fs::create_dir(ws.root().join("out"))?;
    Ok(ws)
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn media_type(path: &str) -> Option<&'static str> {
    match Path::new(path)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "markdown" => Some("text/markdown"),
        "json" => Some("application/json"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "jp2" => Some("image/jp2"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}
#[derive(Default)]
struct Scan {
    files: BTreeMap<String, u64>,
    entries: usize,
    bytes: u64,
    ignored: usize,
}
fn scan(
    ws: &Workspace,
    relative: &str,
    depth: usize,
    result: &mut Scan,
    ctx: &BackendContext<'_>,
) -> Result<()> {
    check(ctx)?;
    if depth > 32 {
        return Err(Error::Limit("layout output depth".into()));
    }
    let dir = ws.root().join(relative);
    let meta = fs::symlink_metadata(&dir)?;
    if meta.file_type().is_symlink()
        || !meta.is_dir()
        || !dir.canonicalize()?.starts_with(ws.root().canonicalize()?)
    {
        return Err(Error::InvalidOutput(
            "layout output directory is not contained or is a symlink".into(),
        ));
    }
    for entry in fs::read_dir(&dir)? {
        check(ctx)?;
        result.entries = result.entries.saturating_add(1);
        if result.entries
            > ctx
                .limits
                .artifact_count
                .saturating_mul(4)
                .saturating_add(32)
        {
            return Err(Error::Limit("layout output scan entries".into()));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::InvalidOutput("non-UTF-8 layout filename".into()))?;
        if !safe_relative(&name) {
            return Err(Error::InvalidOutput("unsafe layout filename".into()));
        }
        let path = format!("{relative}/{name}");
        let meta = fs::symlink_metadata(ws.root().join(&path))?;
        if meta.file_type().is_symlink() {
            return Err(Error::InvalidOutput("symlink layout artifact".into()));
        }
        if meta.is_dir() {
            scan(ws, &path, depth + 1, result, ctx)?;
        } else if meta.is_file() {
            let key = path
                .strip_prefix("out/")
                .ok_or_else(|| Error::InvalidOutput("invalid layout root".into()))?;
            if media_type(key).is_some() {
                result.bytes = result
                    .bytes
                    .checked_add(meta.len())
                    .ok_or_else(|| Error::Limit("layout artifact bytes overflow".into()))?;
                if result.bytes > ctx.limits.artifact_bytes
                    || result.files.len() >= ctx.limits.artifact_count
                {
                    return Err(Error::Limit("layout artifact bytes or count".into()));
                }
                result.files.insert(key.into(), meta.len());
            } else {
                result.ignored += 1;
            }
        } else {
            return Err(Error::InvalidOutput("non-regular layout output".into()));
        }
    }
    Ok(())
}
fn reference(
    dest: &str,
    image: bool,
    document: &str,
    files: &BTreeMap<String, u64>,
) -> Result<usize> {
    if !image && (dest.is_empty() || dest.starts_with('#')) {
        return Ok(0);
    }
    if let Some((scheme, _)) = dest.split_once(':') {
        if !image
            && matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "mailto" | "tel"
            )
        {
            return Ok(1);
        }
        return Err(Error::InvalidOutput(
            "unsupported URI or non-local image reference in layout Markdown".into(),
        ));
    }
    let path = dest.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .map_err(|_| Error::InvalidOutput("invalid encoded layout reference".into()))?;
    if decoded.is_empty() || decoded.contains(['\\', ':', '\0']) {
        return Err(Error::InvalidOutput(
            "invalid local layout reference".into(),
        ));
    }
    let mut resolved = Path::new(document)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    for part in Path::new(decoded.as_ref()).components() {
        match part {
            Component::Normal(name) => resolved.push(name),
            Component::CurDir => {}
            Component::ParentDir if resolved.pop() => {}
            _ => {
                return Err(Error::InvalidOutput(
                    "layout reference escapes output package".into(),
                ));
            }
        }
    }
    let key = resolved
        .to_str()
        .ok_or_else(|| Error::InvalidOutput("invalid layout reference encoding".into()))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    if !files.contains_key(&key)
        || (image && !media_type(&key).is_some_and(|m| m.starts_with("image/")))
    {
        return Err(Error::InvalidOutput(format!(
            "missing or unsupported local layout asset: {key}"
        )));
    }
    Ok(0)
}
fn validate_markdown(
    text: &str,
    document: &str,
    files: &BTreeMap<String, u64>,
    ctx: &BackendContext<'_>,
) -> Result<usize> {
    if text.trim().is_empty() {
        return Err(Error::InvalidOutput("empty layout Markdown".into()));
    }
    let mut remote = 0;
    let images = Selector::parse("img").expect("constant selector");
    let links = Selector::parse("a[href]").expect("constant selector");
    let unsupported = Selector::parse("video, audio, source, iframe, object, embed, script, link")
        .expect("constant selector");
    for event in Parser::new_ext(text, pulldown_cmark::Options::all()) {
        check(ctx)?;
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                remote += reference(&dest_url, true, document, files)?;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                remote += reference(&dest_url, false, document, files)?;
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let fragment = Html::parse_fragment(&html);
                if fragment.select(&unsupported).next().is_some() {
                    return Err(Error::InvalidOutput(
                        "unsupported resource-bearing HTML in layout Markdown".into(),
                    ));
                }
                for img in fragment.select(&images) {
                    if img.value().attr("srcset").is_some() {
                        return Err(Error::InvalidOutput(
                            "HTML srcset is not supported by layout packaging".into(),
                        ));
                    }
                    let src = img.value().attr("src").ok_or_else(|| {
                        Error::InvalidOutput("layout HTML image has no src".into())
                    })?;
                    remote += reference(src, true, document, files)?;
                }
                for link in fragment.select(&links) {
                    remote += reference(
                        link.value().attr("href").unwrap_or(""),
                        false,
                        document,
                        files,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(remote)
}
fn package(
    ws: Workspace,
    mut manifest: Manifest,
    config: &LayoutOutputConfig,
    profile: &str,
    ctx: &BackendContext<'_>,
) -> Result<ArtifactBundle> {
    let mut scanned = Scan::default();
    scan(&ws, "out", 0, &mut scanned, ctx)?;
    let markdown = if let Some(path) = &config.markdown_path {
        if !safe_relative(path)
            || media_type(path) != Some("text/markdown")
            || !scanned.files.contains_key(path)
        {
            return Err(Error::InvalidOutput(
                "configured layout Markdown path is missing or unsafe".into(),
            ));
        }
        path.clone()
    } else {
        let candidates: Vec<_> = scanned
            .files
            .keys()
            .filter(|p| media_type(p) == Some("text/markdown"))
            .collect();
        if candidates.len() != 1 {
            return Err(Error::InvalidOutput(format!(
                "expected exactly one layout Markdown file, found {}; configure markdown_path to select explicitly",
                candidates.len()
            )));
        }
        candidates[0].clone()
    };
    fs::create_dir(ws.root().join("result"))?;
    let mut text = String::new();
    let mut hashes = BTreeMap::new();
    let mut remote = 0;
    for (path, length) in &scanned.files {
        check(ctx)?;
        let data = ws.read(&format!("out/{path}"), &ctx.limits)?;
        if data.len() as u64 != *length {
            return Err(Error::InvalidOutput(
                "layout output changed during packaging".into(),
            ));
        }
        let mime = media_type(path)
            .ok_or_else(|| Error::InvalidOutput("unsupported layout artifact".into()))?;
        if mime == "text/markdown" {
            let md = std::str::from_utf8(&data)
                .map_err(|e| Error::InvalidOutput(format!("layout Markdown UTF-8: {e}")))?;
            remote += validate_markdown(md, path, &scanned.files, ctx)?;
            if *path == markdown {
                text = md.to_owned();
            }
        } else if mime == "application/json" {
            let _: serde_json::Value = serde_json::from_slice(&data)
                .map_err(|e| Error::InvalidOutput(format!("invalid layout JSON sidecar: {e}")))?;
        }
        let relative = format!("result/{path}");
        let destination = ws.root().join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        output.write_all(&data)?;
        hashes.insert(relative.clone(), format!("{:x}", Sha256::digest(&data)));
        manifest.assets.push(ws.asset(
            &relative,
            mime,
            AssetKind::Attachment,
            Location::default(),
            &ctx.limits,
        )?);
    }
    manifest.text.push(TextUnit {
        kind: TextKind::Markdown,
        text,
        location: Location::default(),
        confidence: None,
    });
    manifest.metadata = serde_json::json!({"layout_package": {"contract": "omniattach-layout-package/1", "profile": profile, "markdown_asset": format!("result/{markdown}"), "sha256": hashes, "json_sidecars": "opaque producer schemas"}});
    manifest.warn("LAYOUT_MODEL_OUTPUT", "Generated Markdown and producer JSON are retained without invented authoritative page/region provenance; image MIME types follow extensions, not image decoding. Relative references use markdown_asset as their base.");
    if scanned.ignored > 0 {
        manifest.warn(
            "LAYOUT_OUTPUTS_SKIPPED",
            &format!("Skipped {} unsupported/debug output files", scanned.ignored),
        );
    }
    if remote > 0 {
        manifest.warn("LAYOUT_EXTERNAL_LINKS", &format!("Retained {remote} external hyperlinks without fetching them; remote image references are rejected"));
    }
    ws.finish(manifest, &ctx.limits)
}
