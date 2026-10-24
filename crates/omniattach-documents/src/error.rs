use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unsupported document format: {0}")]
    UnsupportedFormat(String),
    #[error("format {0} requires a Cargo feature that is disabled")]
    FeatureDisabled(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("resource limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("XML table mapping required; use extract_xml_table or Options::xml_table")]
    MappingRequired,
    #[error("conversion failed: {0}")]
    Parse(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
