//! Explicit, synchronous conversion facade. No silent fallback and no parser-API guessing.
//!
//! ```no_run
//! use omniattach::{OmniAttach, ConversionRequest, FacadeContext, Goal};
//! let engine = OmniAttach::new();
//! let result = engine.convert(&ConversionRequest::new("notes.txt", Goal::Markdown), &FacadeContext::default())?;
//! result.export("notes-output")?;
//! # Ok::<(), omniattach::Error>(())
//! ```
//!
//! Facade types intentionally differ from unpublished sibling core types. Optional re-exports
//! expose those types in their own namespace rather than claiming binary/schema equivalence.
#[cfg(feature = "backends")]
mod backend_adapter;
mod bundle;
mod context;
mod documents_adapter;
mod engine;
mod error;
mod format;
mod pipeline;
mod portable;
mod request;

#[cfg(feature = "backends")]
pub use backend_adapter::*;
pub use bundle::*;
pub use context::*;
pub use documents_adapter::*;
pub use engine::*;
pub use error::*;
pub use format::*;
pub use pipeline::*;
pub use request::*;

#[cfg(feature = "backends")]
pub use omniattach_backends as backends;
#[cfg(feature = "core")]
pub use omniattach_core as core;
#[cfg(feature = "documents")]
pub use omniattach_documents as documents;
