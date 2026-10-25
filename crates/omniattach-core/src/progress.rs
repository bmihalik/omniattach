// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! Throttled progress telemetry with weighted overall ratios.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::bundle::{Diagnostic, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressUnit {
    Bytes,
    Pages,
    Slides,
    Sheets,
    Rows,
    XmlRecords,
    Milliseconds,
    Frames,
    Shots,
    Steps,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratio {
    pub completed: u64,
    pub total: Option<u64>,
    pub unit: ProgressUnit,
}

impl Ratio {
    pub fn fraction(&self) -> Option<f32> {
        self.total
            .filter(|&t| t > 0)
            .map(|t| ((self.completed as f64 / t as f64).clamp(0.0, 1.0)) as f32)
    }

    pub fn percent(&self) -> Option<f32> {
        self.fraction().map(|f| f * 100.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub stage_index: u32,
    pub stage_count: u32,
    pub stage_name: String,
    pub stage_ratio: Ratio,
    pub overall_ratio: Option<f32>,
    pub overall_percent: Option<f32>,
    pub elapsed_ms: u64,
    pub rate_per_sec: Option<f32>,
    pub eta_ms: Option<u64>,
}

pub trait ProgressSink: Send + Sync {
    fn on_progress(&self, snapshot: ProgressSnapshot);
    fn on_diagnostic(&self, diagnostic: Diagnostic);
}

pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn on_progress(&self, _: ProgressSnapshot) {}
    fn on_diagnostic(&self, _: Diagnostic) {}
}

pub struct NdjsonStderrSink;

impl ProgressSink for NdjsonStderrSink {
    fn on_progress(&self, snapshot: ProgressSnapshot) {
        if let Ok(line) = serde_json::to_string(&snapshot) {
            eprintln!("{line}");
        }
    }

    fn on_diagnostic(&self, diagnostic: Diagnostic) {
        if let Ok(line) = serde_json::to_string(&diagnostic) {
            eprintln!("{line}");
        }
    }
}

pub struct StageReporter {
    sink: Arc<dyn ProgressSink>,
    started: Instant,
    stage_index: u32,
    stage_count: u32,
    stage_name: &'static str,
    unit: ProgressUnit,
    base_weight: f32,
    stage_weight: f32,
    completed: AtomicU64,
    total: AtomicU64,
    last_emit_ms: AtomicU64,
    last_permille: AtomicU64,
}

impl StageReporter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sink: Arc<dyn ProgressSink>,
        stage_index: u32,
        stage_count: u32,
        stage_name: &'static str,
        unit: ProgressUnit,
        total: Option<u64>,
        base_weight: f32,
        stage_weight: f32,
    ) -> Self {
        Self {
            sink,
            started: Instant::now(),
            stage_index,
            stage_count,
            stage_name,
            unit,
            base_weight,
            stage_weight,
            completed: AtomicU64::new(0),
            total: AtomicU64::new(total.unwrap_or(0)),
            last_emit_ms: AtomicU64::new(0),
            last_permille: AtomicU64::new(0),
        }
    }

    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn inc(&self, delta: u64) {
        let current = self.completed.fetch_add(delta, Ordering::Relaxed) + delta;
        self.update(current);
    }

    pub fn update(&self, completed: u64) {
        self.completed.store(completed, Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        let permille = completed
            .saturating_mul(1000)
            .checked_div(total)
            .unwrap_or(0);
        let finished = total > 0 && completed >= total;

        let due = finished
            || elapsed_ms.saturating_sub(self.last_emit_ms.load(Ordering::Relaxed)) >= 100
            || (total > 0
                && permille.saturating_sub(self.last_permille.load(Ordering::Relaxed)) >= 10);
        if !due {
            return;
        }
        self.last_emit_ms.store(elapsed_ms, Ordering::Relaxed);
        self.last_permille.store(permille, Ordering::Relaxed);

        let ratio = Ratio {
            completed,
            total: (total > 0).then_some(total),
            unit: self.unit,
        };
        let overall_ratio = ratio
            .fraction()
            .map(|f| (self.base_weight + f * self.stage_weight).clamp(0.0, 1.0));
        let rate_per_sec = (elapsed_ms > 0).then(|| completed as f32 * 1000.0 / elapsed_ms as f32);
        let eta_ms = match (ratio.total, rate_per_sec) {
            (Some(t), Some(r)) if r > 0.0 && completed < t => {
                Some(((t - completed) as f32 / r * 1000.0) as u64)
            }
            _ => None,
        };

        self.sink.on_progress(ProgressSnapshot {
            stage_index: self.stage_index,
            stage_count: self.stage_count,
            stage_name: self.stage_name.into(),
            stage_ratio: ratio,
            overall_ratio,
            overall_percent: overall_ratio.map(|r| r * 100.0),
            elapsed_ms,
            rate_per_sec,
            eta_ms,
        });
    }

    pub fn warn(&self, code: &str, message: impl Into<String>, span: Option<SourceSpan>) {
        self.sink.on_diagnostic(Diagnostic {
            code: code.into(),
            severity: "warning".into(),
            stage: self.stage_name.into(),
            message: message.into(),
            span,
        });
    }
}
