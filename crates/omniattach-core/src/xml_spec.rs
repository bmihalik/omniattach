// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! XML inspect-then-convert contract (two-stage mapping).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const XML_SPEC_SCHEMA: &str = "omniattach.xml_spec.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlProbeReport {
    pub schema_version: String,
    pub unambiguous: bool,
    pub ambiguities: Vec<XmlAmbiguity>,
    pub proposed_spec: XmlTableSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlAmbiguity {
    pub element_path: String,
    pub reason: String,
    pub repeated_children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlTableSpec {
    pub table_name: String,
    pub namespaces: BTreeMap<String, String>,
    pub record_path: String,
    pub primary_key: Option<String>,
    pub null_repr: String,
    pub columns: Vec<XmlColumnSpec>,
    pub child_tables: Vec<XmlChildTableSpec>,
}

impl XmlTableSpec {
    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        serde_json::from_str(text)
    }

    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlColumnSpec {
    pub name: String,
    pub selector: String,
    pub data_type: String,
    pub on_repeat: RepeatPolicy,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum RepeatPolicy {
    #[default]
    First,
    Error,
    Join {
        delimiter: String,
    },
    JsonArray,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlChildTableSpec {
    pub table_name: String,
    pub relative_record_path: String,
    pub parent_key_column: String,
    pub columns: Vec<XmlColumnSpec>,
}
