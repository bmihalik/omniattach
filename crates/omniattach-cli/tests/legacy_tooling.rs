#![cfg(all(feature = "backends", feature = "documents"))]
use std::process::Command;

#[test]
fn legacy_request_reports_tool_unavailable_when_libreoffice_is_missing() {
    let d = tempfile::tempdir().unwrap();
    let doc = d.path().join("legacy.doc");
    std::fs::write(&doc, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1rest").unwrap();
    let empty_bin = d.path().join("empty-bin");
    std::fs::create_dir(&empty_bin).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_omniattach"))
        .env("PATH", &empty_bin)
        .args([
            "--external-backends",
            "plan",
            doc.to_str().unwrap(),
            "--allow-external",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(8));
    let err: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(err["error"]["code"], "TOOL_UNAVAILABLE");
}

#[test]
fn unrelated_formats_do_not_probe_libreoffice() {
    let d = tempfile::tempdir().unwrap();
    let txt = d.path().join("a.txt");
    std::fs::write(&txt, "hi").unwrap();
    let empty_bin = d.path().join("empty-bin");
    std::fs::create_dir(&empty_bin).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_omniattach"))
        .env("PATH", &empty_bin)
        .args([
            "--external-backends",
            "plan",
            txt.to_str().unwrap(),
            "--allow-external",
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
