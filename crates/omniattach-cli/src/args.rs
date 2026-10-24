use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "omniattach",
    version,
    about = "File conversion with explicit routes, durable exports and JSON subprocess output"
)]
pub struct Cli {
    /// Pretty-print successful JSON; otherwise emit one compact JSON record.
    #[arg(long, global = true)]
    pub pretty: bool,
    /// Register external routes. This does NOT permit process execution.
    #[arg(long, global = true)]
    pub external_backends: bool,
    /// Register whisper-cpp using this model (requires --external-backends).
    #[arg(long, global = true)]
    pub whisper_model: Option<PathBuf>,
    #[arg(long, global = true, default_value = "auto")]
    pub language: String,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Describe compiled support and registered routes, not installed-tool health.
    Capabilities,
    /// Conservative file signature/extension detection; no tool/model execution.
    Probe(ProbeArgs),
    /// Inspect namespace-aware XML paths/counts without text or attribute values.
    XmlProbe(XmlArgs),
    /// Resolve an explicit route without running converters; no output files created.
    Plan(PlanArgs),
    /// Convert one file, export durable artifacts, then emit a machine-readable result.
    Convert(ConvertArgs),
}
#[derive(Debug, Args)]
pub struct ProbeArgs {
    pub input: PathBuf,
    #[arg(long, default_value_t = 536870912)]
    pub max_input_bytes: u64,
}
#[derive(Debug, Args)]
pub struct XmlArgs {
    pub input: PathBuf,
    /// Entire-file byte budget, enforced while reading (not a sample).
    #[arg(long, default_value_t = 16777216)]
    pub max_input_bytes: u64,
    #[arg(long, default_value_t = 128)]
    pub max_depth: usize,
    #[arg(long, default_value_t = 10000)]
    pub max_paths: usize,
    #[arg(long, default_value_t = 100000)]
    pub max_attributes: usize,
    #[arg(long, default_value_t = 30000)]
    pub timeout_ms: u64,
}
#[derive(Debug, Clone, Args)]
pub struct RequestArgs {
    pub input: PathBuf,
    #[arg(long, default_value = "extract")]
    pub goal: String,
    /// Explicit format override, e.g. text, pdf, docx, audio, video.
    #[arg(long)]
    pub format: Option<String>,
    #[arg(long)]
    pub route: Option<String>,
    #[arg(long, conflicts_with = "options_file")]
    pub options_json: Option<String>,
    #[arg(long, conflicts_with = "options_json")]
    pub options_file: Option<PathBuf>,
    /// Explicit permission for routes declared External. Does not register routes.
    #[arg(long)]
    pub allow_external: bool,
    #[arg(long, default_value_t = 536870912)]
    pub max_input_bytes: u64,
    #[arg(long, default_value_t = 536870912)]
    pub max_export_bytes: u64,
    #[arg(long, default_value_t = 10000)]
    pub max_export_files: usize,
    /// Cooperative deadline. Zero means an immediate deadline, not infinity.
    #[arg(long)]
    pub timeout_ms: Option<u64>,
    #[arg(long)]
    pub scratch_parent: Option<PathBuf>,
}
#[derive(Debug, Args)]
pub struct PlanArgs {
    #[command(flatten)]
    pub request: RequestArgs,
}
#[derive(Debug, Args)]
pub struct ConvertArgs {
    #[command(flatten)]
    pub request: RequestArgs,
    /// NEW directory for artifacts and manifest.json. Parent must already exist.
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long,value_enum,default_value_t=StdoutMode::Manifest)]
    pub stdout: StdoutMode,
    /// Emit facade progress as stderr NDJSON; stdout remains result-only.
    #[arg(long,value_enum,default_value_t=ProgressMode::None)]
    pub progress: ProgressMode,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StdoutMode {
    Manifest,
    Text,
    None,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProgressMode {
    None,
    Ndjson,
}
