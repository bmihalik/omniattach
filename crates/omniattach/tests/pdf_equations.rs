#![cfg(all(feature = "documents", feature = "backends"))]
use omniattach::{*, pdf_equations_adapter::*, backends::{CommandSpec, ProcessOutput, RunOptions, Runner}};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,path::PathBuf,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::Duration};
fn fixture() -> Vec<u8> {
    let content=b"q 10 0 0 10 0 0 cm /Im Do Q q 10 0 0 10 20 0 cm /Im Do Q";
    let objects=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 100] >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im 4 0 R >> >> /Contents 5 0 R >>".to_vec(),b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\nABC\nendstream".to_vec(),[format!("<< /Length {} >>\nstream\n",content.len()).into_bytes(),content.to_vec(),b"\nendstream".to_vec()].concat()];
    let mut bytes=b"%PDF-1.5\n".to_vec();let mut offsets=vec![0];
    for (i,object) in objects.iter().enumerate(){offsets.push(bytes.len());bytes.extend(format!("{} 0 obj\n",i+1).as_bytes());bytes.extend(object);bytes.extend(b"\nendobj\n");}
    let xref=bytes.len();bytes.extend(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1){bytes.extend(format!("{offset:010} 00000 n \n").as_bytes());}
    bytes.extend(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());bytes
}
struct Fake { calls:AtomicUsize, mode:u8 }
impl Runner for Fake {
    fn run(&self,spec:&CommandSpec,options:&RunOptions)->backends::Result<ProcessOutput>{
        self.calls.fetch_add(1,Ordering::SeqCst);assert_eq!(options.max_stdout_bytes,8*1024*1024);assert!(!spec.inherit_env);
        if self.mode==2{return Err(backends::Error::Timeout(1));}
        let path=PathBuf::from(spec.args.last().unwrap());assert_eq!(spec.args[spec.args.len()-2],"--request-file");
        let request:Value=serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(request["schema_version"],"omniattach/formula-worker/0.1");
        let mut rows:Vec<_>=request["images"].as_array().unwrap().iter().map(|i|{assert_eq!(i.as_object().unwrap().len(),2);assert!(std::path::Path::new(i["path"].as_str().unwrap()).is_file());json!({"id":i["id"],"status":"ok","latex":"\\[x=1\\]"})}).collect();
        if self.mode==1{rows.pop();}
        Ok(ProcessOutput{stdout:serde_json::to_vec(&json!({"results":rows,"metadata":{"fake":true}})).unwrap(),stderr:vec![],elapsed:Duration::from_millis(1)})
    }
}
fn setup(mode:u8)->(tempfile::TempDir,OmniAttach,ConversionRequest,Arc<Fake>){
    let temp=tempfile::tempdir().unwrap();let pdf=temp.path().join("a.pdf");let bytes=fixture();fs::write(&pdf,&bytes).unwrap();
    let executable=temp.path().join("worker");fs::write(&executable,b"fake").unwrap();
    let config=EquationWorkerConfig{argv:vec![executable.to_string_lossy().into_owned()],env:Default::default(),timeout_ms:1000,artifacts:vec![EquationWorkerArtifact{path:executable.to_string_lossy().into_owned(),sha256:format!("{:x}",Sha256::digest(b"fake"))}]};
    let runner=Arc::new(Fake{calls:AtomicUsize::new(0),mode});
    let engine=OmniAttach::builder().with_document_defaults().unwrap().with_pdf_equation_worker_runner(config,runner.clone()).unwrap().build();
    let hash=format!("{:x}",Sha256::digest(&bytes));let request=ConversionRequest::new(pdf,Goal::Custom("pdf-equations".into())).with_options(json!({"block_ids":[format!("{hash}:b1"),format!("{hash}:b2")]}));
    (temp,engine,request,runner)
}
#[test]fn permission_and_default_isolation(){
    let (_temp,engine,request,runner)=setup(0);
    assert!(matches!(engine.convert(&request,&FacadeContext::default()),Err(Error::ExternalDisabled)));
    let normal=ConversionRequest::new(&request.path,Goal::Markdown);let _=engine.convert(&normal,&FacadeContext::default());assert_eq!(runner.calls.load(Ordering::SeqCst),0);
}
#[test]fn batch_results_and_durable_exports(){
    let (temp,engine,request,runner)=setup(0);let bundle=engine.convert(&request,&FacadeContext::default().with_external(true)).unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst),1);assert_eq!(bundle.manifest().route.id,"pdf-equations");
    assert_eq!(bundle.plain_text().matches("x=1").count(),2);assert!(bundle.payload("omniattach/pdf-equations/0.1").is_some());
    assert!(bundle.manifest().artifacts.iter().any(|a|a.path=="equation-worker.json"));
    let out=temp.path().join("export");bundle.export(&out).unwrap();drop(bundle);assert!(out.join("document.md").is_file());assert!(out.join("manifest.json").is_file());
}
#[test]fn malformed_response_and_timeout_fail(){
    for mode in [1,2]{let (_temp,engine,request,_)=setup(mode);let error=engine.convert(&request,&FacadeContext::default().with_external(true)).unwrap_err();if mode==2{assert!(matches!(error,Error::Deadline));}else{assert!(matches!(error,Error::InvalidOutput(_)));}}
}
#[test]fn options_are_not_executable_configuration(){
    let (_temp,engine,mut request,runner)=setup(0);request.options=json!({"block_ids":[],"argv":["malicious"]});
    assert!(matches!(engine.convert(&request,&FacadeContext::default().with_external(true)),Err(Error::Configuration(_))));assert_eq!(runner.calls.load(Ordering::SeqCst),0);
}
#[test]fn empty_and_duplicate_selections_do_not_spawn(){
    let (_temp,engine,mut request,runner)=setup(0);request.options=json!({"block_ids":[]});
    assert!(engine.convert(&request,&FacadeContext::default().with_external(true)).is_ok());assert_eq!(runner.calls.load(Ordering::SeqCst),0);
    request.options=json!({"block_ids":["same","same"]});assert!(engine.convert(&request,&FacadeContext::default().with_external(true)).is_err());assert_eq!(runner.calls.load(Ordering::SeqCst),0);
}
