#![cfg(feature="backends")]
use omniattach::{model_store::*, Error, Result};
use sha2::{Digest,Sha256};
use std::{fs,path::Path,sync::atomic::{AtomicUsize,Ordering},time::Duration};
struct Fake { calls: AtomicUsize, bad: bool }
impl ModelFetcher for Fake {
    fn fetch(&self,_:&ModelFile,path:&Path,_:&FetchContext<'_>)->Result<()> {
        self.calls.fetch_add(1,Ordering::SeqCst);fs::write(path,if self.bad {b"wrong"}else{b"model"})?;Ok(())
    }
}
fn spec()->ModelSpec {ModelSpec{id:"test-model".into(),revision:"a".repeat(40),files:vec![ModelFile{name:"weights.onnx".into(),url:"https://example.invalid/pinned/weights.onnx".into(),sha256:format!("{:x}",Sha256::digest(b"model")),max_bytes:8}]}}
fn allow()->ProvisionOptions {ProvisionOptions{allow_network:true,..Default::default()}}
fn fake(bad:bool)->Fake {Fake{calls:AtomicUsize::new(0),bad}}
#[test]fn offline_by_default_and_inspection_has_no_side_effects(){
    let tmp=tempfile::tempdir().unwrap();let root=tmp.path().join("cache");let store=ModelStore::new(&root).unwrap();let fetch=fake(false);
    assert!(matches!(store.inspect(&spec(),&ProvisionOptions::default()).unwrap(),ModelStatus::Missing));
    assert!(store.provision(&spec(),&ProvisionOptions::default(),&fetch).is_err());assert!(!root.exists());assert_eq!(fetch.calls.load(Ordering::SeqCst),0);
}
#[test]fn successful_publication_and_offline_cache_reuse(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let fetch=fake(false);let spec=spec();
    let model=store.provision(&spec,&allow(),&fetch).unwrap();assert!(model.directory().join("model-installation.json").is_file());
    assert_eq!(fs::read(model.file("weights.onnx").unwrap()).unwrap(),b"model");
    store.provision(&spec,&ProvisionOptions::default(),&fetch).unwrap();assert_eq!(fetch.calls.load(Ordering::SeqCst),1);
    assert!(matches!(store.inspect(&spec,&ProvisionOptions::default()).unwrap(),ModelStatus::Ready{..}));
}
#[test]fn bad_hash_never_publishes_a_ready_revision(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let spec=spec();
    assert!(store.provision(&spec,&allow(),&fake(true)).is_err());assert!(!tmp.path().join(&spec.id).join(&spec.revision).exists());
}
#[test]fn corrupt_and_incomplete_installations_are_not_overwritten(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let spec=spec();let fetch=fake(false);
    let model=store.provision(&spec,&allow(),&fetch).unwrap();fs::write(model.file("weights.onnx").unwrap(),b"bad").unwrap();
    assert!(matches!(store.inspect(&spec,&ProvisionOptions::default()).unwrap(),ModelStatus::Invalid{..}));assert!(store.provision(&spec,&allow(),&fetch).is_err());assert_eq!(fetch.calls.load(Ordering::SeqCst),1);
    fs::remove_file(model.directory().join("model-installation.json")).unwrap();assert!(store.verify(&spec,&ProvisionOptions::default()).is_err());
}
#[test]fn traversal_duplicate_names_and_unpinned_revisions_fail_before_fetch(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let fetch=fake(false);
    for mode in 0..4 {let mut s=spec();match mode{0=>s.files[0].name="../escape".into(),1=>s.revision="main".into(),2=>s.files.push(s.files[0].clone()),_=>s.files[0].url="http://example.invalid/file".into()};assert!(store.provision(&s,&allow(),&fetch).is_err());}
    assert_eq!(fetch.calls.load(Ordering::SeqCst),0);
}
#[test]fn size_limits_timeout_and_cancellation_fail_closed(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let fetch=fake(false);
    let mut options=allow();options.max_total_bytes=1;assert!(store.provision(&spec(),&options,&fetch).is_err());
    let mut options=allow();options.timeout=Duration::ZERO;assert!(matches!(store.provision(&spec(),&options,&fetch),Err(Error::Deadline)));
    let options=allow();options.cancellation.cancel();assert!(matches!(store.provision(&spec(),&options,&fetch),Err(Error::Cancelled)));assert_eq!(fetch.calls.load(Ordering::SeqCst),0);
}
#[test]fn curl_requires_external_permission_without_running_any_program(){
    let tmp=tempfile::tempdir().unwrap();let options=allow();let fetch=CurlModelFetcher::default();
    assert!(matches!(fetch.fetch(&spec().files[0],&tmp.path().join("file"),&FetchContext{options:&options,remaining:Duration::from_secs(1)}),Err(Error::ExternalDisabled)));
}
#[test]fn concurrent_writer_lock_is_respected(){
    let tmp=tempfile::tempdir().unwrap();let s=spec();let parent=tmp.path().join(&s.id);fs::create_dir(&parent).unwrap();fs::write(parent.join(format!(".{}.lock",s.revision)),b"locked").unwrap();
    let store=ModelStore::new(tmp.path()).unwrap();let fetch=fake(false);assert!(store.provision(&s,&allow(),&fetch).is_err());assert_eq!(fetch.calls.load(Ordering::SeqCst),0);
}
#[test]fn oversized_payload_is_not_accepted(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path()).unwrap();let mut s=spec();s.files[0].max_bytes=4;
    assert!(matches!(store.provision(&s,&allow(),&fake(false)),Err(Error::Limit(_))));
}
#[test]fn checked_in_profile_is_revision_pinned(){
    let s=texteller3_onnx_spec().unwrap();assert_eq!(s.files.len(),3);assert_eq!(s.revision.len(),40);
    for f in s.files {assert!(f.url.contains(&s.revision));assert_eq!(f.sha256.len(),64);}
}
#[cfg(feature="documents")]
#[test]fn verified_bundle_generates_existing_worker_config(){
    let tmp=tempfile::tempdir().unwrap();let store=ModelStore::new(tmp.path().join("models")).unwrap();let mut s=spec();
    s.files=["encoder_model.onnx","decoder_model_merged.onnx","tokenizer.json"].iter().map(|name|ModelFile{name:name.to_string(),..s.files[0].clone()}).collect();
    let model=store.provision(&s,&allow(),&fake(false)).unwrap();let worker=tmp.path().join("worker");let runtime=tmp.path().join("runtime.so");fs::write(&worker,b"worker").unwrap();fs::write(&runtime,b"runtime").unwrap();
    let value=model.equation_worker_config_json(&worker,&runtime,512).unwrap();
    assert_eq!(value["argv"][4],"512");assert_eq!(value["artifacts"].as_array().unwrap().len(),5);
    let config:omniattach::pdf_equations_adapter::EquationWorkerConfig=serde_json::from_value(value).unwrap();assert_eq!(config.argv.len(),5);
}
