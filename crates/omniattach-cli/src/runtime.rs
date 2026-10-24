use crate::{CliError, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Default)]
pub struct Runtime {
    cancelled: Arc<AtomicBool>,
    callback: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Runtime {
    pub fn with_cancel_callback(callback: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            callback: Some(Arc::new(callback)),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(callback) = &self.callback {
            callback();
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(CliError::new("CANCELLED", "operation cancelled", 130))
        } else {
            Ok(())
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
