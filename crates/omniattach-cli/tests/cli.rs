use clap::Parser;
use omniattach_cli::{Result, Runtime, args::*, dispatch, service::*};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command as Process,
    sync::Mutex,
};
fn cli(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("omniattach").chain(args.iter().copied())).unwrap()
}
fn file(name: &str, contents: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join(name);
    std::fs::write(&p, contents).unwrap();
    (d, p)
}
fn run(args: &[&str]) -> std::process::Output {
    Process::new(env!("CARGO_BIN_EXE_omniattach"))
        .args(args)
        .output()
        .unwrap()
}
fn parsed(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn probe_xml(xml: &str) -> Result<omniattach_cli::xml::XmlReport> {
    let (_d, p) = file("data.xml", xml.as_bytes());
    let c = cli(&["xml-probe", p.to_str().unwrap()]);
    let Command::XmlProbe(args) = c.command else {
        unreachable!()
    };
    omniattach_cli::xml::xml_probe(&args, &Runtime::default())
}
#[derive(Default)]
struct RecordingEngine {
    requests: Mutex<Vec<EngineRequest>>,
}
impl Engine for RecordingEngine {
    fn capabilities(&self) -> Result<Value> {
        Ok(json!({"routes":[{"id":"test-transport-only"}]}))
    }
    fn plan(&self, request: &EngineRequest) -> Result<Value> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(json!({"goal":request.goal,"route":request.route}))
    }
    fn convert(
        &self,
        request: &EngineRequest,
        output: &Path,
        _: ProgressMode,
    ) -> Result<ConversionOutcome> {
        self.requests.lock().unwrap().push(request.clone());
        std::fs::create_dir(output)?;
        let manifest = json!({"source":"transport-test","artifacts":[]});
        std::fs::write(output.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
        Ok(ConversionOutcome {
            manifest,
            text: "preserved text".into(),
        })
    }
}
#[test]
fn parser_accepts_all_subcommands() {
    for args in [
        vec!["capabilities"],
        vec!["probe", "a.pdf"],
        vec!["xml-probe", "a.xml"],
        vec!["plan", "a.txt"],
        vec!["convert", "a.txt", "--output", "new-out"],
    ] {
        assert!(Cli::try_parse_from(std::iter::once("omniattach").chain(args)).is_ok());
    }
}
#[test]
fn parser_enforces_new_output_argument() {
    assert!(Cli::try_parse_from(["omniattach", "convert", "input.txt"]).is_err());
}
#[test]
fn help_is_human_readable_and_successful() {
    let o = run(&["--help"]);
    assert!(o.status.success());
    assert!(String::from_utf8(o.stdout).unwrap().contains("xml-probe"));
    assert!(o.stderr.is_empty());
}
#[test]
fn version_is_successful() {
    let o = run(&["--version"]);
    assert!(o.status.success());
    assert!(
        String::from_utf8(o.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
}
#[test]
fn parser_errors_are_json_on_stderr_only() {
    let o = run(&["--not-an-option"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    let e: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(e["error"]["code"], "INVALID_ARGUMENT");
}
#[test]
fn capabilities_are_compact_json_without_progress_noise() {
    let o = run(&["capabilities"]);
    let v = parsed(&o);
    assert_eq!(v["schema"], "omniattach-cli/capabilities/0.1");
    assert_eq!(v["health_checked"], false);
    assert!(o.stderr.is_empty());
    assert_eq!(o.stdout.iter().filter(|b| **b == b'\n').count(), 1);
}
#[test]
fn pretty_flag_works_after_subcommand() {
    let o = run(&["capabilities", "--pretty"]);
    parsed(&o);
    assert!(o.stdout.iter().filter(|b| **b == b'\n').count() > 1);
}
#[test]
fn probe_signature_overrides_extension() {
    let (_d, p) = file("wrong.txt", b"%PDF-1.4");
    let v = parsed(&run(&["probe", p.to_str().unwrap()]));
    assert_eq!(v["detection"]["format"], "pdf");
    assert_eq!(v["detection"]["basis"], "signature");
    assert_eq!(v["detection"]["warnings"].as_array().unwrap().len(), 1);
}
#[test]
fn probe_zip_family_is_explicitly_extension_hint() {
    let (_d, p) = file("file.docx", b"PK\x03\x04");
    let v = parsed(&run(&["probe", p.to_str().unwrap()]));
    assert_eq!(v["detection"]["format"], "docx");
    assert_eq!(v["detection"]["basis"], "extension");
    assert_eq!(v["conversion_support_checked"], false);
}
#[test]
fn probe_unknown_is_not_assumed_text() {
    let (_d, p) = file("file.bin", b"plain text");
    let v = parsed(&run(&["probe", p.to_str().unwrap()]));
    assert_eq!(v["detection"]["format"], "unknown");
}
#[test]
fn probe_caps_and_missing_files_have_stable_exit_codes() {
    let (_d, p) = file("a.txt", b"123");
    let o = run(&["probe", p.to_str().unwrap(), "--max-input-bytes", "2"]);
    assert_eq!(o.status.code(), Some(6));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stderr).unwrap()["error"]["code"],
        "LIMIT_EXCEEDED"
    );
    let o = run(&["probe", "/nonexistent/omniattach/input"]);
    assert_eq!(o.status.code(), Some(3));
    assert!(o.stdout.is_empty());
}
#[test]
fn probe_directory_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let o = run(&["probe", d.path().to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(3));
}
#[test]
fn whitespace_unicode_paths_are_passed_as_arguments_not_shell() {
    let (_d, p) = file("árvíz $(echo nope).txt", b"ok");
    let v = parsed(&run(&["probe", p.to_str().unwrap()]));
    assert_eq!(v["bytes"], 2);
}
#[test]
fn xml_counts_repeated_children_per_parent() {
    let r = probe_xml("<root><group><item/><item/></group><group><item/></group></root>").unwrap();
    let item = r
        .paths
        .iter()
        .find(|p| p.elements.last().unwrap().local == "item")
        .unwrap();
    assert_eq!(item.occurrences, 3);
    assert_eq!(item.max_occurrences_per_parent, 2);
    assert_eq!(r.elements, 6);
    assert_eq!(r.max_depth, 3);
}
#[test]
fn xml_prefix_aliases_share_expanded_path_and_default_does_not_apply_to_attribute() {
    let r = probe_xml("<r xmlns='urn:r' xmlns:a='urn:item' xmlns:b='urn:item'><a:item key='x' a:key='y'/><b:item/></r>").unwrap();
    let item = r
        .paths
        .iter()
        .find(|p| p.elements.last().unwrap().local == "item")
        .unwrap();
    assert_eq!(item.occurrences, 2);
    assert_eq!(item.elements[0].namespace.as_deref(), Some("urn:r"));
    assert_eq!(item.elements[1].namespace.as_deref(), Some("urn:item"));
    assert!(
        item.attributes
            .iter()
            .any(|a| a.name.namespace.is_none() && a.name.local == "key")
    );
    assert!(
        item.attributes
            .iter()
            .any(|a| a.name.namespace.as_deref() == Some("urn:item") && a.name.local == "key")
    );
}
#[test]
fn xml_namespace_rebinding_produces_distinct_paths() {
    let r = probe_xml("<r xmlns:p='one'><p:x/><g xmlns:p='two'><p:x/></g></r>").unwrap();
    let xs: Vec<_> = r
        .paths
        .iter()
        .filter(|p| p.elements.last().unwrap().local == "x")
        .collect();
    assert_eq!(xs.len(), 2);
    assert_ne!(
        xs[0].elements.last().unwrap().namespace,
        xs[1].elements.last().unwrap().namespace
    );
}
#[test]
fn xml_probe_never_includes_text_or_attribute_values() {
    let r = probe_xml("<r secret='private-attribute-value'>private-text-value<a><![CDATA[private-cdata-value]]></a></r>").unwrap();
    let json = serde_json::to_string(&r).unwrap();
    for s in [
        "private-attribute-value",
        "private-text-value",
        "private-cdata-value",
    ] {
        assert!(!json.contains(s));
    }
}
#[test]
fn xml_predefined_and_numeric_entities_are_accepted() {
    assert!(probe_xml("<r a='a &amp; b'>&lt; &#65; &#x42;</r>").is_ok());
}
#[test]
fn xml_dtd_custom_and_external_entities_are_rejected() {
    for s in [
        "<!DOCTYPE r [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><r>&x;</r>",
        "<!DOCTYPE r><r/>",
        "<r>&unknown;</r>",
        "<r a='&unknown;'/>",
    ] {
        assert!(probe_xml(s).is_err(), "{s}");
    }
}
#[test]
fn xml_malformed_and_incomplete_documents_fail() {
    for s in [
        "",
        "<a>",
        "<a></b>",
        "<a/><b/>",
        "before<a/>",
        "<a/>after",
        "<a q='1' q='2'/>",
        "<a><![CDATA[x]]>",
        "<p:r/>",
    ] {
        assert!(probe_xml(s).is_err(), "{s}");
    }
}
#[test]
fn xml_duplicate_expanded_attributes_fail_even_with_different_prefixes() {
    assert!(probe_xml("<r xmlns:a='u' xmlns:b='u' a:x='1' b:x='2'/>").is_err());
}
#[test]
fn xml_utf8_bom_and_declaration_are_supported() {
    assert!(probe_xml("\u{feff}<?xml version='1.0' encoding='UTF-8'?><r>árvíz</r>").is_ok());
}
#[test]
fn xml_other_encoding_or_version_is_explicitly_rejected() {
    for s in [
        "<?xml version='1.0' encoding='UTF-16'?><r/>",
        "<?xml version='1.1'?><r/>",
        "<r/><?xml version='1.0'?>",
    ] {
        assert!(probe_xml(s).is_err());
    }
}
#[test]
fn xml_budgets_fail_instead_of_silently_truncating() {
    let (_d, p) = file("a.xml", b"<r><a x='1' y='2'/><b/></r>");
    for (name, value) in [
        ("--max-input-bytes", "2"),
        ("--max-depth", "1"),
        ("--max-paths", "1"),
        ("--max-attributes", "1"),
    ] {
        let o = run(&["xml-probe", p.to_str().unwrap(), name, value]);
        assert_eq!(
            o.status.code(),
            Some(6),
            "{name}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
    }
}
#[test]
fn xml_zero_timeout_is_immediate_not_unlimited() {
    let (_d, p) = file("a.xml", b"<r/>");
    let o = run(&["xml-probe", p.to_str().unwrap(), "--timeout-ms", "0"]);
    assert_eq!(o.status.code(), Some(124));
}
#[test]
fn cancelled_library_runtime_stops_before_inspection() {
    let r = Runtime::default();
    r.cancel();
    let mut out = Vec::new();
    let e = dispatch(&cli(&["capabilities"]), &InspectionOnlyEngine, &r, &mut out).unwrap_err();
    assert_eq!(e.exit, 130);
    assert!(out.is_empty());
}
#[test]
fn runtime_callbacks_are_called_on_cancel() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let flag = Arc::new(AtomicBool::new(false));
    let t = flag.clone();
    let r = Runtime::with_cancel_callback(move || t.store(true, Ordering::SeqCst));
    r.cancel();
    assert!(flag.load(Ordering::SeqCst));
    assert!(r.is_cancelled());
}
#[test]
fn goals_formats_and_options_are_checked_before_engine() {
    let engine = RecordingEngine::default();
    for args in [
        vec!["plan", "a.txt", "--goal", "bogus"],
        vec!["plan", "a.txt", "--format", "bogus"],
        vec!["plan", "a.txt", "--options-json", "[]"],
        vec!["plan", "a.txt", "--options-json", "{"],
        vec!["plan", "a.txt", "--route", ""],
    ] {
        assert!(dispatch(&cli(&args), &engine, &Runtime::default(), &mut Vec::new()).is_err());
    }
    assert!(engine.requests.lock().unwrap().is_empty());
}
#[test]
fn request_options_and_policy_are_forwarded_unchanged() {
    let engine = RecordingEngine::default();
    let c = cli(&[
        "plan",
        "a.xml",
        "--goal",
        "tables",
        "--format",
        "xml",
        "--route",
        "explicit",
        "--options-json",
        "{\"rows\":\"item\"}",
        "--allow-external",
        "--max-input-bytes",
        "123",
        "--max-export-bytes",
        "456",
        "--max-export-files",
        "7",
        "--timeout-ms",
        "900",
        "--scratch-parent",
        "/tmp",
    ]);
    let mut out = Vec::new();
    dispatch(&c, &engine, &Runtime::default(), &mut out).unwrap();
    let v = engine.requests.lock().unwrap();
    let r = &v[0];
    assert_eq!(r.goal, "tables");
    assert_eq!(r.format.as_deref(), Some("xml"));
    assert_eq!(r.options, json!({"rows":"item"}));
    assert!(r.allow_external);
    assert_eq!(r.max_input_bytes, 123);
    assert_eq!(r.max_export_bytes, 456);
    assert_eq!(r.max_export_files, 7);
    assert_eq!(r.timeout_ms, Some(900));
    assert_eq!(r.scratch_parent, Some(PathBuf::from("/tmp")));
}
#[test]
fn options_file_is_bounded_and_parsed() {
    let (_d, p) = file("opts.json", br#"{"mode":"x"}"#);
    let c = cli(&["plan", "a.txt", "--options-file", p.to_str().unwrap()]);
    let engine = RecordingEngine::default();
    dispatch(&c, &engine, &Runtime::default(), &mut Vec::new()).unwrap();
    assert_eq!(
        engine.requests.lock().unwrap()[0].options,
        json!({"mode":"x"})
    );
}
#[test]
fn options_sources_are_mutually_exclusive() {
    assert!(
        Cli::try_parse_from([
            "omniattach",
            "plan",
            "a.txt",
            "--options-json",
            "{}",
            "--options-file",
            "x"
        ])
        .is_err()
    );
}
#[test]
fn custom_goal_names_are_preserved() {
    let engine = RecordingEngine::default();
    let c = cli(&[
        "plan",
        "x.bin",
        "--goal",
        "custom:my-analysis",
        "--format",
        "image",
    ]);
    dispatch(&c, &engine, &Runtime::default(), &mut Vec::new()).unwrap();
    assert_eq!(
        engine.requests.lock().unwrap()[0].goal,
        "custom:my-analysis"
    );
}
#[test]
fn empty_custom_goal_is_rejected() {
    assert!(validate_goal("custom:").is_err());
}
#[test]
fn durable_export_result_has_actual_export_path_and_embedded_manifest() {
    let (_d, p) = file("a.txt", b"hi");
    let output = p.parent().unwrap().join("result");
    let c = cli(&[
        "convert",
        p.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    let mut stdout = Vec::new();
    dispatch(
        &c,
        &RecordingEngine::default(),
        &Runtime::default(),
        &mut stdout,
    )
    .unwrap();
    let v: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(v["schema"], "omniattach-cli/result/0.1");
    assert_eq!(v["output_dir"], output.to_str().unwrap());
    assert!(output.join("manifest.json").is_file());
    assert_eq!(v["manifest"]["source"], "transport-test");
}
#[test]
fn stdout_text_and_none_modes_do_not_skip_durable_export() {
    for mode in ["text", "none"] {
        let (_d, p) = file("a.txt", b"hi");
        let output = p.parent().unwrap().join("out");
        let c = cli(&[
            "convert",
            p.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--stdout",
            mode,
        ]);
        let mut stdout = Vec::new();
        dispatch(
            &c,
            &RecordingEngine::default(),
            &Runtime::default(),
            &mut stdout,
        )
        .unwrap();
        assert_eq!(
            stdout,
            if mode == "text" {
                b"preserved text".to_vec()
            } else {
                vec![]
            }
        );
        assert!(output.join("manifest.json").is_file());
    }
}
#[test]
fn existing_destination_is_never_overwritten() {
    let (_d, p) = file("a.txt", b"hi");
    let engine = RecordingEngine::default();
    let c = cli(&[
        "convert",
        p.to_str().unwrap(),
        "--output",
        p.parent().unwrap().to_str().unwrap(),
    ]);
    let e = dispatch(&c, &engine, &Runtime::default(), &mut Vec::new()).unwrap_err();
    assert_eq!(e.exit, 2);
    assert!(engine.requests.lock().unwrap().is_empty());
}
#[test]
fn nonexistent_export_parent_is_not_created_implicitly() {
    let (_d, p) = file("a.txt", b"hi");
    let output = p.parent().unwrap().join("absent/new");
    let c = cli(&[
        "convert",
        p.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        dispatch(
            &c,
            &RecordingEngine::default(),
            &Runtime::default(),
            &mut Vec::new()
        )
        .is_err()
    );
    assert!(!output.parent().unwrap().exists());
}
#[test]
fn whisper_configuration_is_explicitly_validated() {
    for args in [
        vec!["capabilities", "--whisper-model", "model.bin"],
        vec!["capabilities", "--language", "hu"],
    ] {
        let c = cli(&args);
        assert!(EngineConfig::from_cli(&c).is_err());
    }
    let c = cli(&[
        "capabilities",
        "--external-backends",
        "--whisper-model",
        "model.bin",
        "--language",
        "hu",
    ]);
    assert_eq!(EngineConfig::from_cli(&c).unwrap().language, "hu");
}
#[test]
fn broken_output_writer_is_reported_not_panicked() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        dispatch(
            &cli(&["capabilities"]),
            &InspectionOnlyEngine,
            &Runtime::default(),
            &mut Broken
        )
        .is_err()
    );
}
#[cfg(not(feature = "facade"))]
#[test]
fn no_facade_build_reports_conversion_unavailable_honestly() {
    let o = run(&["plan", "some.txt"]);
    assert_eq!(o.status.code(), Some(4));
    let e: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(e["error"]["code"], "UNSUPPORTED");
    let v = parsed(&run(&["capabilities"]));
    assert_eq!(v["registered"]["conversion_enabled"], false);
    assert_eq!(v["build"]["facade"], false);
}
#[test]
fn xml_control_characters_and_illegal_character_references_fail() {
    for s in ["<r>\u{1}</r>", "<r>&#0;</r>", "<r a='&#1;'/>"] {
        assert!(probe_xml(s).is_err(), "{s:?}");
    }
}
#[test]
fn xml_comments_and_processing_instructions_are_ignored() {
    let r = probe_xml(
        "<?xml version='1.0'?><!-- lead --><r><?task value?><!--ok--><a/></r><!-- tail -->",
    )
    .unwrap();
    assert_eq!(r.elements, 2);
}
#[test]
fn invalid_comment_is_rejected() {
    assert!(probe_xml("<r><!-- a--b --></r>").is_err());
}
#[test]
fn xml_compact_subprocess_result_is_parseable_and_complete() {
    let (_d, p) = file("a.xml", b"<root><item/><item/></root>");
    let o = run(&["xml-probe", p.to_str().unwrap()]);
    let v = parsed(&o);
    assert_eq!(v["elements"], 3);
    assert!(o.stderr.is_empty());
    assert_eq!(o.stdout.iter().filter(|b| **b == b'\n').count(), 1);
}
#[test]
fn oversized_options_are_rejected_before_engine() {
    let (_d, p) = file("large.json", &vec![b' '; 1048577]);
    let c = cli(&["plan", "a.txt", "--options-file", p.to_str().unwrap()]);
    let e = make_request(match &c.command {
        Command::Plan(p) => &p.request,
        _ => unreachable!(),
    })
    .unwrap_err();
    assert_eq!(e.exit, 6);
}
#[test]
fn engine_failure_does_not_print_a_success_record() {
    struct Failure;
    impl Engine for Failure {
        fn capabilities(&self) -> Result<Value> {
            Err(omniattach_cli::CliError::new("TEST_FAILURE", "expected", 1))
        }
        fn plan(&self, _: &EngineRequest) -> Result<Value> {
            unreachable!()
        }
        fn convert(
            &self,
            _: &EngineRequest,
            _: &Path,
            _: ProgressMode,
        ) -> Result<ConversionOutcome> {
            unreachable!()
        }
    }
    let mut out = Vec::new();
    assert!(
        dispatch(
            &cli(&["capabilities"]),
            &Failure,
            &Runtime::default(),
            &mut out
        )
        .is_err()
    );
    assert!(out.is_empty());
}
#[cfg(unix)]
#[test]
fn dangling_output_symlink_is_not_followed_or_overwritten() {
    let (_d, p) = file("a.txt", b"Hi");
    let output = p.parent().unwrap().join("out");
    std::os::unix::fs::symlink("missing-target", &output).unwrap();
    let c = cli(&[
        "convert",
        p.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        dispatch(
            &c,
            &RecordingEngine::default(),
            &Runtime::default(),
            &mut Vec::new()
        )
        .is_err()
    );
    assert!(
        std::fs::symlink_metadata(output)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
#[test]
fn cancellation_before_xml_probe_returns_130() {
    let (_d, p) = file("a.xml", b"<r/>");
    let c = cli(&["xml-probe", p.to_str().unwrap()]);
    let runtime = Runtime::default();
    runtime.cancel();
    let Command::XmlProbe(args) = c.command else {
        unreachable!()
    };
    assert_eq!(
        omniattach_cli::xml::xml_probe(&args, &runtime)
            .unwrap_err()
            .exit,
        130
    );
}
#[test]
fn extension_case_and_json_lines_aliases_are_consistent() {
    assert_eq!(omniattach_cli::probe::extension_format(".PDF"), "pdf");
    assert_eq!(
        omniattach_cli::probe::extension_format("ndjson"),
        "json_lines"
    );
    assert!(validate_format("json-lines").is_ok());
    assert!(validate_format("json_lines").is_ok());
}
#[test]
fn xml_zero_limits_are_usage_errors() {
    let (_d, p) = file("a.xml", b"<r/>");
    for option in ["--max-depth", "--max-paths"] {
        let o = run(&["xml-probe", p.to_str().unwrap(), option, "0"]);
        assert_eq!(o.status.code(), Some(2));
    }
}
