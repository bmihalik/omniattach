//! Explicit provisioning with stderr progress and final JSON-only stdout.
use omniattach::{model_store::*,model_progress::*};
use std::{fs,io::{self,Write},path::Path,sync::Mutex,time::Duration};
fn progress_mode(args:&mut Vec<String>)->Result<String,Box<dyn std::error::Error>> {
    if args.iter().filter(|s|s.as_str()=="--progress").count()>1{return Err("duplicate --progress".into());}
    let mode=if let Some(index)=args.iter().position(|s|s=="--progress") {
        if index+1>=args.len(){return Err("--progress needs none, text or ndjson".into());}
        let mode=args.remove(index+1);args.remove(index);mode
    }else{"text".into()};
    if !matches!(mode.as_str(),"none"|"text"|"ndjson"){return Err("--progress must be none, text or ndjson".into());}Ok(mode)
}
fn render(event:&ModelProgressEvent,mode:&str,writer:&mut dyn Write)->io::Result<()> {
    if mode=="ndjson" {serde_json::to_writer(&mut *writer,event).map_err(io::Error::other)?;writer.write_all(b"\n")}
    else if mode=="text" {
        write!(writer,"{:?}",event.stage)?;
        if let Some(file)=&event.file {write!(writer," [{}/{}] {}: {:.2} MiB written",event.file_index.unwrap_or(0),event.file_count,file,event.file_bytes as f64/1048576.0)?;}
        if let Some(message)=&event.message{write!(writer," — {message}")?;}
        writeln!(writer)
    }else{Ok(())}
}
fn run()->Result<(),Box<dyn std::error::Error>> {
    let mut args:Vec<String>=std::env::args().skip(1).collect();let mode=progress_mode(&mut args)?;
    if args.len()!=2 && args.len()!=4{return Err("usage: provision-equation-model ROOT --inspect | ROOT --download --allow-network --allow-external [--progress none|text|ndjson]".into());}
    let spec=texteller3_onnx_spec()?;let store=ModelStore::new(Path::new(&args[0]))?;
    let mut options=ProvisionOptions::default();options.timeout=Duration::from_secs(1800);
    if args.len()==2 && args[1]=="--inspect"{println!("{}",serde_json::to_string_pretty(&store.inspect(&spec,&options)?)?);return Ok(());}
    if args.len()!=4 || args[1]!="--download" || args[2]!="--allow-network" || args[3]!="--allow-external" {return Err("download requires both explicit permissions".into());}
    options.allow_network=true;options.allow_external=true;
    let last_tick=Mutex::new(None::<u64>);
    let observer=|event:&ModelProgressEvent|{
        if mode=="text" && event.stage==ModelProvisionStage::Downloading {
            if let Ok(mut tick)=last_tick.lock(){if tick.is_some_and(|last|event.elapsed_ms.saturating_sub(last)<1000){return;}*tick=Some(event.elapsed_ms);}
        }
        let _=render(event,&mode,&mut io::stderr().lock());
    };
    let fetcher=CurlModelFetcher::default();
    let installed=if mode=="none"{store.provision(&spec,&options,&fetcher)?}else{store.provision_with_progress(&spec,&options,&fetcher,&observer)?};
    println!("{}",serde_json::to_string_pretty(&installed)?);
    let _=fs::metadata(installed.directory())?;Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn modes_are_explicit_and_validated(){
        let mut args=vec!["root".into(),"--inspect".into(),"--progress".into(),"ndjson".into()];assert_eq!(progress_mode(&mut args).unwrap(),"ndjson");assert_eq!(args.len(),2);
        let mut bad=vec!["--progress".into(),"invalid".into()];assert!(progress_mode(&mut bad).is_err());
    }
    #[test]fn machine_progress_is_one_json_line_and_none_is_silent(){
        let event=ModelProgressEvent{schema_version:"omniattach/model-progress/0.1",model_id:"test".into(),revision:"a".repeat(40),stage:ModelProvisionStage::Downloading,file:Some("weights".into()),file_index:Some(1),file_count:1,file_bytes:12,downloaded_bytes:12,total_bytes:None,elapsed_ms:1,message:None};
        let mut bytes=Vec::new();render(&event,"ndjson",&mut bytes).unwrap();assert_eq!(bytes.iter().filter(|b|**b==b'\n').count(),1);
        let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(value["total_bytes"].is_null());assert_eq!(value["file_bytes"],12);
        bytes.clear();render(&event,"none",&mut bytes).unwrap();assert!(bytes.is_empty());
    }
}

fn main()->std::process::ExitCode {
    match run() {
        Ok(())=>std::process::ExitCode::SUCCESS,
        Err(error)=>{
            let mut stderr=io::stderr().lock();
            let value=serde_json::json!({"schema_version":"omniattach/provision-error/0.1","error":error.to_string()});
            let _=serde_json::to_writer(&mut stderr,&value);let _=writeln!(stderr);
            std::process::ExitCode::FAILURE
        }
    }
}
