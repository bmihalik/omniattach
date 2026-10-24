use crate::Format;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    Extract,
    Markdown,
    Tables,
    PdfLayout,
    PdfScientific,
    Ocr,
    Transcribe,
    VideoFrames,
    MediaProbe,
    Normalize,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionKind {
    InProcess,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRequest {
    pub path: PathBuf,
    pub goal: Goal,
    pub format: Option<Format>,
    pub route: Option<String>,
    /// Converter-defined options, forwarded unchanged. Not interpreted as executable configuration.
    pub options: serde_json::Value,
}
impl ConversionRequest {
    pub fn new(path: impl Into<PathBuf>, goal: Goal) -> Self {
        Self {
            path: path.into(),
            goal,
            format: None,
            route: None,
            options: serde_json::json!({}),
        }
    }
    pub fn with_format(mut self, format: Format) -> Self {
        self.format = Some(format);
        self
    }
    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }
    pub fn with_options(mut self, options: serde_json::Value) -> Self {
        self.options = options;
        self
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Route {
    pub id: String,
    pub formats: Vec<Format>,
    pub goals: Vec<Goal>,
    pub execution: ExecutionKind,
    pub description: String,
}
impl Route {
    pub fn new(
        id: impl Into<String>,
        formats: Vec<Format>,
        goals: Vec<Goal>,
        execution: ExecutionKind,
    ) -> Self {
        Self {
            id: id.into(),
            formats,
            goals,
            execution,
            description: String::new(),
        }
    }
    pub fn described(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }
    pub fn accepts(&self, format: Format, goal: &Goal) -> bool {
        self.formats.contains(&format) && self.goals.contains(goal)
    }
}
