//! Explicit synchronous conversion facade; fallback is opt-in.
#[cfg(feature = "backends")]
mod backend_adapter;
mod bundle;
mod context;
mod documents_adapter;
#[cfg(feature = "documents")]
mod documents_defaults;
#[cfg(feature = "documents")]
mod pdf_images_adapter;
mod engine;
mod error;
mod fallback;
mod format;
#[cfg(feature = "backends")]
mod layout_adapter;
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
#[cfg(feature = "backends")]
pub use omniattach_backends as backends;
#[cfg(feature = "core")]
pub use omniattach_core as core;
#[cfg(feature = "documents")]
pub use omniattach_documents as documents;
pub use pipeline::*;
pub use request::*;
