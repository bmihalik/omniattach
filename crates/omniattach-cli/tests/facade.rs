//! Actual sibling facade tests. NOT compiled or run by the isolated inspection harness.
#![cfg(feature = "facade")]
use serde_json::Value;
use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_omniattach"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn actual_facade_markdown_conversion_exports_before_json_is_printed() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.txt");
    let output = d.path().join("output");
    std::fs::write(&input, "árvíz\n").unwrap();
    let result = run(&[
        "convert",
        input.to_str().unwrap(),
        "--goal",
        "markdown",
        "--output",
        output.to_str().unwrap(),
        "--progress",
        "ndjson",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let v: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(v["manifest"]["route"]["id"], "portable-text");
    assert!(output.join("manifest.json").is_file());
    assert!(output.join("document.md").is_file());
    for line in String::from_utf8(result.stderr).unwrap().lines() {
        let _: Value = serde_json::from_str(line).unwrap();
    }
}
#[test]
fn actual_facade_routes_office_zip_to_documents_at_plan() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fake.docx");
    std::fs::write(&p, b"PK\x03\x04").unwrap();
    let o = run(&["plan", p.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["plan"]["route"]["id"],
        "documents"
    );
}
#[test]
fn actual_facade_malformed_office_package_fails_without_export() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fake.docx");
    std::fs::write(&p, b"PK\x03\x04").unwrap();
    let output = d.path().join("out");
    let o = run(&["convert", p.to_str().unwrap(), "--output", output.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stderr).unwrap()["error"]["code"],
        "CONVERSION_FAILED"
    );
    assert!(!output.exists());
}
#[test]
fn xml_without_mapping_fails_with_clear_invalid_argument() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("data.xml");
    std::fs::write(&p, b"<measurements/>").unwrap();
    let output = d.path().join("out");
    let o = run(&["convert", p.to_str().unwrap(), "--output", output.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(e["error"]["code"], "INVALID_ARGUMENT");
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("explicit column mapping")
    );
    assert!(!output.exists());
}
#[test]
fn csv_tables_goal_exports_table_csv_through_documents_route() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("measurements.csv");
    std::fs::write(&p, "sample,value\n001,1.2300\n").unwrap();
    let output = d.path().join("out");
    let o = run(&[
        "convert",
        p.to_str().unwrap(),
        "--goal",
        "tables",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["manifest"]["route"]["id"],
        "documents"
    );
    let csv = std::fs::read_to_string(output.join("table-0001.csv")).unwrap();
    assert!(csv.contains("sample,value") && csv.contains("001,1.2300"));
}
#[test]
fn actual_facade_plan_does_not_create_output_or_run_conversion() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.txt");
    std::fs::write(&p, "hi").unwrap();
    let o = run(&["plan", p.to_str().unwrap(), "--goal", "markdown"]);
    assert!(o.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap()["plan"]["route"]["id"],
        "portable-text"
    );
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
}
#[test]
fn actual_facade_immediate_deadline_maps_to_124() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.txt");
    std::fs::write(&p, "hi").unwrap();
    let output = d.path().join("out");
    let o = run(&[
        "convert",
        p.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--timeout-ms",
        "0",
    ]);
    assert_eq!(o.status.code(), Some(124));
    assert!(!output.exists());
}
#[cfg(feature = "backends")]
#[test]
fn external_route_permission_is_distinct_from_registration() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.pdf");
    std::fs::write(&p, b"%PDF-1.4").unwrap();
    let o = run(&[
        "--external-backends",
        "plan",
        p.to_str().unwrap(),
        "--route",
        "poppler",
    ]);
    assert_eq!(o.status.code(), Some(5));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stderr).unwrap()["error"]["code"],
        "EXTERNAL_DISABLED"
    );
}
