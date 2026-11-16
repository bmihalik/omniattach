//! Hub-only pre-download; stdout is final JSON, progress/errors use stderr.
use omniattach::{model_store::*,model_progress::*};
use std::{io::{self,Write},time::Duration};
fn run()->Result<(),Box<dyn std::error::Error>>{
    let mut args:Vec<String>=std::env::args().skip(1).collect();let mut mode="text".to_string();let mut repo=None;let mut revision=None;let mut files=Vec::new();let mut index=0;let mut refresh=false;
    while index<args.len(){
        if args[index]=="--refresh"{refresh=true;args.remove(index);continue;}
        if matches!(args[index].as_str(),"--progress"|"--repo"|"--revision"|"--file"){
            if index+1>=args.len(){return Err("option requires a value".into());}
            let key=args.remove(index);let value=args.remove(index);match key.as_str(){"--progress"=>mode=value,"--repo"=>repo=Some(value),"--revision"=>revision=Some(value),_=>files.push(value)}
        }else{index+=1;}
    }
    if !matches!(mode.as_str(),"text"|"ndjson"|"none")||args.is_empty(){return Err("usage: provision-equation-model CACHE --inspect | CACHE --download --allow-network [--repo ID --revision REF --file PATH...] [--progress text|ndjson|none]".into());}
    let mut spec=texteller3_onnx_spec()?;
    if let Some(repo_id)=repo{if files.is_empty(){return Err("custom Hub repo requires explicit --file selections".into());}spec=HubModelSpec{repo_id,revision:"main".into(),files:Vec::new()};}
    if let Some(revision)=revision{spec.revision=revision;}
    if !files.is_empty(){spec.files=files.into_iter().map(|path|HubFile{path,sha256:None}).collect();}
    let store=ModelStore::new(&args[0])?;let mut options=ProvisionOptions::default();options.timeout=Duration::from_secs(1800);options.refresh=refresh;
    if args.len()==2&&args[1]=="--inspect"{println!("{}",serde_json::to_string_pretty(&store.inspect(&spec,&options)?)?);return Ok(());}
    if args.len()!=3||args[1]!="--download"||args[2]!="--allow-network"{return Err("Hub download requires --download --allow-network; --allow-external is no longer a provisioning option".into());}
    options.allow_network=true;
    let observer=|event:&ModelProgressEvent|{
        let mut err=io::stderr().lock();if mode=="ndjson"{let _=serde_json::to_writer(&mut err,event);let _=writeln!(err);}
        else if mode=="text"{let _=write!(err,"{:?}",event.stage);if let Some(file)=&event.file{let _=write!(err," [{}/{}] {}: {} bytes",event.file_index.unwrap_or(0),event.file_count,file,event.file_bytes);if let Some(total)=event.total_bytes{let _=write!(err," / {total}");}}if let Some(message)=&event.message{let _=write!(err," — {message}");}let _=writeln!(err);}
    };
    let installed=store.provision_with_progress(&spec,&options,&observer)?;println!("{}",serde_json::to_string_pretty(&installed)?);Ok(())
}
fn main()->std::process::ExitCode{match run(){Ok(())=>std::process::ExitCode::SUCCESS,Err(error)=>{let _=writeln!(io::stderr().lock(),"{}",serde_json::json!({"schema_version":"omniattach/provision-error/0.2","error":error.to_string()}));std::process::ExitCode::FAILURE}}}
