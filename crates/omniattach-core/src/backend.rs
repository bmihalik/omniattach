// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! Backend contract shared by native parsers and external-process adapters.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::bundle::{
    AssetRef, Derivation, Diagnostic, Document, Product, SourceManifest, Table, Timeline,
};
use crate::progress::StageReporter;
use crate::xml_spec::XmlTableSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionClass {
    CpuBound,
    GlobalMutexNative,
    ExternalProcess,
    LocalAccelerator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProfile {
    Default,
    PdfText,
    PdfLayout,
    PdfOcr,
    PdfScientific,
    XmlExplicitSpec,
    XmlAutoUnambiguous,
    ShotFrames,
    AudioTranscript,
}

#[derive(Debug, Clone)]
pub struct BackendDescriptor {
    pub id: &'static str,
    pub version: &'static str,
    pub supported_mimes: &'static [&'static str],
    pub profiles: &'static [CapabilityProfile],
    pub produces_products: &'static [&'static str],
    pub execution_class: ExecutionClass,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub enum Availability {
    Ready { version_info: Option<String> },
    Unavailable { code: &'static str, reason: String },
}

#[derive(Debug, Clone)]
pub struct ConversionJob {
    pub source: SourceManifest,
    pub input_path: PathBuf,
    pub requested_products: Vec<String>,
    pub profile: CapabilityProfile,
    pub xml_spec: Option<XmlTableSpec>,
    pub shot_threshold: f32,
    pub max_shot_interval_ms: u64,
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub artifact_dir: PathBuf,
    pub scratch_dir: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub timeout_ms: u64,
}

impl ExecutionContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StageOutput {
    pub documents: Vec<Document>,
    pub tables: Vec<Table>,
    pub timelines: Vec<Timeline>,
    pub assets: Vec<AssetRef>,
    pub products: Vec<Product>,
    pub derivation: Option<Derivation>,
}

pub trait Backend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;

    fn probe_availability(&self) -> Availability {
        Availability::Ready { version_info: None }
    }

    #[allow(clippy::result_large_err)]
    fn execute(
        &self,
        job: &ConversionJob,
        ctx: &ExecutionContext,
        reporter: &StageReporter,
    ) -> Result<StageOutput, Diagnostic>;
}
