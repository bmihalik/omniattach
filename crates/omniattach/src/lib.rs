//! Explicit synchronous conversion facade; fallback is opt-in.
#[cfg(feature = "backends")]
mod backend_adapter;
#[cfg(feature = "backends")]
mod layout_adapter;
mod bundle;
mod context;
mod documents_adapter;
#[cfg(feature = "documents")]
mod documents_defaults;
mod engine;
mod error;
mod fallback;
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
pub use fallback::*;
pub use format::*;
pub use pipeline::*;
pub use request::*;
#[cfg(feature = "backends")]
pub use omniattach_backends as backends;
#[cfg(feature = "core")]
pub use omniattach_core as core;
#[cfg(feature = "documents")]
pub use omniattach_documents as documents;
