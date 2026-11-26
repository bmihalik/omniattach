use clap::Parser;
use omniattach_cli::{
    Runtime,
    args::{Cli, Command, TableOutputFormat},
    dispatch,
    service::InspectionOnlyEngine,
};
#[test]
fn parses_explicit_selection_and_default_json() {
    let cli = Cli::try_parse_from([
        "omniattach",
        "pdf-table",
        "input.pdf",
        "--page",
        "2",
        "--candidate",
        "3",
    ])
    .unwrap();
    let Command::PdfTable(args) = cli.command else {
        panic!("wrong command");
    };
    assert_eq!(args.selection.page, 2);
    assert_eq!(args.candidate, 3);
    assert_eq!(args.output_format, TableOutputFormat::Json);
}
#[test]
fn requires_page_and_rejects_zero_page() {
    assert!(Cli::try_parse_from(["omniattach", "pdf-tables", "input.pdf"]).is_err());
    assert!(Cli::try_parse_from(["omniattach", "pdf-tables", "input.pdf", "--page", "0"]).is_err());
}
#[test]
fn zero_candidate_is_usage_error_without_stdout() {
    let cli = Cli::try_parse_from([
        "omniattach",
        "pdf-table",
        "missing.pdf",
        "--page",
        "1",
        "--candidate",
        "0",
    ])
    .unwrap();
    let mut out = Vec::new();
    let error = dispatch(&cli, &InspectionOnlyEngine, &Runtime::default(), &mut out).unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
    assert!(out.is_empty());
}
#[cfg(not(feature = "documents"))]
#[test]
fn missing_documents_is_explicitly_unsupported() {
    let cli =
        Cli::try_parse_from(["omniattach", "pdf-tables", "missing.pdf", "--page", "1"]).unwrap();
    let mut out = Vec::new();
    let error = dispatch(&cli, &InspectionOnlyEngine, &Runtime::default(), &mut out).unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED");
    assert!(out.is_empty());
}

/// Subprocess checks of the real binary: exit codes, clean stdout/stderr, caps.
#[cfg(feature = "documents")]
mod binary {
    use serde_json::Value;
    use std::process::{Command, Output};

    const GRID: &str = "50 100 m 250 100 l 50 130 m 250 130 l 50 160 m 250 160 l 50 100 m 50 160 l 150 100 m 150 160 l 250 100 m 250 160 l S\n";
    const TEXT: &str = "BT /F1 10 Tf 1 0 0 1 60 140 Tm (Name) Tj 1 0 0 1 160 140 Tm (Value) Tj 1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj ET\n";
    const EXPECTED: &str = "| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n";

