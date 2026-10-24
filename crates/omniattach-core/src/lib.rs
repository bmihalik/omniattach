// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! Core contracts for omniattach: no parsers, no I/O beyond progress output.

pub mod backend;
pub mod bundle;
pub mod planner;
pub mod progress;
pub mod xml_spec;

pub use backend::{
    Availability, Backend, BackendDescriptor, CapabilityProfile, ConversionJob, ExecutionClass,
    ExecutionContext, StageOutput,
};
pub use bundle::*;
pub use planner::{BackendRegistry, PlannedStep};
pub use progress::{
    NdjsonStderrSink, NoopProgressSink, ProgressSink, ProgressSnapshot, ProgressUnit, Ratio,
    StageReporter,
};
pub use xml_spec::{
    RepeatPolicy, XmlAmbiguity, XmlChildTableSpec, XmlColumnSpec, XmlProbeReport, XmlTableSpec,
};
