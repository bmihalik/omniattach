use crate::{CliError, Result, Runtime, args::*};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Small transport boundary; production FacadeEngine delegates conversion to omniattach.
pub trait Engine {
    fn capabilities(&self) -> Result<Value>;
    fn plan(&self, request: &EngineRequest) -> Result<Value>;
    fn convert(
        &self,
        request: &EngineRequest,
        output: &Path,
        progress: ProgressMode,
    ) -> Result<ConversionOutcome>;
}
#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub input: PathBuf,
    pub goal: String,
    pub format: Option<String>,
    pub route: Option<String>,
    pub options: Value,
    pub allow_external: bool,
    pub max_input_bytes: u64,
    pub max_export_bytes: u64,
    pub max_export_files: usize,
    pub timeout_ms: Option<u64>,
    pub scratch_parent: Option<PathBuf>,
}
#[derive(Debug)]
pub struct ConversionOutcome {
    pub manifest: Value,
    pub text: String,
}
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub external_backends: bool,
    pub whisper_model: Option<PathBuf>,
    pub language: String,
}
impl EngineConfig {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        if cli.whisper_model.is_some() && !cli.external_backends {
            return Err(CliError::usage(
                "--whisper-model requires --external-backends",
            ));
        }
        if cli.language != "auto" && cli.whisper_model.is_none() {
            return Err(CliError::usage("--language requires --whisper-model"));
        }
        Ok(Self {
            external_backends: cli.external_backends,
            whisper_model: cli.whisper_model.clone(),
            language: cli.language.clone(),
        })
    }
}
fn options(args: &RequestArgs) -> Result<Value> {
    const LIMIT: u64 = 1048576;
    let raw = if let Some(path) = &args.options_file {
        if !std::fs::metadata(path)?.is_file() {
            return Err(CliError::usage("options-file is not a regular file"));
        }
        let file = std::fs::File::open(path)?;
        let mut data = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut data)?;
        if data.len() as u64 > LIMIT {
            return Err(CliError::limit("options JSON is over 1 MiB"));
        }
        data
    } else if let Some(json) = &args.options_json {
        if json.len() as u64 > LIMIT {
            return Err(CliError::limit("options JSON is over 1 MiB"));
        }
        json.as_bytes().to_vec()
    } else {
        return Ok(json!({}));
    };
    let value: Value = serde_json::from_slice(&raw)?;
    if !value.is_object() {
        return Err(CliError::usage("options JSON must be an object"));
    }
    Ok(value)
}
pub fn make_request(args: &RequestArgs) -> Result<EngineRequest> {
    validate_goal(&args.goal)?;
    if let Some(format) = &args.format {
        validate_format(format)?;
    }
    if args.route.as_ref().is_some_and(|s| s.trim().is_empty()) {
        return Err(CliError::usage("route must not be empty"));
    }
    Ok(EngineRequest {
        input: args.input.clone(),
        goal: args.goal.clone(),
        format: args.format.clone(),
        route: args.route.clone(),
        options: options(args)?,
        allow_external: args.allow_external,
        max_input_bytes: args.max_input_bytes,
        max_export_bytes: args.max_export_bytes,
        max_export_files: args.max_export_files,
        timeout_ms: args.timeout_ms,
        scratch_parent: args.scratch_parent.clone(),
    })
}
pub fn validate_goal(goal: &str) -> Result<()> {
    if matches!(
        goal,
        "extract"
            | "markdown"
            | "tables"
            | "pdf-layout"
            | "pdf-scientific"
            | "ocr"
            | "transcribe"
            | "video-frames"
            | "media-probe"
            | "normalize"
    ) || goal
        .strip_prefix("custom:")
        .is_some_and(|s| !s.trim().is_empty())
    {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "unknown goal {goal:?}; use extract, markdown, tables, pdf-layout, pdf-scientific, ocr, transcribe, video-frames, media-probe, normalize, custom:NAME"
        )))
    }
}
pub fn validate_format(format: &str) -> Result<()> {
    if matches!(
        format,
        "text"
            | "markdown"
            | "html"
            | "xml"
            | "csv"
            | "tsv"
            | "json"
            | "json-lines"
            | "json_lines"
            | "pdf"
            | "docx"
            | "pptx"
            | "xlsx"
            | "odt"
            | "odp"
            | "ods"
            | "epub"
            | "mobi"
            | "azw3"
            | "doc"
            | "ppt"
            | "xls"
            | "rtf"
            | "audio"
            | "video"
            | "image"
            | "zip"
    ) {
        Ok(())
    } else {
        Err(CliError::usage(format!("unknown format {format:?}")))
    }
}
fn ensure_destination(path: &Path, input: &Path) -> Result<()> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(CliError::usage(
            "output directory already exists; choose a new destination",
        ));
    }
    if path.file_name().is_none() {
        return Err(CliError::usage("output must name a new directory"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize()?;
    if !parent.is_dir() {
        return Err(CliError::usage("output parent is not a directory"));
    }
    if let Ok(input) = input.canonicalize()
        && parent.join(path.file_name().unwrap()) == input
    {
        return Err(CliError::usage("output cannot replace input"));
    }
    Ok(())
}
pub fn write_json(out: &mut dyn Write, value: &Value, pretty: bool) -> Result<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut *out, value)?;
    } else {
        serde_json::to_writer(&mut *out, value)?;
    }
    out.write_all(b"\n")?;
    Ok(())
}
pub fn dispatch(
    cli: &Cli,
    engine: &dyn Engine,
    runtime: &Runtime,
    stdout: &mut dyn Write,
) -> Result<()> {
    runtime.check()?;
    let _ = EngineConfig::from_cli(cli)?;
    match &cli.command {
        Command::Capabilities => write_json(
            stdout,
            &json!({"schema":"omniattach-cli/capabilities/0.1","build":crate::error::BuildSupport::default(),"commands":["capabilities","probe","xml-probe","plan","convert"],"registered":engine.capabilities()?,"health_checked":false}),
            cli.pretty,
        ),
        Command::Probe(args) => write_json(
            stdout,
            &crate::probe::probe(&args.input, args.max_input_bytes, runtime)?,
            cli.pretty,
        ),
        Command::XmlProbe(args) => write_json(
            stdout,
            &serde_json::to_value(crate::xml::xml_probe(args, runtime)?)?,
            cli.pretty,
        ),
        Command::Plan(args) => {
            let request = make_request(&args.request)?;
            runtime.check()?;
            write_json(
                stdout,
                &json!({"schema":"omniattach-cli/plan/0.1","plan":engine.plan(&request)?}),
                cli.pretty,
            )
        }
        Command::Convert(args) => {
            let request = make_request(&args.request)?;
            ensure_destination(&args.output, &request.input)?;
            runtime.check()?;
            let result = engine.convert(&request, &args.output, args.progress)?;
            // A late signal does not invalidate a completed durable export.
            match args.stdout {
                StdoutMode::None => Ok(()),
                StdoutMode::Text => {
                    stdout.write_all(result.text.as_bytes())?;
                    Ok(())
                }
                StdoutMode::Manifest => write_json(
                    stdout,
                    &json!({"schema":"omniattach-cli/result/0.1","output_dir":args.output.canonicalize()?,"manifest_file":args.output.canonicalize()?.join("manifest.json"),"manifest":result.manifest}),
                    cli.pretty,
                ),
            }
        }
    }
}
/// Without facade, inspection is still usable. Conversion is explicitly unavailable.
pub struct InspectionOnlyEngine;
impl Engine for InspectionOnlyEngine {
    fn capabilities(&self) -> Result<Value> {
        Ok(json!({"routes":[],"conversion_enabled":false,"reason":"built without facade feature"}))
    }
    fn plan(&self, _: &EngineRequest) -> Result<Value> {
        Err(CliError::unsupported(
            "plan requires the facade Cargo feature and the actual sibling omniattach crate",
        ))
    }
    fn convert(&self, _: &EngineRequest, _: &Path, _: ProgressMode) -> Result<ConversionOutcome> {
        Err(CliError::unsupported(
            "convert requires the facade Cargo feature and the actual sibling omniattach crate",
        ))
    }
}
