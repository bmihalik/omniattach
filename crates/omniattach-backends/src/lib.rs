//! Opt-in local external-tool adapters for omniattach.
//!
//! Adapters produce an owned [`ArtifactBundle`], not rendered text masquerading as source data.
//! No tool is started until `convert` or `probe` is called. No tool/model is downloaded.
//!
//! ```no_run
//! use omniattach_backends::{Backend, BackendContext, Poppler, ProcessRunner};
//! let runner = ProcessRunner;
//! let context = BackendContext::new(&runner);
//! let result = Poppler::default().convert(std::path::Path::new("paper.pdf"), &context)?;
//! println!("{}", result.manifest.to_json()?);
//! # Ok::<(), omniattach_backends::Error>(())
//! ```
mod backend;
mod custom;
mod documents;
mod error;
mod media;
mod model;
mod parsers;
mod pdf_ocr;
mod process;
mod transcription;
mod workspace;

pub use backend::*;
pub use custom::*;
pub use documents::*;
pub use error::*;
pub use media::*;
pub use model::*;
pub use parsers::*;
pub use pdf_ocr::*;
pub use process::*;
pub use transcription::*;
pub use workspace::{ArtifactBundle, Limits, Workspace as JobWorkspace};
