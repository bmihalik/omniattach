//! Download feedback layered on the existing explicit provisioning API.
use crate::{Error, Result};
use crate::model_store::{FetchContext, InstalledModel, ModelFetcher, ModelFile, ModelSpec, ModelStore, ProvisionOptions};
use serde::Serialize;
use std::{cell::Cell, fs, path::Path, sync::mpsc::{self, RecvTimeoutError}, thread, time::{Duration, Instant}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum ModelProvisionStage { Started, CheckingCache, FileStarted, Downloading, FileDownloaded, VerifyingFile, CacheHit, Completed, Cancelled, Failed }
#[derive(Debug, Clone, Serialize)]
pub struct ModelProgressEvent {
    pub schema_version: &'static str,
    pub model_id: String,
    pub revision: String,
    pub stage: ModelProvisionStage,
    pub file: Option<String>,
    pub file_index: Option<usize>,
    pub file_count: usize,
    /// Current staging-file length, not network wire bytes.
    pub file_bytes: u64,
    /// Prior successfully fetched file lengths plus current observed length.
    pub downloaded_bytes: u64,
    /// Unknown: manifest max_bytes is a limit, NOT Content-Length.
    pub total_bytes: Option<u64>,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}
pub trait ModelProgressObserver: Send + Sync {
    /// Runs synchronously on the provisioning caller's thread. Keep fast; do not panic.
    fn event(&self, event: &ModelProgressEvent);
}
impl<F: Fn(&ModelProgressEvent) + Send + Sync> ModelProgressObserver for F {
    fn event(&self, event: &ModelProgressEvent) { self(event) }
}
struct ObservedFetcher<'a> {
    inner: &'a (dyn ModelFetcher + Sync), observer: &'a dyn ModelProgressObserver,
    spec: &'a ModelSpec, start: Instant, started: Cell<usize>, completed_bytes: Cell<u64>,
}
impl ObservedFetcher<'_> {
    fn emit(&self, stage: ModelProvisionStage, file: Option<&ModelFile>, index: Option<usize>, bytes: u64, message: Option<String>) {
        self.observer.event(&ModelProgressEvent {
            schema_version:"omniattach/model-progress/0.1",model_id:self.spec.id.clone(),revision:self.spec.revision.clone(),
            stage,file:file.map(|f|f.name.clone()),file_index:index,file_count:self.spec.files.len(),file_bytes:bytes,
            downloaded_bytes:self.completed_bytes.get().saturating_add(bytes),total_bytes:None,
            elapsed_ms:self.start.elapsed().as_millis().min(u64::MAX as u128) as u64,message,
        });
    }
}
fn observed_bytes(path: &Path) -> u64 {
    fs::symlink_metadata(path).ok().filter(|m|m.file_type().is_file()).map(|m|m.len()).unwrap_or(0)
}
impl ModelFetcher for ObservedFetcher<'_> {
    fn fetch(&self,file:&ModelFile,destination:&Path,context:&FetchContext<'_>)->Result<()> {
        let index=self.started.get()+1;self.started.set(index);let begin=Instant::now();
        self.emit(ModelProvisionStage::FileStarted,Some(file),Some(index),0,None);
        if context.options.cancellation.is_cancelled(){return Err(Error::Cancelled);}
        let remaining=context.remaining.saturating_sub(begin.elapsed());
        if remaining.is_zero(){return Err(Error::Deadline);}
        let adjusted=FetchContext{options:context.options,remaining};let inner=self.inner;
        // The inner fetcher must honor its existing deadline/cancellation contract.
        // Only it runs in the scoped thread; observer calls stay on this thread.
        let result=thread::scope(|scope| {
            let (sender,receiver)=mpsc::sync_channel(1);
            let handle=scope.spawn(move || {let result=inner.fetch(file,destination,&adjusted);let _=sender.send(result);});
            let result=loop {
                match receiver.recv_timeout(Duration::from_millis(200)) {
                    Ok(result)=>break result,
                    Err(RecvTimeoutError::Timeout)=>self.emit(ModelProvisionStage::Downloading,Some(file),Some(index),observed_bytes(destination),None),
                    Err(RecvTimeoutError::Disconnected)=>break Err(Error::InvalidOutput("model fetcher disconnected".into())),
                }
            };
            if handle.join().is_err(){Err(Error::InvalidOutput("model fetcher panicked".into()))}else{result}
        });
        let bytes=observed_bytes(destination);
        self.emit(ModelProvisionStage::Downloading,Some(file),Some(index),bytes,None);
        if let Err(error)=result { self.completed_bytes.set(self.completed_bytes.get().saturating_add(bytes)); return Err(error); }
        self.emit(ModelProvisionStage::FileDownloaded,Some(file),Some(index),bytes,None);
        self.emit(ModelProvisionStage::VerifyingFile,Some(file),Some(index),bytes,None);
        self.completed_bytes.set(self.completed_bytes.get().saturating_add(bytes));
        Ok(())
    }
}
impl ModelStore {
    /// Optional progress-aware equivalent of provision(). Existing APIs/options and
    /// ModelFetcher implementations are unchanged. This variant needs a Sync fetcher.
    /// Size/hash verification and receipt publication remain owned by ModelStore.
    pub fn provision_with_progress(&self,spec:&ModelSpec,options:&ProvisionOptions,fetcher:&(dyn ModelFetcher+Sync),observer:&dyn ModelProgressObserver)->Result<InstalledModel>{
        let observed=ObservedFetcher{inner:fetcher,observer,spec,start:Instant::now(),started:Cell::new(0),completed_bytes:Cell::new(0)};
        observed.emit(ModelProvisionStage::Started,None,None,0,None);
        observed.emit(ModelProvisionStage::CheckingCache,None,None,0,None);
        match self.provision(spec,options,&observed) {
            Ok(installed)=>{
                if observed.started.get()==0{observed.emit(ModelProvisionStage::CacheHit,None,None,0,Some("verified local installation; no download".into()));}
                observed.emit(ModelProvisionStage::Completed,None,None,0,None);Ok(installed)
            }
            Err(error)=>{
                let stage=if matches!(&error,Error::Cancelled){ModelProvisionStage::Cancelled}else{ModelProvisionStage::Failed};
                observed.emit(stage,None,None,0,Some(error.to_string()));Err(error)
            }
        }
    }
}
