//! Machine-friendly command-line boundary for the omniattach workspace.
//!
//! `probe` and `xml-probe` do not require conversion backends. The default production
//! build uses the sibling facade. The isolated harness tests only this transport/inspection
//! layer and does not pretend to provide a replacement facade.
//!
//! ```
//! use clap::Parser;
//! use omniattach_cli::args::{Cli,Command};
//! let cli=Cli::try_parse_from(["omniattach","probe","input.pdf"]).unwrap();
//! assert!(matches!(cli.command,Command::Probe(_)));
//! ```
pub mod args;
pub mod error;
#[cfg(feature = "facade")]
pub mod facade;
pub mod probe;
pub mod runtime;
pub mod service;
pub mod xml;
pub use error::{CliError, Result};
pub use runtime::Runtime;

/// Library entry point with caller-provided engine and output writer.
/// Does not install signal handlers; process main owns that global operation.
pub use service::dispatch;
