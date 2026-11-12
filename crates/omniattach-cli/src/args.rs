use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
#[derive(Debug, Parser)]
#[command(
    name = "omniattach",
    version,
    about = "File conversion with explicit routes, durable exports and JSON subprocess output"
)]
pub struct Cli {
    /// Pretty-print successful JSON.
    #[arg(long, global = true)]
    pub pretty: bool,
    /// Register external routes; does not permit execution.
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
    /// Describe compiled support and registered routes, not tool health.
    Capabilities,
    /// Inspect signatures/extensions without conversion.
    Probe(ProbeArgs),
    /// Inspect XML structure without text or attribute values.
    XmlProbe(XmlArgs),
    /// Resolve an advisory conversion route.
    Plan(PlanArgs),
    /// Convert and export a new durable directory.
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
    /// Explicit format override.
    #[arg(long)]
    pub format: Option<String>,
    #[arg(long)]
    pub route: Option<String>,
    #[arg(long, conflicts_with = "options_file")]
    pub options_json: Option<String>,
    #[arg(long, conflicts_with = "options_json")]
    pub options_file: Option<PathBuf>,
    /// Document-route joined-word token estimate, 0..=64; 0 disables (default 6).
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=64))]
    pub dehyphenate_max_tokens: Option<u8>,
    /// Permit execution of registered external routes.
    #[arg(long)]
    pub allow_external: bool,
    #[arg(long, default_value_t = 536870912)]
    pub max_input_bytes: u64,
    #[arg(long, default_value_t = 536870912)]
    pub max_export_bytes: u64,
    #[arg(long, default_value_t = 10000)]
    pub max_export_files: usize,
    /// Cooperative deadline; zero means immediate.
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
    /// NEW output directory; parent must exist.
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long,value_enum,default_value_t=StdoutMode::Manifest)]
    pub stdout: StdoutMode,
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
