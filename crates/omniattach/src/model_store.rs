//! Hugging Face Hub is the only provisioning source. Inference paths stay local.
use crate::{Error,Result,backends::CancellationToken,model_progress::*};
use hf_hub::{Cache,Repo,RepoType,api::tokio::ApiBuilder};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,fs,io::Read,path::{Path,PathBuf},time::{Duration,Instant},future::Future,cell::Cell};
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HubFile {pub path:String,#[serde(default)]pub sha256:Option<String>}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HubModelSpec {pub repo_id:String,pub revision:String,pub files:Vec<HubFile>}
#[derive(Clone)]
pub struct ProvisionOptions {
    pub allow_network:bool,pub refresh:bool,pub token:Option<String>,pub timeout:Duration,
    pub max_file_bytes:u64,pub max_total_bytes:u64,pub cancellation:CancellationToken,
}
impl std::fmt::Debug for ProvisionOptions {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.debug_struct("ProvisionOptions").field("allow_network",&self.allow_network).field("refresh",&self.refresh).field("token",&self.token.as_ref().map(|_|"REDACTED")).field("timeout",&self.timeout).finish_non_exhaustive()}}
impl Default for ProvisionOptions {fn default()->Self{Self{allow_network:false,refresh:false,token:None,timeout:Duration::from_secs(1800),max_file_bytes:8*1024*1024*1024,max_total_bytes:32*1024*1024*1024,cancellation:CancellationToken::default()}}}
#[derive(Debug,Clone,Serialize)]
pub struct InstalledFile {pub path:String,pub sha256:String,pub bytes:u64}
#[derive(Debug,Clone,Serialize)]
pub struct InstalledModel {spec:HubModelSpec,resolved_revision:String,directory:PathBuf,files:Vec<InstalledFile>}
impl InstalledModel {
    pub fn spec(&self)->&HubModelSpec{&self.spec}
    pub fn revision(&self)->&str{&self.resolved_revision}
    pub fn directory(&self)->&Path{&self.directory}
    pub fn files(&self)->&[InstalledFile]{&self.files}
    pub fn file(&self,name:&str)->Result<PathBuf>{if !self.files.iter().any(|f|f.path==name){return Err(Error::Configuration("file not in installed Hub selection".into()));}Ok(self.directory.join(name))}
    #[cfg(feature="documents")]
    pub fn equation_worker_config_json(&self,worker:&Path,runtime:&Path,max_tokens:usize)->Result<serde_json::Value>{
        if !(1..=512).contains(&max_tokens){return Err(Error::Configuration("max_tokens must be 1..=512".into()));}
        let opts=ProvisionOptions::default();let start=Instant::now();let worker=worker.canonicalize()?;let runtime=runtime.canonicalize()?;let mut artifacts=Vec::new();
        for path in [&worker,&runtime]{let (hash,_)=hash_file(path,u64::MAX,&opts,start)?;artifacts.push(serde_json::json!({"path":path,"sha256":hash}));}
        for file in &self.files{let path=self.file(&file.path)?;let (hash,_)=hash_file(&path,opts.max_file_bytes,&opts,start)?;if hash!=file.sha256{return Err(Error::InvalidOutput("cached Hub artifact changed".into()));}artifacts.push(serde_json::json!({"path":path,"sha256":hash}));}
        Ok(serde_json::json!({"argv":[worker,self.file("encoder_model.onnx")?,self.file("decoder_model_merged.onnx")?,self.file("tokenizer.json")?,max_tokens.to_string()],"env":{"ORT_DYLIB_PATH":runtime},"timeout_ms":600000,"artifacts":artifacts}))
    }
}
#[derive(Debug,Clone,Serialize)]
#[serde(tag="status",rename_all="snake_case")]
pub enum ModelStatus{Missing,Ready{model:InstalledModel},Invalid{reason:String}}
#[derive(Debug,Clone)]
pub struct ModelStore{root:PathBuf}
fn component(s:&str)->bool{!s.is_empty()&&s!="."&&s!=".."&&s.len()<=255&&s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'))}
fn relative(s:&str)->bool{!s.is_empty()&&s.len()<=2048&&s.split('/').all(component)}
fn commit(s:&str)->bool{s.len()==40&&s.bytes().all(|b|b.is_ascii_hexdigit())}
fn validate(s:&HubModelSpec)->Result<()>{
    if s.repo_id.split('/').count()>2||!relative(&s.repo_id)||!relative(&s.revision)||s.files.is_empty()||s.files.len()>128{return Err(Error::Configuration("Hub repo ID, revision and 1..=128 relative file paths required".into()));}
    let mut names=BTreeSet::new();for f in &s.files{if !relative(&f.path)||!names.insert(f.path.to_ascii_lowercase())||f.sha256.as_ref().is_some_and(|h|h.len()!=64||!h.bytes().all(|b|b.is_ascii_hexdigit())){return Err(Error::Configuration("invalid/duplicate Hub path or SHA-256".into()));}}Ok(())
}
fn check(o:&ProvisionOptions,start:Instant)->Result<()>{if o.cancellation.is_cancelled(){Err(Error::Cancelled)}else if start.elapsed()>=o.timeout{Err(Error::Deadline)}else{Ok(())}}
fn hash_file(path:&Path,limit:u64,o:&ProvisionOptions,start:Instant)->Result<(String,u64)>{
    let mut file=fs::File::open(path)?;if !file.metadata()?.is_file(){return Err(Error::InvalidInput("Hub artifact is not a regular file".into()));}
    let mut h=Sha256::new();let mut bytes=0u64;let mut b=[0;65536];loop{check(o,start)?;let n=file.read(&mut b)?;if n==0{break;}bytes=bytes.checked_add(n as u64).ok_or_else(||Error::Limit("Hub artifact size".into()))?;if bytes>limit{return Err(Error::Limit("Hub artifact bytes".into()));}h.update(&b[..n]);}Ok((format!("{:x}",h.finalize()),bytes))
}
struct Events<'a>{spec:&'a HubModelSpec,observer:&'a dyn ModelProgressObserver,start:Instant,aggregate:Cell<u64>}
impl Events<'_>{fn emit(&self,stage:ModelProvisionStage,revision:&str,index:Option<usize>,bytes:u64,done:u64,total:Option<u64>,message:Option<String>){let aggregate=if index.is_some(){let value=done.saturating_add(bytes);self.aggregate.set(value);value}else{self.aggregate.get()};self.observer.event(&ModelProgressEvent{schema_version:"omniattach/model-progress/0.2",model_id:self.spec.repo_id.clone(),revision:revision.into(),stage,file:index.map(|i|self.spec.files[i-1].path.clone()),file_index:index,file_count:self.spec.files.len(),file_bytes:bytes,downloaded_bytes:aggregate,total_bytes:total,elapsed_ms:self.start.elapsed().as_millis().min(u64::MAX as u128)as u64,message});}}
async fn guarded<T>(future:impl Future<Output=Result<T>>,o:&ProvisionOptions,e:&Events<'_>,revision:&str,index:Option<usize>,done:u64,prior_sizes:u64,progress:Option<&crate::model_progress::HubProgress>)->Result<T>{
    tokio::pin!(future);loop{check(o,e.start)?;
        if let Some(p)=progress{let (bytes,total)=p.snapshot();if bytes>o.max_file_bytes||total.is_some_and(|v|v>o.max_file_bytes)||prior_sizes.saturating_add(total.unwrap_or(bytes))>o.max_total_bytes{return Err(Error::Limit("Hub download byte budget".into()));}}
        tokio::select!{result=&mut future=>{check(o,e.start)?;return result;},_=tokio::time::sleep(Duration::from_millis(200))=>{if let Some(p)=progress{let (b,t)=p.snapshot();e.emit(ModelProvisionStage::Downloading,revision,index,b,done,t,None);}}}
    }
}
impl ModelStore {
    pub fn new(root:impl Into<PathBuf>)->Result<Self>{let root=root.into();Ok(Self{root:if root.is_absolute(){root}else{std::env::current_dir()?.join(root)}})}
    pub fn from_env()->Result<Self>{if let Some(root)=std::env::var_os("HF_HUB_CACHE"){return Self::new(PathBuf::from(root));}Self::new(Cache::from_env().path().clone())}
    pub fn root(&self)->&Path{&self.root}
    fn repo(&self,s:&HubModelSpec)->Repo{Repo::with_revision(s.repo_id.clone(),RepoType::Model,s.revision.clone())}
    fn cached_revision(&self,s:&HubModelSpec)->Result<Option<String>>{
        if commit(&s.revision){return Ok(Some(s.revision.to_ascii_lowercase()));}
        let path=self.root.join(self.repo(s).folder_name()).join("refs").join(&s.revision);
        let mut raw=String::new();match fs::File::open(path){Ok(file)=>{file.take(129).read_to_string(&mut raw)?;},Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.into())}
        let revision=raw.trim();if !commit(revision){return Err(Error::InvalidOutput("invalid Hub cache revision reference".into()));}Ok(Some(revision.to_string()))
    }
    fn snapshot(&self,s:&HubModelSpec,revision:&str,o:&ProvisionOptions,start:Instant)->Result<InstalledModel>{
        let directory=self.root.join(self.repo(s).folder_name()).join("snapshots").join(revision);let root=self.root.canonicalize()?;let mut files=Vec::new();let mut total=0u64;
        for file in &s.files{let path=directory.join(&file.path);let actual=path.canonicalize()?;if !actual.starts_with(&root){return Err(Error::InvalidInput("Hub cache file escapes configured root".into()));}
            let (hash,bytes)=hash_file(&path,o.max_file_bytes,o,start)?;if file.sha256.as_ref().is_some_and(|h|h.to_ascii_lowercase()!=hash){return Err(Error::InvalidOutput(format!("Hub checksum mismatch: {}",file.path)));}
            total=total.checked_add(bytes).ok_or_else(||Error::Limit("Hub total bytes".into()))?;if total>o.max_total_bytes{return Err(Error::Limit("Hub total bytes".into()));}files.push(InstalledFile{path:file.path.clone(),sha256:hash,bytes});}
        Ok(InstalledModel{spec:s.clone(),resolved_revision:revision.into(),directory,files})
    }
    pub fn verify(&self,s:&HubModelSpec,o:&ProvisionOptions)->Result<InstalledModel>{validate(s)?;let start=Instant::now();check(o,start)?;let revision=self.cached_revision(s)?.ok_or_else(||Error::InvalidInput("Hub revision is not cached".into()))?;self.snapshot(s,&revision,o,start)}
    pub fn inspect(&self,s:&HubModelSpec,o:&ProvisionOptions)->Result<ModelStatus>{validate(s)?;match self.verify(s,o){Ok(model)=>Ok(ModelStatus::Ready{model}),Err(Error::Io(e))if e.kind()==std::io::ErrorKind::NotFound=>Ok(ModelStatus::Missing),Err(Error::InvalidInput(message))if message=="Hub revision is not cached"=>Ok(ModelStatus::Missing),Err(Error::Cancelled)=>Err(Error::Cancelled),Err(Error::Deadline)=>Err(Error::Deadline),Err(e)=>Ok(ModelStatus::Invalid{reason:e.to_string()})}}
    pub fn provision(&self,s:&HubModelSpec,o:&ProvisionOptions)->Result<InstalledModel>{self.provision_with_progress(s,o,&|_:&ModelProgressEvent|{})}
    /// Own runtime prevents nested-runtime panics and contains SDK background tasks.
    /// Observer callbacks can run on this provisioning worker thread; use UI channels.
    pub fn provision_with_progress(&self,s:&HubModelSpec,o:&ProvisionOptions,observer:&dyn ModelProgressObserver)->Result<InstalledModel>{
        std::thread::scope(|scope|scope.spawn(||{
            let events=Events{spec:s,observer,start:Instant::now(),aggregate:Cell::new(0)};events.emit(ModelProvisionStage::Started,&s.revision,None,0,0,None,None);
            let result=match tokio::runtime::Builder::new_current_thread().enable_all().build(){
                Ok(runtime)=>{let result=runtime.block_on(self.run(s,o,&events));runtime.shutdown_timeout(Duration::from_secs(1));result},Err(error)=>Err(Error::Io(error)),
            };
            let stage=match &result{Ok(_)=>ModelProvisionStage::Completed,Err(Error::Cancelled)=>ModelProvisionStage::Cancelled,Err(_)=>ModelProvisionStage::Failed};
            events.emit(stage,result.as_ref().map(|m|m.revision()).unwrap_or(&s.revision),None,0,0,None,result.as_ref().err().map(ToString::to_string));result
        }).join().unwrap_or_else(|_|Err(Error::InvalidOutput("Hub provisioning worker panicked".into()))))
    }
    async fn run(&self,s:&HubModelSpec,o:&ProvisionOptions,e:&Events<'_>)->Result<InstalledModel>{
        validate(s)?;check(o,e.start)?;e.emit(ModelProvisionStage::CheckingCache,&s.revision,None,0,0,None,None);
        if !o.refresh {match self.cached_revision(s)?{Some(revision)=>match self.snapshot(s,&revision,o,e.start){Ok(model)=>{e.emit(ModelProvisionStage::CacheHit,&revision,None,0,0,None,None);return Ok(model)},Err(Error::Io(error))if error.kind()==std::io::ErrorKind::NotFound=>{},Err(error)=>return Err(error)},None=>{}}}
        if !o.allow_network{return Err(Error::Configuration("Hub files missing; explicit network permission required".into()));}
        let cache=Cache::new(self.root.clone());let mut builder=ApiBuilder::from_cache(cache.clone()).with_progress(false).with_max_files(1);
        if let Some(token)=o.token.clone().or_else(||std::env::var("HF_TOKEN").ok()){builder=builder.with_token(Some(token));}
        let api=builder.build().map_err(|_|Error::Configuration("cannot create Hugging Face Hub client".into()))?;
        e.emit(ModelProvisionStage::ResolvingRevision,&s.revision,None,0,0,None,None);
        let requested=api.repo(self.repo(s));let info=guarded(async{requested.info().await.map_err(|_|Error::InvalidOutput("Hub repository/revision request failed; check access and identity".into()))},o,e,&s.revision,None,0,0,None).await?;
        if !commit(&info.sha)||s.files.iter().any(|f|!info.siblings.iter().any(|entry|entry.rfilename==f.path)){return Err(Error::InvalidOutput("Hub resolved invalid revision or missing selected file".into()));}
        let repo=api.repo(Repo::with_revision(s.repo_id.clone(),RepoType::Model,info.sha.clone()));let mut done=0u64;let mut sizes=0u64;
        for (index,file) in s.files.iter().enumerate(){
            e.emit(ModelProvisionStage::FileStarted,&info.sha,Some(index+1),0,done,None,None);
            let progress=crate::model_progress::HubProgress::default();
            let path=guarded(async{repo.download_with_progress(&file.path,progress.clone()).await.map_err(|_|Error::InvalidOutput(format!("Hub download failed: {}",file.path)))},o,e,&info.sha,Some(index+1),done,sizes,Some(&progress)).await?;
            let (count,total)=progress.snapshot();e.emit(ModelProvisionStage::FileDownloaded,&info.sha,Some(index+1),count,done,total,None);
            e.emit(ModelProvisionStage::VerifyingFile,&info.sha,Some(index+1),count,done,total,None);
            let (hash,bytes)=hash_file(&path,o.max_file_bytes,o,e.start)?;if file.sha256.as_ref().is_some_and(|h|h.to_ascii_lowercase()!=hash){return Err(Error::InvalidOutput(format!("Hub checksum mismatch: {}",file.path)));}
            sizes=sizes.checked_add(bytes).ok_or_else(||Error::Limit("Hub total bytes".into()))?;if sizes>o.max_total_bytes{return Err(Error::Limit("Hub total bytes".into()));}done=done.saturating_add(count);
        }
        let installed=self.snapshot(s,&info.sha,o,e.start)?;cache.repo(self.repo(s)).create_ref(&info.sha)?;Ok(installed)
    }
}
pub fn texteller3_onnx_spec()->Result<HubModelSpec>{Ok(serde_json::from_str(include_str!("../model-specs/texteller3-onnx.json"))?)}
