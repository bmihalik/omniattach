use omniattach::*;
use serde_json::json;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn input(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    (temp, path)
}
fn empty(_: &StagedInput, _: &ConversionRequest, _: &JobContext) -> Result<ConversionData> {
    Ok(ConversionData::default())
}
fn route(id: &str) -> Route {
    Route::new(
        id,
        vec![Format::Text],
        vec![Goal::Extract],
        ExecutionKind::InProcess,
    )
}
#[test]
fn default_registry_contains_only_real_portable_support() {
    let engine = OmniAttach::new();
    assert_eq!(engine.capabilities().len(), 1);
    assert_eq!(engine.capabilities()[0].id, "portable-text");
}
#[test]
fn text_source_hash_and_unmodified_contents() {
    let (_d, p) = input("a.txt", b"abc");
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p.clone(), Goal::Extract),
            &FacadeContext::default(),
        )
        .unwrap();
    assert_eq!(b.plain_text(), "abc");
    assert_eq!(
        b.manifest().source.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(b.manifest().source.bytes, 3);
    assert_eq!(b.manifest().source.name, p.to_string_lossy());
}
#[test]
fn markdown_conversion_creates_owned_fenced_artifact() {
    let (_d, p) = input("a.txt", b"```\nhi");
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p, Goal::Markdown),
            &FacadeContext::default(),
        )
        .unwrap();
    let root = b.root();
    let text = std::fs::read_to_string(b.artifact_path(0).unwrap()).unwrap();
    assert!(text.starts_with("````text\n```\nhi\n````"));
    drop(b);
    assert!(!root.exists());
}
#[test]
fn markdown_input_is_preserved_not_reparsed() {
    let (_d, p) = input("a.md", b"# Heading\n\ntext\n");
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p, Goal::Markdown),
            &FacadeContext::default(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(b.artifact_path(0).unwrap()).unwrap(),
        b"# Heading\n\ntext\n"
    );
}
#[test]
fn export_only_contains_declared_artifacts_and_manifest() {
    let (_d, p) = input("a.txt", b"Hi");
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p, Goal::Markdown),
            &FacadeContext::default(),
        )
        .unwrap();
    let dest = tempfile::tempdir().unwrap();
    let path = dest.path().join("export");
    b.export(&path).unwrap();
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 2);
    assert!(!path.join("input.txt").exists());
    assert!(b.export(&path).is_err());
    let parsed: FacadeManifest =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(&parsed, b.manifest());
}
#[test]
fn ambiguity_requires_explicit_choice() {
    let (_d, p) = input("a.txt", b"hi");
    let engine = OmniAttach::builder()
        .register(route("a"), empty)
        .unwrap()
        .register(route("b"), empty)
        .unwrap()
        .build();
    let r = ConversionRequest::new(p, Goal::Extract);
    assert!(matches!(
        engine.plan(&r, &FacadeContext::default()),
        Err(Error::AmbiguousRoute(_))
    ));
    assert_eq!(
        engine
            .plan(&r.with_route("b"), &FacadeContext::default())
            .unwrap()
            .route
            .id,
        "b"
    );
}
#[test]
fn duplicate_registration_fails() {
    let b = OmniAttach::builder().register(route("a"), empty).unwrap();
    assert!(b.register(route("a"), empty).is_err());
}
#[test]
fn no_route_never_claims_pdf_layout_support() {
    let (_d, p) = input("a.pdf", b"%PDF-1.4");
    assert!(matches!(
        OmniAttach::new().convert(
            &ConversionRequest::new(p, Goal::PdfLayout),
            &FacadeContext::default()
        ),
        Err(Error::NoRoute { .. })
    ));
}
#[test]
fn route_failure_does_not_fall_back() {
    let (_d, p) = input("a.txt", b"Hi");
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let fail = move |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(Error::InvalidOutput("deliberate".into()))
    };
    let e = OmniAttach::builder()
        .register(route("bad"), fail)
        .unwrap()
        .register(route("other"), empty)
        .unwrap()
        .build();
    assert!(
        e.convert(
            &ConversionRequest::new(p, Goal::Extract).with_route("bad"),
            &FacadeContext::default()
        )
        .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
#[test]
fn external_routes_require_permission() {
    let (_d, p) = input("a.txt", b"Hi");
    let r = Route::new(
        "external",
        vec![Format::Text],
        vec![Goal::Extract],
        ExecutionKind::External,
    );
    let e = OmniAttach::builder().register(r, empty).unwrap().build();
    let req = ConversionRequest::new(p, Goal::Extract);
    assert!(matches!(
        e.convert(&req, &FacadeContext::default()),
        Err(Error::ExternalDisabled)
    ));
    assert!(
        e.convert(&req, &FacadeContext::default().with_external(true))
            .is_ok()
    );
}
#[test]
fn explicitly_selected_external_route_does_not_fall_back_to_inprocess() {
    let (_d, p) = input("a.txt", b"Hi");
    let r = Route::new(
        "external",
        vec![Format::Text],
        vec![Goal::Extract],
        ExecutionKind::External,
    );
    let e = OmniAttach::builder()
        .with_portable_defaults()
        .unwrap()
        .register(r, empty)
        .unwrap()
        .build();
    let req = ConversionRequest::new(p, Goal::Extract).with_route("external");
    assert!(matches!(
        e.convert(&req, &FacadeContext::default()),
        Err(Error::ExternalDisabled)
    ));
}
#[test]
fn unknown_and_mismatched_routes_are_distinct() {
    let (_d, p) = input("a.txt", b"Hi");
    let e = OmniAttach::new();
    assert!(matches!(
        e.plan(
            &ConversionRequest::new(&p, Goal::Extract).with_route("absent"),
            &FacadeContext::default()
        ),
        Err(Error::UnknownRoute(_))
    ));
    assert!(matches!(
        e.plan(
            &ConversionRequest::new(p, Goal::PdfLayout).with_route("portable-text"),
            &FacadeContext::default()
        ),
        Err(Error::RouteMismatch { .. })
    ));
}
#[test]
fn cancelled_request_never_calls_converter() {
    let (_d, p) = input("a.txt", b"Hi");
    let c = FacadeContext::default();
    c.cancellation.cancel();
    assert!(matches!(
        OmniAttach::new().convert(&ConversionRequest::new(p, Goal::Extract), &c),
        Err(Error::Cancelled)
    ));
}
#[test]
fn cancellation_during_converter_discards_result() {
    let (_d, p) = input("a.txt", b"Hi");
    let f = |_: &StagedInput, _: &ConversionRequest, c: &JobContext| {
        c.cancellation().cancel();
        Ok(ConversionData::default())
    };
    let e = OmniAttach::builder()
        .register(route("cancel"), f)
        .unwrap()
        .build();
    assert!(matches!(
        e.convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::Cancelled)
    ));
}
#[test]
fn deadline_is_cooperative_and_late_results_are_rejected() {
    let (_d, p) = input("a.txt", b"Hi");
    let f = |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        std::thread::sleep(Duration::from_millis(30));
        Ok(ConversionData::default())
    };
    let e = OmniAttach::builder()
        .register(route("slow"), f)
        .unwrap()
        .build();
    let mut c = FacadeContext::default();
    c.policy.timeout = Some(Duration::from_millis(10));
    assert!(matches!(
        e.convert(&ConversionRequest::new(p, Goal::Extract), &c),
        Err(Error::Deadline)
    ));
}
#[test]
fn input_and_output_caps_are_checked() {
    let (_d, p) = input("a.txt", b"long text");
    let mut c = FacadeContext::default();
    c.policy.max_input_bytes = 2;
    assert!(matches!(
        OmniAttach::new().convert(&ConversionRequest::new(&p, Goal::Extract), &c),
        Err(Error::Limit(_))
    ));
    let mut c = FacadeContext::default();
    c.policy.max_export_bytes = 2;
    assert!(matches!(
        OmniAttach::new().convert(&ConversionRequest::new(p, Goal::Markdown), &c),
        Err(Error::Limit(_))
    ));
}
#[test]
fn text_encoding_support_is_strict() {
    for (little, name) in [(true, "le.txt"), (false, "be.txt")] {
        let mut bytes = if little {
            vec![255, 254]
        } else {
            vec![254, 255]
        };
        for unit in "árvíz 🎵".encode_utf16() {
            bytes.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        let (_d, p) = input(name, &bytes);
        let b = OmniAttach::new()
            .convert(
                &ConversionRequest::new(p, Goal::Extract),
                &FacadeContext::default(),
            )
            .unwrap();
        assert_eq!(b.plain_text(), "árvíz 🎵");
    }
    for bytes in [&b"\xff"[..], &b"\xff\xfeA"[..]] {
        let (_d, p) = input("bad.txt", bytes);
        assert!(
            OmniAttach::new()
                .convert(
                    &ConversionRequest::new(p, Goal::Extract),
                    &FacadeContext::default()
                )
                .is_err()
        );
    }
}
#[test]
fn unknown_format_requires_override() {
    let (_d, p) = input("data", b"abc");
    assert!(matches!(
        OmniAttach::new().convert(
            &ConversionRequest::new(&p, Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::UnknownFormat)
    ));
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p, Goal::Extract).with_format(Format::Text),
            &FacadeContext::default(),
        )
        .unwrap();
    assert!(b.manifest().warnings.iter().any(|w| w.code == "DETECTION"));
}
#[test]
fn signature_detection_overrides_misleading_extension() {
    let d = detect_prefix(b"%PDF-1.4", "txt");
    assert_eq!(d.format, Format::Pdf);
    assert_eq!(d.basis, DetectionBasis::Signature);
    assert_eq!(d.warnings.len(), 1);
    let d = detect_prefix(b"PK\x03\x04", "docx");
    assert_eq!(d.basis, DetectionBasis::Extension);
    assert!(!d.warnings.is_empty());
}
#[test]
fn document_adapter_keeps_payload_schema_without_field_guessing() {
    let (_d, p) = input("a.txt", b"Hi");
    let adapter = DocumentAdapter::new("my-doc/1", |_: &Path, opts: &serde_json::Value| {
        Ok::<_, String>(json!({"unknown_document_field":opts}))
    });
    let e = OmniAttach::builder()
        .register(route("docs"), adapter)
        .unwrap()
        .build();
    let b = e
        .convert(
            &ConversionRequest::new(p, Goal::Extract).with_options(json!({"tables":true})),
            &FacadeContext::default(),
        )
        .unwrap();
    assert_eq!(
        b.payload("my-doc/1").unwrap()["unknown_document_field"]["tables"],
        true
    );
    assert!(b.manifest().text.is_empty());
}
#[test]
fn document_extractor_errors_are_not_swallowed() {
    let (_d, p) = input("a.txt", b"Hi");
    let adapter = DocumentAdapter::new("doc", |_: &Path, _: &serde_json::Value| {
        Err::<serde_json::Value, _>("failed")
    });
    let e = OmniAttach::builder()
        .register(route("docs"), adapter)
        .unwrap()
        .build();
    assert!(matches!(
        e.convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::Converter { .. })
    ));
}
#[test]
fn staged_input_is_not_the_original_and_sidecars_are_not_copied() {
    let (d, p) = input("weird $(name).txt", b"Hi");
    std::fs::write(d.path().join("sidecar"), b"secret").unwrap();
    let original = p.clone();
    let adapter = move |i: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        assert_ne!(i.path, original);
        assert_eq!(i.path.file_name().unwrap(), "input.txt");
        assert!(!i.path.parent().unwrap().join("sidecar").exists());
        Ok(ConversionData::default())
    };
    let e = OmniAttach::builder()
        .register(route("inspect"), adapter)
        .unwrap()
        .build();
    e.convert(
        &ConversionRequest::new(p, Goal::Extract),
        &FacadeContext::default(),
    )
    .unwrap();
}
#[test]
fn undeclared_artifacts_do_not_export() {
    let (_d, p) = input("a.txt", b"Hi");
    let f = |i: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        std::fs::write(i.output_dir.join("private"), b"secret")?;
        Ok(ConversionData::default())
    };
    let e = OmniAttach::builder()
        .register(route("no-assets"), f)
        .unwrap()
        .build();
    let b = e
        .convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default(),
        )
        .unwrap();
    let d = tempfile::tempdir().unwrap();
    b.export(d.path().join("out")).unwrap();
    assert_eq!(std::fs::read_dir(d.path().join("out")).unwrap().count(), 1);
}
#[test]
fn output_path_escape_reserved_and_duplicate_names_are_rejected() {
    for path in [
        "../escape",
        "/etc/passwd",
        "bad\\path",
        "manifest.json",
        "manifest.json/child",
    ] {
        let (_d, p) = input("a.txt", b"Hi");
        let name = path.to_owned();
        let f = move |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
            Ok(ConversionData {
                artifacts: vec![PendingArtifact::new(&name, "text/plain")],
                ..Default::default()
            })
        };
        let e = OmniAttach::builder()
            .register(route("bad-output"), f)
            .unwrap()
            .build();
        assert!(
            e.convert(
                &ConversionRequest::new(p, Goal::Extract),
                &FacadeContext::default()
            )
            .is_err(),
            "{path}"
        );
    }
}
#[test]
fn multiple_artifact_limits_and_duplicates_are_enforced() {
    for kind in [0, 1, 2] {
        let (_d, p) = input("a.txt", b"Hi");
        let f = move |i: &StagedInput, _: &ConversionRequest, _: &JobContext| {
            std::fs::write(i.output_dir.join("a"), b"123")?;
            std::fs::write(i.output_dir.join("b"), b"123")?;
            Ok(ConversionData {
                artifacts: vec![
                    PendingArtifact::new("a", "text/plain"),
                    PendingArtifact::new(if kind == 0 { "a" } else { "b" }, "text/plain"),
                ],
                ..Default::default()
            })
        };
        let e = OmniAttach::builder()
            .register(route("assets"), f)
            .unwrap()
            .build();
        let mut c = FacadeContext::default();
        if kind == 1 {
            c.policy.max_export_files = 1;
        }
        if kind == 2 {
            c.policy.max_export_bytes = 5;
        }
        assert!(
            e.convert(&ConversionRequest::new(p, Goal::Extract), &c)
                .is_err()
        );
    }
}
#[cfg(unix)]
#[test]
fn artifact_symlinks_are_rejected() {
    let (_d, p) = input("a.txt", b"Hi");
    let f = |i: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        std::os::unix::fs::symlink("/etc/passwd", i.output_dir.join("link"))?;
        Ok(ConversionData {
            artifacts: vec![PendingArtifact::new("link", "text/plain")],
            ..Default::default()
        })
    };
    let e = OmniAttach::builder()
        .register(route("bad"), f)
        .unwrap()
        .build();
    assert!(matches!(
        e.convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::InvalidOutput(_))
    ));
}
#[test]
fn scratch_directory_cleans_on_failed_conversion() {
    let (_d, p) = input("a.txt", b"Hi");
    let scratch = tempfile::tempdir().unwrap();
    let mut c = FacadeContext::default();
    c.policy.scratch_parent = Some(scratch.path().into());
    let f = |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        Err(Error::InvalidOutput("bad".into()))
    };
    let e = OmniAttach::builder()
        .register(route("bad"), f)
        .unwrap()
        .build();
    assert!(
        e.convert(&ConversionRequest::new(p, Goal::Extract), &c)
            .is_err()
    );
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
}
#[test]
fn progress_events_are_ordered_on_success() {
    let (_d, p) = input("a.txt", b"Hi");
    let log = Arc::new(Mutex::new(vec![]));
    let target = log.clone();
    let c = FacadeContext::default()
        .with_progress(move |e: &ProgressEvent| target.lock().unwrap().push(e.stage.clone()));
    OmniAttach::new()
        .convert(&ConversionRequest::new(p, Goal::Extract), &c)
        .unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        vec![Stage::Staging, Stage::Converting, Stage::Completed]
    );
}
#[test]
fn engine_can_be_shared_for_concurrent_jobs() {
    let (_d, p) = input("a.txt", b"Hi");
    let e = Arc::new(OmniAttach::new());
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let e = e.clone();
            let p = p.clone();
            std::thread::spawn(move || {
                e.convert(
                    &ConversionRequest::new(p, Goal::Extract),
                    &FacadeContext::default(),
                )
                .unwrap()
                .plain_text()
            })
        })
        .collect();
    for t in threads {
        assert_eq!(t.join().unwrap(), "Hi");
    }
}
#[test]
fn portable_route_rejects_unrecognized_options() {
    let (_d, p) = input("a.txt", b"Hi");
    assert!(matches!(
        OmniAttach::new().convert(
            &ConversionRequest::new(p, Goal::Extract).with_options(json!({"ignored":true})),
            &FacadeContext::default()
        ),
        Err(Error::Configuration(_))
    ));
}
#[test]
fn pipeline_retains_intermediate_identity_and_both_asset_sets() {
    let (_d, p) = input("a.txt", b"original");
    let first = |i: &StagedInput, r: &ConversionRequest, _: &JobContext| {
        assert_eq!(r.goal, Goal::Normalize);
        std::fs::write(i.output_dir.join("middle.txt"), b"normalized")?;
        Ok(ConversionData {
            artifacts: vec![PendingArtifact::new("middle.txt", "text/plain")],
            ..Default::default()
        })
    };
    let second = |i: &StagedInput, r: &ConversionRequest, _: &JobContext| {
        assert_eq!(std::fs::read_to_string(&i.path)?, "normalized");
        assert_eq!(i.source.bytes, 10);
        assert_eq!(r.goal, Goal::Extract);
        std::fs::write(i.output_dir.join("final.txt"), b"final")?;
        Ok(ConversionData {
            text: vec![TextFragment {
                text: "final".into(),
                location: json!(null),
            }],
            artifacts: vec![PendingArtifact::new("final.txt", "text/plain")],
            ..Default::default()
        })
    };
    let pipe = ArtifactPipeline::new(first, second, Goal::Normalize, Format::Text);
    let engine = OmniAttach::builder()
        .register(route("pipe"), pipe)
        .unwrap()
        .build();
    let b = engine
        .convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default(),
        )
        .unwrap();
    assert_eq!(b.manifest().source.bytes, 8);
    assert_eq!(b.manifest().artifacts.len(), 2);
    assert_eq!(b.manifest().artifacts[0].path, "stage-1/middle.txt");
    assert_eq!(b.plain_text(), "final");
    assert_eq!(
        b.payload("omniattach/pipeline/0.1").unwrap()["intermediate"]["bytes"],
        10
    );
}
#[test]
fn pipeline_does_not_run_second_stage_after_first_failure() {
    let (_d, p) = input("a.txt", b"Hi");
    let first = |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        Err(Error::InvalidOutput("first failed".into()))
    };
    let second = |_: &StagedInput,
                  _: &ConversionRequest,
                  _: &JobContext|
     -> Result<ConversionData> { panic!("must not be called") };
    let pipe = ArtifactPipeline::new(first, second, Goal::Normalize, Format::Text);
    let engine = OmniAttach::builder()
        .register(route("pipe"), pipe)
        .unwrap()
        .build();
    assert!(
        engine
            .convert(
                &ConversionRequest::new(p, Goal::Extract),
                &FacadeContext::default()
            )
            .is_err()
    );
}
#[test]
fn pipeline_missing_intermediate_is_an_error() {
    let (_d, p) = input("a.txt", b"Hi");
    let pipe = ArtifactPipeline::new(empty, empty, Goal::Normalize, Format::Text);
    let engine = OmniAttach::builder()
        .register(route("pipe"), pipe)
        .unwrap()
        .build();
    assert!(matches!(
        engine.convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::InvalidOutput(_))
    ));
}
#[test]
fn zero_deadline_returns_before_converter_runs() {
    let (_d, p) = input("a.txt", b"Hi");
    let mut c = FacadeContext::default();
    c.policy.timeout = Some(Duration::ZERO);
    assert!(matches!(
        OmniAttach::new().convert(&ConversionRequest::new(p, Goal::Extract), &c),
        Err(Error::Deadline)
    ));
}
#[test]
fn invalid_route_contracts_are_rejected() {
    for r in [
        Route::new(
            "",
            vec![Format::Text],
            vec![Goal::Extract],
            ExecutionKind::InProcess,
        ),
        Route::new(
            "empty-formats",
            vec![],
            vec![Goal::Extract],
            ExecutionKind::InProcess,
        ),
        Route::new(
            "empty-goals",
            vec![Format::Text],
            vec![],
            ExecutionKind::InProcess,
        ),
    ] {
        assert!(OmniAttach::builder().register(r, empty).is_err());
    }
}
#[test]
fn empty_and_bom_only_text_remain_valid() {
    for bytes in [&b""[..], &b"\xef\xbb\xbf"[..], &b"\xff\xfe"[..]] {
        let (_d, p) = input("empty.txt", bytes);
        let b = OmniAttach::new()
            .convert(
                &ConversionRequest::new(p, Goal::Extract),
                &FacadeContext::default(),
            )
            .unwrap();
        assert_eq!(b.plain_text(), "");
    }
}
#[test]
fn failed_converter_reports_failure_without_completion() {
    let (_d, p) = input("a.txt", b"Hi");
    let log = Arc::new(Mutex::new(vec![]));
    let target = log.clone();
    let c = FacadeContext::default()
        .with_progress(move |e: &ProgressEvent| target.lock().unwrap().push(e.stage.clone()));
    let f = |_: &StagedInput, _: &ConversionRequest, _: &JobContext| {
        Err(Error::InvalidOutput("bad".into()))
    };
    let engine = OmniAttach::builder()
        .register(route("bad"), f)
        .unwrap()
        .build();
    assert!(
        engine
            .convert(&ConversionRequest::new(p, Goal::Extract), &c)
            .is_err()
    );
    assert_eq!(
        *log.lock().unwrap(),
        vec![Stage::Staging, Stage::Converting, Stage::Failed]
    );
}
#[test]
fn input_directory_is_rejected_without_hanging() {
    let temp = tempfile::tempdir().unwrap();
    assert!(matches!(
        OmniAttach::new().convert(
            &ConversionRequest::new(temp.path(), Goal::Extract),
            &FacadeContext::default()
        ),
        Err(Error::InvalidInput(_))
    ));
}
#[test]
fn utf8_bom_is_removed_without_changing_source_hash() {
    let (_d, p) = input("a.txt", b"\xef\xbb\xbfHi");
    let b = OmniAttach::new()
        .convert(
            &ConversionRequest::new(p, Goal::Extract),
            &FacadeContext::default(),
        )
        .unwrap();
    assert_eq!(b.plain_text(), "Hi");
    assert_eq!(b.manifest().source.bytes, 5);
}