    fn pdf(content: &str) -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 600] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        ];
        let mut bytes = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        bytes
    }

    struct Fixture {
        dir: tempfile::TempDir,
        path: std::path::PathBuf,
    }

    impl Fixture {
        fn new(content: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("table with spaces.pdf");
            std::fs::write(&path, pdf(content)).unwrap();
            Self { dir, path }
        }

        fn run(&self, command: &str, extra: &[&str]) -> Output {
            Command::new(env!("CARGO_BIN_EXE_omniattach"))
                .arg(command)
                .arg(&self.path)
                .args(["--page", "1"])
                .args(extra)
                .current_dir(self.dir.path())
                .output()
                .unwrap()
        }
    }

    fn success(output: &Output) -> String {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful output must keep stderr clean"
        );
        String::from_utf8(output.stdout.clone()).unwrap()
    }

    fn failure(output: &Output, code: &str, exit_code: i32) {
        assert_eq!(
            output.status.code(),
            Some(exit_code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stdout.is_empty(),
            "failures must not emit partial results"
        );
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["schema"], "omniattach-cli/error/0.1");
        assert_eq!(error["error"]["code"], code);
        assert_eq!(error["exit_code"], exit_code);
    }

    #[test]
    fn lists_and_converts_with_json_and_markdown_output() {
        let f = Fixture::new(&format!("{GRID}{TEXT}"));
        let listing: Value = serde_json::from_str(&success(&f.run("pdf-tables", &[]))).unwrap();
        assert_eq!(
            listing["schema_version"],
            "omniattach/pdf-table-discovery/0.2"
        );
        assert_eq!(listing["source"], f.path.to_string_lossy().as_ref());
        assert_eq!(listing["candidates"][0]["detector"], "stroked_grid");
        let json = success(&f.run("pdf-table", &["--candidate", "1"]));
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["markdown"], EXPECTED);
        assert_eq!(value["first_row_is_header"], true);
        let pretty = success(&f.run("pdf-table", &["--candidate", "1", "--pretty"]));
        assert!(pretty.contains("\n  \""));
        assert_eq!(serde_json::from_str::<Value>(&pretty).unwrap(), value);
        let markdown = success(&f.run(
            "pdf-table",
            &["--candidate", "1", "--output-format", "markdown"],
        ));
        assert_eq!(markdown, EXPECTED);
    }

    #[test]
    fn output_and_input_caps_fail_without_partial_output() {
        let f = Fixture::new(&format!("{GRID}{TEXT}"));
        for (command, extra) in [
            ("pdf-tables", vec![]),
            ("pdf-table", vec!["--candidate", "1"]),
            (
                "pdf-table",
                vec!["--candidate", "1", "--output-format", "markdown"],
            ),
        ] {
            let mut args = extra.clone();
            args.extend(["--max-output-bytes", "1"]);
            failure(&f.run(command, &args), "LIMIT_EXCEEDED", 6);
        }
        let exact = EXPECTED.len().to_string();
        let shorter = (EXPECTED.len() - 1).to_string();
        let md = [
            "--candidate",
            "1",
            "--output-format",
            "markdown",
            "--max-output-bytes",
        ];
        assert_eq!(
            success(&f.run("pdf-table", &[&md[..], &[exact.as_str()]].concat())),
            EXPECTED
        );
        failure(
            &f.run("pdf-table", &[&md[..], &[shorter.as_str()]].concat()),
            "LIMIT_EXCEEDED",
            6,
        );
        failure(
            &f.run("pdf-tables", &["--max-input-bytes", "1"]),
            "LIMIT_EXCEEDED",
            6,
        );
    }

    #[test]
    fn invalid_selection_and_unsupported_content_are_structured_errors() {
        let f = Fixture::new(&format!("{GRID}{TEXT}"));
        failure(
            &f.run("pdf-table", &["--candidate", "0"]),
            "INVALID_ARGUMENT",
            2,
        );
        failure(
            &f.run("pdf-table", &["--candidate", "99"]),
            "INVALID_INPUT",
            3,
        );
        failure(&f.run("pdf-table", &[]), "INVALID_ARGUMENT", 2);
        failure(
            &Fixture::new(GRID).run("pdf-table", &["--candidate", "1"]),
            "INVALID_INPUT",
            3,
        );
        let crossing = Fixture::new(&format!("{GRID}{}", TEXT.replace("60 110", "145 110")));
        failure(
            &crossing.run("pdf-table", &["--candidate", "1"]),
            "INVALID_INPUT",
            3,
        );
    }

    #[test]
    fn clipped_page_falls_back_to_the_layout_detector() {
        let clipped = Fixture::new(&format!("0 0 600 600 re W n\n{GRID}{TEXT}"));
        let listed: Value =
            serde_json::from_str(&success(&clipped.run("pdf-tables", &[]))).unwrap();
        assert_eq!(listed["candidates"][0]["detector"], "layout");
        let markdown = success(&clipped.run(
            "pdf-table",
            &["--candidate", "1", "--output-format", "markdown"],
        ));
        assert_eq!(markdown, EXPECTED);
    }

    /// Needs pdftoppm and tesseract on PATH and the scanned sample.
    #[test]
    #[ignore = "needs OMNIATTACH_TABLE_SAMPLES with 11-starcraft.pdf, pdftoppm and tesseract"]
    fn ocr_reads_a_scanned_table() {
        let dir = std::path::PathBuf::from(std::env::var("OMNIATTACH_TABLE_SAMPLES").unwrap());
        let output = Command::new(env!("CARGO_BIN_EXE_omniattach"))
            .arg("pdf-table")
            .arg(dir.join("11-starcraft.pdf"))
            .args([
                "--page",
                "1",
                "--candidate",
                "1",
                "--ocr",
                "--output-format",
                "markdown",
            ])
            .output()
            .unwrap();
        let markdown = success(&output);
        assert!(
            markdown.starts_with(
                "| Armor Name | Class | Damage Resistance | Defense Modifier | Cost |"
            )
        );
        assert!(markdown.contains("| Flak Armor | Heavy | 6 damage resistance | -2 | 500 |"));
    }

    #[test]
    fn commands_write_nothing() {
        let f = Fixture::new(&format!("{GRID}{TEXT}"));
        let before = std::fs::read(&f.path).unwrap();
        success(&f.run("pdf-tables", &[]));
        success(&f.run("pdf-table", &["--candidate", "1"]));
        let entries: Vec<_> = std::fs::read_dir(f.dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(std::fs::read(&f.path).unwrap(), before);
    }
}
