//! Explicit fallback orchestration. Ordinary convert/plan selection is unchanged.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackCandidate {
    pub route: String,
    pub options: Option<serde_json::Value>,
}
impl FallbackCandidate {
    pub fn new(route: impl Into<String>) -> Self {
        Self {
            route: route.into(),
            options: None,
        }
    }
    /// Replace, not merge, the request options for this candidate.
    pub fn with_options(mut self, options: serde_json::Value) -> Self {
        self.options = Some(options);
        self
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackRetry {
    ToolUnavailable,
    ProcessFailure,
    AttemptTimeout,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FallbackAcceptance {
    #[default]
    AnySuccess,
    NonEmptyText,
    MinTextChars {
        min: usize,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FallbackPolicy {
    pub candidates: Vec<FallbackCandidate>,
    pub retry_on: Vec<FallbackRetry>,
    pub acceptance: FallbackAcceptance,
    pub per_attempt_timeout_ms: Option<u64>,
}
impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            candidates: Vec::new(),
            retry_on: vec![
                FallbackRetry::ToolUnavailable,
                FallbackRetry::ProcessFailure,
            ],
            acceptance: FallbackAcceptance::AnySuccess,
            per_attempt_timeout_ms: None,
        }
    }
}
impl FallbackPolicy {
    pub fn new<I, S>(routes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            candidates: routes.into_iter().map(FallbackCandidate::new).collect(),
            ..Default::default()
        }
    }
    pub fn with_acceptance(mut self, acceptance: FallbackAcceptance) -> Self {
        self.acceptance = acceptance;
        self
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackOutcome {
    Accepted,
    Rejected,
    RetryableError,
    TerminalError,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackAttempt {
    pub route: String,
    pub outcome: FallbackOutcome,
    pub elapsed_ms: u64,
    pub retry_class: Option<FallbackRetry>,
    pub detail: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackReport {
    pub schema_version: String,
    pub goal: Goal,
    pub candidates: Vec<String>,
    pub source: Option<SourceIdentity>,
    pub attempts: Vec<FallbackAttempt>,
    pub selected_route: Option<String>,
    pub elapsed_ms: u64,
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}
fn check(context: &FacadeContext, start: Instant) -> Result<()> {
    if context.cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if context.policy.timeout.is_some_and(|d| start.elapsed() >= d) {
        return Err(Error::Deadline);
    }
    Ok(())
}
fn validate(request: &ConversionRequest, policy: &FallbackPolicy) -> Result<()> {
    if request.route.is_some() {
        return Err(Error::Configuration(
            "fallback and request.route are mutually exclusive".into(),
        ));
    }
    if policy.candidates.is_empty() || policy.candidates.len() > 32 {
        return Err(Error::Configuration(
            "fallback requires 1..=32 candidates".into(),
        ));
    }
    if policy.per_attempt_timeout_ms == Some(0) {
        return Err(Error::Configuration(
            "per-attempt timeout must be positive".into(),
        ));
    }
    if matches!(
        policy.acceptance,
        FallbackAcceptance::MinTextChars { min: 0 }
    ) {
        return Err(Error::Configuration(
            "minimum text characters must be positive".into(),
        ));
    }
    let mut seen = HashSet::new();
    for candidate in &policy.candidates {
        if candidate.route.trim().is_empty() || !seen.insert(&candidate.route) {
            return Err(Error::Configuration(
                "fallback route IDs must be nonempty and unique".into(),
            ));
        }
    }
    Ok(())
}
fn retry_class(error: &Error, attempt_cap: bool) -> Option<FallbackRetry> {
    match error {
        Error::Deadline if attempt_cap => Some(FallbackRetry::AttemptTimeout),
        #[cfg(feature = "backends")]
        Error::Backend(omniattach_backends::Error::Spawn { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Some(FallbackRetry::ToolUnavailable)
        }
        #[cfg(feature = "backends")]
        Error::Backend(omniattach_backends::Error::ProcessFailed { code: Some(_), .. }) => {
            Some(FallbackRetry::ProcessFailure)
        }
        #[cfg(feature = "backends")]
        Error::Backend(omniattach_backends::Error::Timeout(_)) => {
            Some(FallbackRetry::AttemptTimeout)
        }
        _ => None,
    }
}
fn accepted(bundle: &FacadeBundle, rule: &FallbackAcceptance) -> bool {
    let count = || {
        bundle
            .manifest()
            .text
            .iter()
            .flat_map(|t| t.text.chars())
            .filter(|c| !c.is_whitespace())
            .count()
    };
    match rule {
        FallbackAcceptance::AnySuccess => true,
        FallbackAcceptance::NonEmptyText => count() > 0,
        FallbackAcceptance::MinTextChars { min } => count() >= *min,
    }
}
fn attempt(
    route: &str,
    outcome: FallbackOutcome,
    start: Instant,
    class: Option<FallbackRetry>,
    detail: Option<String>,
) -> FallbackAttempt {
    FallbackAttempt {
        route: route.into(),
        outcome,
        elapsed_ms: elapsed(start),
        retry_class: class,
        detail: detail.map(|s| s.chars().take(1024).collect()),
    }
}
fn snapshot(
    request: &ConversionRequest,
    context: &FacadeContext,
    start: Instant,
) -> Result<(tempfile::TempDir, PathBuf, SourceIdentity)> {
    check(context, start)?;
    let mut reader = File::open(&request.path)?;
    if !reader.metadata()?.is_file() {
        return Err(Error::InvalidInput("not a regular file".into()));
    }
    let directory = if let Some(parent) = &context.policy.scratch_parent {
        tempfile::Builder::new()
            .prefix("omniattach-fallback-")
            .tempdir_in(parent.canonicalize()?)?
    } else {
        tempfile::Builder::new()
            .prefix("omniattach-fallback-")
            .tempdir()?
    };
    let extension = request
        .path
        .extension()
        .and_then(|v| v.to_str())
        .filter(|v| v.len() <= 16 && v.bytes().all(|b| b.is_ascii_alphanumeric()));
    let path = directory.path().join(
        extension
            .map(|e| format!("input.{e}"))
            .unwrap_or_else(|| "input".into()),
    );
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(context, start)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| Error::Limit("fallback input size overflow".into()))?;
        if bytes > context.policy.max_input_bytes {
            return Err(Error::Limit("fallback input bytes".into()));
        }
        hash.update(&buffer[..count]);
        writer.write_all(&buffer[..count])?;
    }
    check(context, start)?;
    Ok((
        directory,
        path,
        SourceIdentity {
            name: request.path.to_string_lossy().into_owned(),
            bytes,
            sha256: format!("{:x}", hash.finalize()),
        },
    ))
}
impl OmniAttach {
    /// Run an explicit ordered chain; ordinary convert remains single-route.
    /// Success includes an audit payload; failure is Error::Fallback with typed source/report.
    pub fn convert_with_fallback(
        &self,
        request: &ConversionRequest,
        context: &FacadeContext,
        policy: &FallbackPolicy,
    ) -> Result<FacadeBundle> {
        let start = Instant::now();
        let mut report = FallbackReport {
            schema_version: "omniattach/fallback/0.1".into(),
            goal: request.goal.clone(),
            candidates: policy.candidates.iter().map(|c| c.route.clone()).collect(),
            source: None,
            attempts: Vec::new(),
            selected_route: None,
            elapsed_ms: 0,
        };
        match self.run_fallback(request, context, policy, start, &mut report) {
            Ok(bundle) => Ok(bundle),
            Err(source) => {
                report.elapsed_ms = elapsed(start);
                context.progress.event(&ProgressEvent {
                    route: report
                        .attempts
                        .last()
                        .map(|a| a.route.clone())
                        .unwrap_or_else(|| "fallback".into()),
                    stage: Stage::Failed,
                    elapsed_ms: report.elapsed_ms,
                    message: Some(source.to_string().chars().take(1024).collect()),
                });
                Err(Error::Fallback {
                    report: Box::new(report),
                    source: Box::new(source),
                })
            }
        }
    }
    fn run_fallback(
        &self,
        request: &ConversionRequest,
        context: &FacadeContext,
        policy: &FallbackPolicy,
        start: Instant,
        report: &mut FallbackReport,
    ) -> Result<FacadeBundle> {
        check(context, start)?;
        validate(request, policy)?;
        let (_snapshot, path, source) = snapshot(request, context, start)?;
        report.source = Some(source.clone());
        let mut requests = Vec::new();
        // Validate the entire chain, including permissions, before the first converter.
        for candidate in &policy.candidates {
            check(context, start)?;
            let mut request = request.clone();
            request.path = path.clone();
            request.route = Some(candidate.route.clone());
            if let Some(options) = &candidate.options {
                request.options = options.clone();
            }
            self.plan(&request, context)?;
            check(context, start)?;
            requests.push(request);
        }
        for request in requests {
            check(context, start)?;
            let route = request.route.as_deref().expect("preflight sets route");
            let mut child = context.clone();
            let overall = context
                .policy
                .timeout
                .map(|d| d.saturating_sub(start.elapsed()));
            let per_attempt = policy.per_attempt_timeout_ms.map(Duration::from_millis);
            child.policy.timeout = match (overall, per_attempt) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            let observer = context.progress.clone();
            child.progress = Arc::new(move |event: &ProgressEvent| {
                if !matches!(&event.stage, Stage::Completed | Stage::Failed) {
                    let mut event = event.clone();
                    event.elapsed_ms = elapsed(start);
                    observer.event(&event);
                }
            });
            let began = Instant::now();
            let result = self.convert(&request, &child);
            if let Err(error) = check(context, start) {
                report.attempts.push(attempt(
                    route,
                    FallbackOutcome::TerminalError,
                    began,
                    None,
                    Some(error.to_string()),
                ));
                return Err(error);
            }
            match result {
                Ok(mut bundle) => {
                    if !accepted(&bundle, &policy.acceptance) {
                        report.attempts.push(attempt(
                            route,
                            FallbackOutcome::Rejected,
                            began,
                            None,
                            Some("text acceptance criterion not satisfied".into()),
                        ));
                        bundle.discard()?;
                        continue;
                    }
                    if bundle.payload("omniattach/fallback/0.1").is_some() {
                        let error = Error::InvalidOutput(
                            "converter used the reserved fallback report schema".into(),
                        );
                        report.attempts.push(attempt(
                            route,
                            FallbackOutcome::TerminalError,
                            began,
                            None,
                            Some(error.to_string()),
                        ));
                        return Err(error);
                    }
                    report.attempts.push(attempt(
                        route,
                        FallbackOutcome::Accepted,
                        began,
                        None,
                        None,
                    ));
                    report.selected_route = Some(route.into());
                    report.elapsed_ms = elapsed(start);
                    let payload = serde_json::to_value(&*report)?;
                    let manifest = bundle.manifest_mut();
                    manifest.source = source.clone();
                    manifest.elapsed_ms = report.elapsed_ms;
                    manifest.payloads.push(Payload {
                        schema: "omniattach/fallback/0.1".into(),
                        value: payload,
                    });
                    manifest.warnings.push(Warning { code: "FALLBACK_SELECTED".into(), message: format!("Selected {route} after {} attempt(s); only the accepted bundle is retained", report.attempts.len()) });
                    manifest.warnings.push(Warning { code: "FALLBACK_SOURCE_SNAPSHOT".into(), message: "Facade source identifies the original input; converter payload source names may identify staged snapshot copies. Artifact paths and bytes are unchanged.".into() });
                    check(context, start)?;
                    context.progress.event(&ProgressEvent {
                        route: route.into(),
                        stage: Stage::Completed,
                        elapsed_ms: elapsed(start),
                        message: None,
                    });
                    return Ok(bundle);
                }
                Err(error) => {
                    let class = retry_class(&error, per_attempt.is_some());
                    let retry = class.is_some_and(|c| policy.retry_on.contains(&c));
                    report.attempts.push(attempt(
                        route,
                        if retry {
                            FallbackOutcome::RetryableError
                        } else {
                            FallbackOutcome::TerminalError
                        },
                        began,
                        class,
                        Some(error.to_string()),
                    ));
                    if !retry {
                        return Err(error);
                    }
                }
            }
        }
        check(context, start)?;
        Err(Error::FallbackExhausted)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_validation_and_json_defaults() {
        let request = ConversionRequest::new("a.pdf", Goal::Markdown);
        assert!(validate(&request, &FallbackPolicy::new(["a", "b"])).is_ok());
        assert!(validate(&request, &FallbackPolicy::new(["a", "a"])).is_err());
        assert!(
            validate(
                &request.clone().with_route("a"),
                &FallbackPolicy::new(["a"])
            )
            .is_err()
        );
        assert!(validate(&request, &FallbackPolicy::default()).is_err());
        let policy: FallbackPolicy =
            serde_json::from_str(r#"{"candidates":[{"route":"a"}]}"#).unwrap();
        assert_eq!(policy.retry_on.len(), 2);
        assert!(policy.candidates[0].options.is_none());
    }
    #[test]
    fn terminal_classes_cannot_be_enabled_as_retries() {
        for error in [
            Error::Cancelled,
            Error::Limit("fixture".into()),
            Error::InvalidInput("fixture".into()),
            Error::InvalidOutput("fixture".into()),
            Error::ExternalDisabled,
            Error::Converter {
                route: "a".into(),
                message: "opaque failure".into(),
            },
        ] {
            assert!(retry_class(&error, true).is_none());
        }
        assert_eq!(
            retry_class(&Error::Deadline, true),
            Some(FallbackRetry::AttemptTimeout)
        );
        assert_eq!(retry_class(&Error::Deadline, false), None);
    }
}
