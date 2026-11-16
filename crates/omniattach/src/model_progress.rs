//! Hub-native progress. Totals describe the current file, never a safety limit.
use hf_hub::api::tokio::Progress;
use serde::Serialize;
use std::sync::{Arc, Mutex};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvisionStage {
    Started,
    CheckingCache,
    ResolvingRevision,
    FileStarted,
    Downloading,
    FileDownloaded,
    VerifyingFile,
    CacheHit,
    Completed,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModelProgressEvent {
    pub schema_version: &'static str,
    pub model_id: String,
    pub revision: String,
    pub stage: ModelProvisionStage,
    pub file: Option<String>,
    pub file_index: Option<usize>,
    pub file_count: usize,
    pub file_bytes: u64,
    /// SDK counters may include a resumed prefix; this is not network wire traffic.
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}
pub trait ModelProgressObserver: Send + Sync {
    fn event(&self, event: &ModelProgressEvent);
}
impl<F: Fn(&ModelProgressEvent) + Send + Sync> ModelProgressObserver for F {
    fn event(&self, event: &ModelProgressEvent) {
        self(event)
    }
}
#[derive(Clone, Default)]
pub(crate) struct HubProgress {
    pub state: Arc<Mutex<(u64, Option<u64>)>>,
}
impl HubProgress {
    pub fn snapshot(&self) -> (u64, Option<u64>) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}
impl Progress for HubProgress {
    async fn init(&mut self, size: usize, _filename: &str) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = (0, Some(size as u64));
    }
    async fn update(&mut self, size: usize) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.0 = s.0.saturating_add(size as u64);
    }
    async fn finish(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_callback_tracks_real_total_and_updates() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut progress = HubProgress::default();
            progress.init(12, "file").await;
            progress.update(5).await;
            progress.update(2).await;
            assert_eq!(progress.snapshot(), (7, Some(12)));
            progress.finish().await;
            assert_eq!(progress.snapshot(), (7, Some(12)));
        });
    }
}
