#![cfg(all(feature = "backends", feature = "documents"))]
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_omniattach"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn external_backends_expose_legacy_office_pipeline_routes() {
    let o = run(&["--external-backends", "capabilities"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.contains("legacy-doc-rtf"));
    assert!(out.contains("legacy-ppt"));
}

#[test]
fn legacy_office_routes_are_absent_without_external_backends() {
    let o = run(&["capabilities"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(!out.contains("legacy-doc-rtf"));
    assert!(!out.contains("legacy-ppt"));
}
