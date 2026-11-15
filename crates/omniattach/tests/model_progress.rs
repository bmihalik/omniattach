#![cfg(feature="backends")]
use omniattach::{model_store::*,model_progress::*,Error,Result};
use sha2::{Digest,Sha256};
use std::{fs::{self,OpenOptions},io::Write,path::Path,sync::{Mutex,Arc,atomic::{AtomicBool,AtomicUsize,Ordering}},thread,time::{Duration,Instant}};
fn spec()->ModelSpec{ModelSpec{id:"progress-test".into(),revision:"a".repeat(40),files:vec![ModelFile{name:"weights.onnx".into(),url:"https://example.invalid/weights.onnx".into(),sha256:format!("{:x}",Sha256::digest(b"model")),max_bytes:8}]}}
fn options()->ProvisionOptions{ProvisionOptions{allow_network:true,timeout:Duration::from_secs(10),..Default::default()}}
struct Fake{mode:u8,calls:AtomicUsize,seen:Arc<AtomicBool>}
impl ModelFetcher for Fake{
    fn fetch(&self,_:&ModelFile,path:&Path,context:&FetchContext<'_>)->Result<()>{
        self.calls.fetch_add(1,Ordering::SeqCst);
        if self.mode==3{panic!("test transport panic");}
        if self.mode==1{fs::write(path,b"wrong")?;return Ok(());}
        if self.mode==4{fs::write(path,b"mo")?;return Err(Error::InvalidOutput("transport failed".into()));}
        if self.mode==2{
            fs::write(path,b"mo")?;let start=Instant::now();
            while !self.seen.load(Ordering::SeqCst){
                if context.options.cancellation.is_cancelled(){return Err(Error::Cancelled);}
                if start.elapsed()>Duration::from_secs(5){return Err(Error::InvalidOutput("live progress was not delivered".into()));}
                thread::sleep(Duration::from_millis(5));
            }
            OpenOptions::new().append(true).open(path)?.write_all(b"del")?;
        }else{fs::write(path,b"model")?;}
        Ok(())
    }
}
fn fake(mode:u8)->Fake{Fake{mode,calls:AtomicUsize::new(0),seen:Arc::new(AtomicBool::new(false))}}
#[test]fn live_progress_arrives_before_fetch_finishes(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let transport=fake(2);let events=Mutex::new(Vec::<ModelProgressEvent>::new());
    let observer=|event:&ModelProgressEvent|{if event.stage==ModelProvisionStage::Downloading&&event.file_bytes==2{transport.seen.store(true,Ordering::SeqCst);}events.lock().unwrap().push(event.clone());};
    store.provision_with_progress(&spec(),&options(),&transport,&observer).unwrap();
    let events=events.lock().unwrap();let live=events.iter().position(|e|e.stage==ModelProvisionStage::Downloading&&e.file_bytes==2).unwrap();let done=events.iter().position(|e|e.stage==ModelProvisionStage::FileDownloaded).unwrap();
    assert!(live<done);assert_eq!(events.last().unwrap().stage,ModelProvisionStage::Completed);assert_eq!(events.last().unwrap().downloaded_bytes,5);
    assert!(events.iter().all(|e|e.total_bytes.is_none()));assert!(events.windows(2).all(|w|w[0].elapsed_ms<=w[1].elapsed_ms));
}
#[test]fn checksum_failure_never_reports_completed(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.stage);
    assert!(store.provision_with_progress(&spec(),&options(),&fake(1),&observer).is_err());let events=events.lock().unwrap();
    assert!(events.contains(&ModelProvisionStage::VerifyingFile));assert!(!events.contains(&ModelProvisionStage::Completed));assert_eq!(events.last(),Some(&ModelProvisionStage::Failed));
}
#[test]fn verified_cache_reports_hit_without_fetch_or_network(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let transport=fake(0);store.provision(&spec(),&options(),&transport).unwrap();
    let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.clone());
    store.provision_with_progress(&spec(),&ProvisionOptions::default(),&transport,&observer).unwrap();assert_eq!(transport.calls.load(Ordering::SeqCst),1);
    let events=events.lock().unwrap();assert!(events.iter().any(|e|e.stage==ModelProvisionStage::CacheHit));assert!(events.iter().all(|e|e.downloaded_bytes==0));
}
#[test]fn cancellation_is_a_terminal_event(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let opts=options();let events=Mutex::new(Vec::new());
    let observer=|e:&ModelProgressEvent|{events.lock().unwrap().push(e.stage);if e.stage==ModelProvisionStage::Started{opts.cancellation.cancel();}};
    assert!(matches!(store.provision_with_progress(&spec(),&opts,&fake(0),&observer),Err(Error::Cancelled)));
    assert_eq!(events.lock().unwrap().last(),Some(&ModelProvisionStage::Cancelled));
}
#[test]fn offline_failure_has_no_file_started_event(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.stage);
    assert!(store.provision_with_progress(&spec(),&ProvisionOptions::default(),&fake(0),&observer).is_err());let events=events.lock().unwrap();assert!(!events.contains(&ModelProvisionStage::FileStarted));assert_eq!(events.last(),Some(&ModelProvisionStage::Failed));
}
#[test]fn fetcher_panic_becomes_failed_not_completed(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.stage);
    assert!(store.provision_with_progress(&spec(),&options(),&fake(3),&observer).is_err());assert_eq!(events.lock().unwrap().last(),Some(&ModelProvisionStage::Failed));
}
#[test]fn failed_partial_download_retains_observed_bytes(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.clone());
    assert!(store.provision_with_progress(&spec(),&options(),&fake(4),&observer).is_err());let events=events.lock().unwrap();assert_eq!(events.last().unwrap().downloaded_bytes,2);assert_eq!(events.last().unwrap().stage,ModelProvisionStage::Failed);
}
#[test]fn file_indices_and_aggregate_byte_counts_are_consistent(){
    let temp=tempfile::tempdir().unwrap();let store=ModelStore::new(temp.path()).unwrap();let mut s=spec();let base=s.files[0].clone();s.files=(0..3).map(|i|ModelFile{name:format!("weights-{i}.onnx"),..base.clone()}).collect();
    let events=Mutex::new(Vec::new());let observer=|e:&ModelProgressEvent|events.lock().unwrap().push(e.clone());store.provision_with_progress(&s,&options(),&fake(0),&observer).unwrap();let events=events.lock().unwrap();
    assert_eq!(events.iter().filter(|e|e.stage==ModelProvisionStage::FileStarted).map(|e|e.file_index.unwrap()).collect::<Vec<_>>(),vec![1,2,3]);assert_eq!(events.last().unwrap().downloaded_bytes,15);assert!(events.iter().all(|e|e.file_count==3));
}
