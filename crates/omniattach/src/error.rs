use crate::{Format, Goal};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid configuration: {0}")]
    Configuration(String),
    #[error("input format is unknown; specify request.format or use an explicit route")]
    UnknownFormat,
    #[error("no route for {format:?} -> {goal:?}")]
    NoRoute { format: Format, goal: Goal },
    #[error("multiple eligible routes: {0:?}; select request.route explicitly")]
    AmbiguousRoute(Vec<String>),
    #[error("unknown route: {0}")]
    UnknownRoute(String),
    #[error("route {route} does not accept {format:?} -> {goal:?}")]
    RouteMismatch {
        route: String,
        format: Format,
        goal: Goal,
    },
    #[error("external execution is disabled by policy")]
    ExternalDisabled,
    #[error("operation cancelled")]
    Cancelled,
    #[error("conversion deadline exceeded")]
    Deadline,
    #[error("size limit exceeded: {0}")]
    Limit(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid output: {0}")]
    InvalidOutput(String),
    #[error("converter {route}: {message}")]
    Converter { route: String, message: String },
    #[cfg(feature = "backends")]
    #[error(transparent)]
    Backend(#[from] omniattach_backends::Error),
}
