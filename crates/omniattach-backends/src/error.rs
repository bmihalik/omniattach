use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot execute {program}: {source}")]
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
    #[error("process timed out after {0} ms")]
    Timeout(u64),
    #[error("operation cancelled")]
    Cancelled,
    #[error("output limit exceeded: {0}")]
    Limit(String),
    #[error("process failed ({code:?}): {stderr}")]
    ProcessFailed { code: Option<i32>, stderr: String },
    #[error("invalid configuration: {0}")]
    Configuration(String),
    #[error("invalid backend output: {0}")]
    InvalidOutput(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("backend not registered: {0}")]
    UnknownBackend(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}
