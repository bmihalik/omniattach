use crate::{Error, Result};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "backends")]
pub use omniattach_backends::CancellationToken;
#[cfg(not(feature = "backends"))]
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<std::sync::atomic::AtomicBool>);
#[cfg(not(feature = "backends"))]
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub allow_external: bool,
    pub max_input_bytes: u64,
    pub max_export_bytes: u64,
    pub max_export_files: usize,
    /// Cooperative deadline; arbitrary converter code cannot be forcibly interrupted.
    pub timeout: Option<Duration>,
    pub scratch_parent: Option<std::path::PathBuf>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            allow_external: false,
            max_input_bytes: 512 * 1024 * 1024,
            max_export_bytes: 512 * 1024 * 1024,
            max_export_files: 10000,
            timeout: None,
            scratch_parent: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Staging,
    Converting,
    Completed,
    Failed,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProgressEvent {
    pub route: String,
    pub stage: Stage,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}
pub trait ProgressObserver: Send + Sync {
    fn event(&self, event: &ProgressEvent);
}
impl<F: Fn(&ProgressEvent) + Send + Sync> ProgressObserver for F {
    fn event(&self, event: &ProgressEvent) {
        self(event)
    }
}
#[derive(Debug, Default)]
pub struct SilentProgress;
impl ProgressObserver for SilentProgress {
    fn event(&self, _: &ProgressEvent) {}
}
/// Serializes each event as one JSON line, holding stderr's lock for the entire record.
#[derive(Debug, Default)]
pub struct NdjsonProgress;
impl ProgressObserver for NdjsonProgress {
    fn event(&self, event: &ProgressEvent) {
        use std::io::Write;
        let mut stderr = std::io::stderr().lock();
        if let Ok(json) = serde_json::to_string(event) {
            let _ = writeln!(stderr, "{json}");
        }
    }
}

#[derive(Clone)]
pub struct FacadeContext {
    pub policy: Policy,
    pub cancellation: CancellationToken,
    pub progress: Arc<dyn ProgressObserver>,
}
impl Default for FacadeContext {
    fn default() -> Self {
        Self {
            policy: Policy::default(),
            cancellation: CancellationToken::default(),
            progress: Arc::new(SilentProgress),
        }
    }
}
impl FacadeContext {
    pub fn with_external(mut self, allow: bool) -> Self {
        self.policy.allow_external = allow;
        self
    }
    pub fn with_progress(mut self, progress: impl ProgressObserver + 'static) -> Self {
        self.progress = Arc::new(progress);
        self
    }
}

#[derive(Clone)]
pub struct JobContext {
    context: FacadeContext,
    start: Instant,
    deadline: Option<Instant>,
    route: String,
}
impl JobContext {
    pub(crate) fn new(context: &FacadeContext, route: &str) -> Result<Self> {
        let start = Instant::now();
        let deadline = context
            .policy
            .timeout
            .map(|d| {
                start
                    .checked_add(d)
                    .ok_or_else(|| Error::Configuration("deadline duration overflow".into()))
            })
            .transpose()?;
        Ok(Self {
            context: context.clone(),
            start,
            deadline,
            route: route.into(),
        })
    }
    pub(crate) fn for_route(&self, route: &str) -> Self {
        let mut result = self.clone();
        result.route = route.into();
        result
    }
    pub fn check(&self) -> Result<()> {
        if self.context.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(Error::Deadline);
        }
        Ok(())
    }
    pub fn policy(&self) -> &Policy {
        &self.context.policy
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.context.cancellation
    }
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
    }
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    pub fn report(&self, stage: Stage, message: Option<String>) {
        self.context.progress.event(&ProgressEvent {
            route: self.route.clone(),
            stage,
            elapsed_ms: self.start.elapsed().as_millis().min(u64::MAX as u128) as u64,
            message,
        });
    }
}
