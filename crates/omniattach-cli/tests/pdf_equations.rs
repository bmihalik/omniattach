#![cfg(all(feature="facade",feature="documents",feature="backends"))]
#[test]
fn configured_cli_capabilities_register_without_executing_worker(){
    let path=std::env::temp_dir().join(format!("omniattach-equations-{}.json",std::process::id()));
    let config=serde_json::json!({"argv":["/not-installed/equation-worker"],"artifacts":[{"path":"/not-installed/equation-worker","sha256":"0".repeat(64)}]});
    use std::io::Write;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();file.write_all(config.to_string().as_bytes()).unwrap();drop(file);
    let output=std::process::Command::new(env!("CARGO_BIN_EXE_omniattach")).env("OMNIATTACH_PDF_EQUATION_WORKER_CONFIG",&path).args(["--external-backends","capabilities"]).output().unwrap();
    std::fs::remove_file(path).unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let response:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();assert!(response.to_string().contains("pdf-equations"));
}
