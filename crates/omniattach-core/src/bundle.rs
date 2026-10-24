// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! ConversionBundle: canonical result envelope and typed records.

use serde::{Deserialize, Serialize};

pub const BUNDLE_SCHEMA: &str = "omniattach.bundle.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionBundle {
    pub schema_version: String,
    pub status: BundleStatus,
    pub source: SourceManifest,
    pub documents: Vec<Document>,
    pub tables: Vec<Table>,
    pub timelines: Vec<Timeline>,
    pub assets: Vec<AssetRef>,
    pub products: Vec<Product>,
    pub derivations: Vec<Derivation>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ConversionBundle {
    pub fn new(source: SourceManifest) -> Self {
        Self {
            schema_version: BUNDLE_SCHEMA.into(),
            status: BundleStatus::Completed,
            source,
            documents: vec![],
            tables: vec![],
            timelines: vec![],
            assets: vec![],
            products: vec![],
            derivations: vec![],
            diagnostics: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    Completed,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceManifest {
    pub source_id: String,
    pub path: Option<String>,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub duration_us: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub document_id: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub block_id: String,
    pub parent_id: Option<String>,
    pub kind: BlockKind,
    pub evidence: EvidenceKind,
    pub text: String,
    pub heading: Option<String>,
    pub section_path: Option<String>,
    pub keep_together: bool,
    pub spans: Vec<SourceSpan>,
    pub asset_ids: Vec<String>,
    pub table_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    Code { language: Option<String> },
    ListItem { ordered: bool, depth: u8 },
    TableRef,
    Equation { display: bool },
    FigureCaption,
    TranscriptSegment { speaker: Option<String> },
    OcrRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Extracted,
    Recognized,
    Inferred,
    Generated,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_range: Option<[u64; 2]>,
    pub line_range: Option<[u32; 2]>,
    pub page: Option<u32>,
    pub slide: Option<u32>,
    pub sheet: Option<String>,
    pub cell_range: Option<String>,
    pub xml_path: Option<String>,
    pub time_us: Option<[i64; 2]>,
    pub pts: Option<RationalSpan>,
    pub bbox: Option<BoundingBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RationalSpan {
    pub start: i64,
    pub end: i64,
    pub num: i32,
    pub den: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub page_width: f32,
    pub page_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub table_id: String,
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub row_count: u64,
    pub preview_rows: Vec<Vec<CellValue>>,
    pub csv_asset_id: Option<String>,
    pub spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellValue {
    pub raw: Option<String>,
    pub formula: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub timeline_id: String,
    pub duration_us: i64,
    pub shots: Vec<Shot>,
    pub transcript: Vec<TranscriptItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shot {
    pub shot_id: String,
    pub time_us: [i64; 2],
    pub cut_score: f32,
    pub frame_asset_ids: Vec<String>,
    pub transcript_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptItem {
    pub segment_id: String,
    pub time_us: [i64; 2],
    pub text: String,
    pub block_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRef {
    pub asset_id: String,
    pub role: String,
    pub media_type: String,
    pub rel_path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub spans: Vec<SourceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub kind: String,
    pub required: bool,
    pub status: BundleStatus,
    pub asset_ids: Vec<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Derivation {
    pub step_id: String,
    pub extractor_name: String,
    pub extractor_version: String,
    pub model_id: Option<String>,
    pub config_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub message: String,
    pub span: Option<SourceSpan>,
}
