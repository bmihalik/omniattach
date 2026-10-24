// SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
// SPDX-License-Identifier: Apache-2.0

//! Strict route resolution: no profile downgrade unless explicitly allowed.

use std::sync::Arc;

use crate::backend::{Availability, Backend, CapabilityProfile};
use crate::bundle::Diagnostic;

pub struct PlannedStep {
    pub backend: Arc<dyn Backend>,
    pub profile: CapabilityProfile,
    pub downgraded: bool,
}

#[derive(Default)]
pub struct BackendRegistry {
    backends: Vec<Arc<dyn Backend>>,
}

impl BackendRegistry {
    pub fn register(&mut self, backend: Arc<dyn Backend>) {
        self.backends.push(backend);
        self.backends.sort_by_key(|b| -b.descriptor().priority);
    }

    fn ready_for(&self, mime: &str, profile: &CapabilityProfile) -> Vec<Arc<dyn Backend>> {
        self.backends
            .iter()
            .filter(|b| {
                let d = b.descriptor();
                d.supported_mimes.contains(&mime)
                    && d.profiles.contains(profile)
                    && matches!(b.probe_availability(), Availability::Ready { .. })
            })
            .cloned()
            .collect()
    }

    #[allow(clippy::result_large_err)]
    pub fn resolve_chain(
        &self,
        mime: &str,
        profile: &CapabilityProfile,
        allow_downgrade: bool,
    ) -> Result<Vec<PlannedStep>, Diagnostic> {
        let mut chain: Vec<PlannedStep> = self
            .ready_for(mime, profile)
            .into_iter()
            .map(|backend| PlannedStep {
                backend,
                profile: profile.clone(),
                downgraded: false,
            })
            .collect();

        if chain.is_empty() && allow_downgrade && *profile == CapabilityProfile::PdfLayout {
            chain = self
                .ready_for(mime, &CapabilityProfile::PdfText)
                .into_iter()
                .map(|backend| PlannedStep {
                    backend,
                    profile: CapabilityProfile::PdfText,
                    downgraded: true,
                })
                .collect();
        }

        if chain.is_empty() {
            return Err(Diagnostic {
                code: "PROFILE_UNMET".into(),
                severity: "error".into(),
                stage: "plan".into(),
                message: format!("No ready backend for mime={mime} and requested profile"),
                span: None,
            });
        }
        Ok(chain)
    }
}
