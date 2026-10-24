use serde::Serialize;
pub type Result<T> = std::result::Result<T, CliError>;
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub exit: u8,
}
impl CliError {
    pub fn new(code: &'static str, message: impl Into<String>, exit: u8) -> Self {
        Self {
            code,
            message: message.into(),
            exit,
        }
    }
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message, 2)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("UNSUPPORTED", message, 4)
    }
    pub fn input(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message, 3)
    }
    pub fn limit(message: impl Into<String>) -> Self {
        Self::new("LIMIT_EXCEEDED", message, 6)
    }
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({"schema":"omniattach-cli/error/0.1","error":{"code":self.code,"message":self.message},"exit_code":self.exit})
    }
}
impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::new("IO_ERROR", error.to_string(), 3)
    }
}
impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::usage(error.to_string())
    }
}
#[derive(Debug, Serialize)]
pub struct BuildSupport {
    pub facade: bool,
    pub external_adapter: bool,
    pub version: &'static str,
}
impl Default for BuildSupport {
    fn default() -> Self {
        Self {
            facade: cfg!(feature = "facade"),
            external_adapter: cfg!(feature = "backends"),
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}
