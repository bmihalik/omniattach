use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Location {
    /// One-based PDF or image page; absent when the backend cannot supply it.
    pub page: Option<u32>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// Pixel coordinates in the image supplied to OCR, not PDF points.
    pub bbox: Option<BoundingBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TextKind {
    Page,
    OcrWord,
    Transcript,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextUnit {
    pub kind: TextKind,
    pub text: String,
    pub location: Location,
    /// Native backend score, not a calibrated probability (e.g. Tesseract 0..100).
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    ConvertedDocument,
    Audio,
    Frame,
    Attachment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    /// Relative path inside the owned ArtifactBundle workspace.
    pub path: String,
    pub media_type: String,
    pub kind: AssetKind,
    pub bytes: u64,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub elapsed_ms: u64,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    /// Successful-process diagnostic output, bounded to a tail by BackendContext.
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema_version: String,
    pub backend: String,
    pub source: Source,
    pub text: Vec<TextUnit>,
    pub assets: Vec<Asset>,
    pub diagnostics: Vec<Diagnostic>,
    pub invocations: Vec<Invocation>,
    pub metadata: serde_json::Value,
}

impl Manifest {
    pub fn new(backend: &str, source: Source) -> Self {
        Self {
            schema_version: "omniattach-backends/0.1".into(),
            backend: backend.into(),
            source,
            text: vec![],
            assets: vec![],
            diagnostics: vec![],
            invocations: vec![],
            metadata: serde_json::json!({}),
        }
    }
    pub fn warn(&mut self, code: &str, message: &str) {
        self.diagnostics.push(Diagnostic {
            code: code.into(),
            message: message.into(),
        });
    }
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
