#![cfg(feature = "documents")]
use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};

// Structural JPEG stream: image decoding is covered separately by fixture validation.
fn fixture(filter: &str) -> Vec<u8> {
    let content = "BT /F1 12 Tf (Before) Tj ET /Im Do BT /F1 12 Tf (After) Tj ET /Im Do";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Im 5 0 R >> >> /Contents 6 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
        format!("<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /{filter} /Length 4 >>\nstream\nABCD\nendstream"),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut bytes = b"%PDF-1.5\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    bytes
}
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
#[test]
fn opt_in_route_exports_relocatable_declared_images_without_external_permission() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.pdf");
    let out = dir.path().join("export");
    fs::write(&input, fixture("DCTDecode")).unwrap();
    let p = input.to_str().unwrap();
    let default = success(&run(&["plan", p, "--goal", "markdown"]));
    assert_eq!(default["plan"]["route"]["id"], "documents");
    let plan = success(&run(&[
        "plan",
        p,
        "--goal",
        "custom:pdf-images",
        "--route",
        "pdf-images",
    ]));
    assert_eq!(plan["plan"]["route"]["execution"], "in_process");
    let converted = success(&run(&[
        "convert",
        p,
        "--goal",
        "custom:pdf-images",
        "--route",
        "pdf-images",
        "--output",
        out.to_str().unwrap(),
    ]));
    assert_eq!(converted["manifest"]["route"]["id"], "pdf-images");
    let manifest: Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["artifacts"].as_array().unwrap().len(), 3);
    assert!(
        manifest["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "LFE_IMAGE_EXPORT_EXPERIMENTAL")
    );
    assert!(
        manifest["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["schema"] == "omniattach-documents/0.2")
    );
    let doc: Value = serde_json::from_slice(&fs::read(out.join("document.json")).unwrap()).unwrap();
    let image_path = doc["assets"][0]["filename"].as_str().unwrap();
    assert!(image_path.starts_with("images/book-"));
    assert_eq!(fs::read(out.join(image_path)).unwrap(), b"ABCD");
    let md = fs::read_to_string(out.join("document.md")).unwrap();
    assert_eq!(md.matches(&format!("]({image_path})")).count(), 2);
    assert!(
        manifest["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["path"] == image_path && a["media_type"] == "image/jpeg")
    );
    let moved = dir.path().join("moved");
    fs::rename(&out, &moved).unwrap();
    assert_eq!(fs::read(moved.join(image_path)).unwrap(), b"ABCD");
}
#[test]
fn options_and_export_budgets_fail_before_durable_export() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.pdf");
    fs::write(&input, fixture("DCTDecode")).unwrap();
    for (index, extra) in [
        vec!["--options-json", "{\"ocr\":true}"],
        vec!["--dehyphenate-max-tokens", "0"],
        vec!["--max-export-files", "2"],
        vec!["--max-export-bytes", "512"],
    ]
    .iter()
    .enumerate()
    {
        let out = dir.path().join(format!("bad-{index}"));
        let mut args = vec![
            "convert",
            input.to_str().unwrap(),
            "--goal",
            "custom:pdf-images",
            "--output",
            out.to_str().unwrap(),
        ];
        args.extend(extra.iter().copied());
        let result = run(&args);
        assert!(!result.status.success());
        assert!(!out.exists());
    }
}
#[test]
fn unsupported_filter_is_skipped_and_existing_export_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.pdf");
    let out = dir.path().join("export");
    fs::write(&input, fixture("FlateDecode")).unwrap();
    let args = [
        "convert",
        input.to_str().unwrap(),
        "--goal",
        "custom:pdf-images",
        "--output",
        out.to_str().unwrap(),
    ];
    let value = success(&run(&args));
    assert!(
        value["manifest"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "PDF_IMAGE_SKIPPED")
    );
    let original = fs::read(out.join("document.md")).unwrap();
    assert!(!String::from_utf8_lossy(&original).contains("!["));
    assert!(!run(&args).status.success());
    assert_eq!(fs::read(out.join("document.md")).unwrap(), original);
}
