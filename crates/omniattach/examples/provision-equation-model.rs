//! Explicit provisioning API example, not part of conversion execution.
use omniattach::model_store::*;
use std::{fs, path::Path, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 && args.len() != 4 {
        return Err("usage: provision-equation-model ROOT --inspect | ROOT --download --allow-network --allow-external".into());
    }
    let spec = texteller3_onnx_spec()?;
    let store = ModelStore::new(Path::new(&args[0]))?;
    let mut options = ProvisionOptions::default();
    options.timeout = Duration::from_secs(1800);
    if args.len() == 2 && args[1] == "--inspect" {
        println!(
            "{}",
            serde_json::to_string_pretty(&store.inspect(&spec, &options)?)?
        );
        return Ok(());
    }
    if args.len() != 4
        || args[1] != "--download"
        || args[2] != "--allow-network"
        || args[3] != "--allow-external"
    {
        return Err("download requires both explicit permissions".into());
    }
    options.allow_network = true;
    options.allow_external = true;
    let installed = store.provision(&spec, &options, &CurlModelFetcher::default())?;
    println!("{}", serde_json::to_string_pretty(&installed)?);
    // Worker configuration generation is deliberately separate: pass explicit
    // local executable/runtime paths to installed.equation_worker_config_json.
    let _ = fs::metadata(installed.directory())?;
    Ok(())
}
