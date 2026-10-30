#![cfg(feature = "documents")]
use serde_json::Value;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omniattach"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fixture(version: u32, encrypted: bool) -> Vec<u8> {
    let text = b"<p>Hello ebook CLI</p>";
    let mut header = vec![0u8; 280];
    header[..2].copy_from_slice(&1u16.to_be_bytes());
    header[4..8].copy_from_slice(&(text.len() as u32).to_be_bytes());
    header[8..10].copy_from_slice(&1u16.to_be_bytes());
    header[12..14].copy_from_slice(&u16::from(encrypted).to_be_bytes());
    header[16..20].copy_from_slice(b"MOBI");
    for (offset, value) in [(20, 264u32), (28, 65001), (36, version)] {
        header[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
    let mut bytes = vec![0; 96];
    bytes[60..68].copy_from_slice(b"BOOKMOBI");
    bytes[76..78].copy_from_slice(&2u16.to_be_bytes());
    bytes[78..82].copy_from_slice(&96u32.to_be_bytes());
    bytes[86..90].copy_from_slice(&376u32.to_be_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(text);
    bytes
}
#[test]
fn probe_plan_and_markdown_export_work_without_external_permission() {
    for (extension, version) in [("mobi", 6), ("azw3", 8)] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join(format!("book.{extension}"));
        let output = dir.path().join("export");
        std::fs::write(&input, fixture(version, false)).unwrap();
        let p = input.to_str().unwrap();
        let probe = success(&run(&["probe", p]));
        assert_eq!(probe["detection"]["format"], extension);
        assert_eq!(probe["detection"]["basis"], "extension");
        let plan = success(&run(&["plan", p, "--goal", "markdown"]));
        assert_eq!(plan["plan"]["route"]["id"], "documents");
        let converted = success(&run(&[
            "convert",
            p,
            "--goal",
            "markdown",
            "--output",
            output.to_str().unwrap(),
        ]));
        assert_eq!(converted["manifest"]["route"]["id"], "documents");
        assert!(output.join("manifest.json").is_file());
        assert!(
            std::fs::read_to_string(output.join("document.md"))
                .unwrap()
                .contains("Hello ebook CLI")
        );
        let document: Value =
            serde_json::from_slice(&std::fs::read(output.join("document.json")).unwrap()).unwrap();
        assert_eq!(document["format"], extension);
        if extension == "azw3" {
            assert!(
                document["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["code"] == "AZW3_EXPERIMENTAL_PROJECTION")
            );
        }
    }
}
#[test]
fn explicit_format_accepts_renamed_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.bin");
    std::fs::write(&input, fixture(8, false)).unwrap();
    let output = dir.path().join("export");
    success(&run(&[
        "convert",
        input.to_str().unwrap(),
        "--format",
        "azw3",
        "--output",
        output.to_str().unwrap(),
    ]));
    assert!(output.join("document.json").is_file());
}
#[test]
fn malformed_and_encrypted_ebooks_fail_without_export() {
    for data in [b"not an ebook".to_vec(), fixture(8, true)] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("book.azw3");
        let output = dir.path().join("export");
        std::fs::write(&input, data).unwrap();
        let result = run(&[
            "convert",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["error"]["code"], "CONVERSION_FAILED");
        assert!(!output.exists());
    }
}
#[test]
fn capabilities_advertise_ebook_formats() {
    let value = success(&run(&["capabilities"]));
    let routes = value["registered"]["routes"].as_array().unwrap();
    let route = routes.iter().find(|r| r["id"] == "documents").unwrap();
    let serialized = serde_json::to_string(route).unwrap();
    assert!(serialized.contains("mobi"));
    assert!(serialized.contains("azw3"));
}
