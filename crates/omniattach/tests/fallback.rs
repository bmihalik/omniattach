use omniattach::*;
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Copy)]
enum Step {
    Text(&'static str),
    Artifact,
    Limit,
    Input,
    Unsafe,
    Opaque,
    Cancel,
    Timeout,
    Spoof,
}
struct Script {
    id: &'static str,
    step: Step,
    calls: Arc<Mutex<Vec<String>>>,
}
fn output(input: &StagedInput, text: &str) -> Result<ConversionData> {
    fs::write(input.output_dir.join("result.md"), text)?;
    let mut data = ConversionData::default();
    data.text.push(TextFragment {
        text: text.into(),
        location: serde_json::Value::Null,
    });
    data.artifacts
        .push(PendingArtifact::new("result.md", "text/markdown"));
    Ok(data)
}
impl Converter for Script {
    fn convert(
        &self,
        input: &StagedInput,
        _: &ConversionRequest,
        context: &JobContext,
    ) -> Result<ConversionData> {
        self.calls.lock().unwrap().push(self.id.into());
        match self.step {
            Step::Text(text) => output(input, text),
            Step::Artifact => {
                fs::write(input.output_dir.join("data.bin"), b"bytes")?;
                let mut d = ConversionData::default();
                d.artifacts
                    .push(PendingArtifact::new("data.bin", "application/octet-stream"));
                Ok(d)
            }
            Step::Limit => Err(Error::Limit("fixture".into())),
            Step::Input => Err(Error::InvalidInput("fixture".into())),
            Step::Unsafe => {
                let mut d = output(input, "")?;
                d.artifacts[0].path = "../escape.md".into();
                Ok(d)
            }
            Step::Opaque => Err(Error::Converter {
                route: self.id.into(),
                message: "opaque".into(),
            }),
            Step::Cancel => {
                context.cancellation().cancel();
                Err(Error::Converter {
                    route: self.id.into(),
                    message: "cancelled while converting".into(),
                })
            }
            Step::Timeout => Err(Error::Deadline),
            Step::Spoof => {
                output(input, "text")?.payload("omniattach/fallback/0.1", serde_json::json!({}))
            }
        }
    }
}
fn route(id: &str) -> Route {
    Route::new(
        id,
        vec![Format::Text, Format::Pdf],
        vec![Goal::Extract, Goal::Markdown],
        ExecutionKind::InProcess,
    )
}
fn engine(steps: &[(&'static str, Step)], calls: &Arc<Mutex<Vec<String>>>) -> OmniAttach {
    let mut b = OmniAttach::builder();
    for (id, step) in steps {
        b = b
            .register(
                route(id),
                Script {
                    id,
                    step: *step,
                    calls: calls.clone(),
                },
            )
            .unwrap();
    }
    b.build()
}
fn input() -> (tempfile::TempDir, ConversionRequest) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.txt");
    fs::write(&path, b"stable").unwrap();
    (dir, ConversionRequest::new(path, Goal::Markdown))
}
fn unpack(error: Error) -> (Box<FallbackReport>, Box<Error>) {
    if let Error::Fallback { report, source } = error {
        (report, source)
    } else {
        panic!("missing fallback failure report: {error}")
    }
}
fn report(bundle: &FacadeBundle) -> FallbackReport {
    serde_json::from_value(bundle.payload("omniattach/fallback/0.1").unwrap().clone()).unwrap()
}
#[test]
fn order_first_success_and_ordinary_ambiguity_are_distinct() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = engine(
        &[("a", Step::Text("first")), ("b", Step::Text("second"))],
        &calls,
    );
    let (_dir, r) = input();
    assert!(matches!(
        e.convert(&r, &FacadeContext::default()),
        Err(Error::AmbiguousRoute(_))
    ));
    let b = e
        .convert_with_fallback(
            &r,
            &FacadeContext::default(),
            &FallbackPolicy::new(["b", "a"]),
        )
        .unwrap();
    assert_eq!(calls.lock().unwrap().join(","), "b");
    assert_eq!(b.manifest().route.id, "b");
    assert_eq!(report(&b).attempts.len(), 1);
    let direct = e
        .convert(&r.with_route("a"), &FacadeContext::default())
        .unwrap();
    assert!(direct.payload("omniattach/fallback/0.1").is_none());
}
#[test]
fn acceptance_handles_empty_unicode_and_artifact_only_results() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = engine(
        &[("a", Step::Text(" á β ")), ("b", Step::Text("abcd"))],
        &calls,
    );
    let (_dir, r) = input();
    let p = FallbackPolicy::new(["a", "b"])
        .with_acceptance(FallbackAcceptance::MinTextChars { min: 3 });
    let b = e
        .convert_with_fallback(&r, &FacadeContext::default(), &p)
        .unwrap();
    assert_eq!(report(&b).attempts[0].outcome, FallbackOutcome::Rejected);
    assert_eq!(b.plain_text(), "abcd");
    let e = engine(&[("a", Step::Artifact)], &calls);
    assert!(
        e.convert_with_fallback(&r, &FacadeContext::default(), &FallbackPolicy::new(["a"]))
            .is_ok()
    );
    let (trace, err) = unpack(
        e.convert_with_fallback(
            &r,
            &FacadeContext::default(),
            &FallbackPolicy::new(["a"]).with_acceptance(FallbackAcceptance::NonEmptyText),
        )
        .unwrap_err(),
    );
    assert!(matches!(*err, Error::FallbackExhausted));
    assert_eq!(trace.attempts[0].outcome, FallbackOutcome::Rejected);
}
#[test]
fn preflight_checks_entire_chain_before_any_converter() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = engine(&[("a", Step::Text("ok"))], &calls);
    let (_dir, r) = input();
    for policy in [
        FallbackPolicy::default(),
        FallbackPolicy::new(["a", "a"]),
        FallbackPolicy::new(["a", "missing"]),
    ] {
        let (trace, _) = unpack(
            e.convert_with_fallback(&r, &FacadeContext::default(), &policy)
                .unwrap_err(),
        );
        assert!(trace.attempts.is_empty());
    }
    let e = OmniAttach::builder()
        .register(
            route("a"),
            Script {
                id: "a",
                step: Step::Text("ok"),
                calls: calls.clone(),
            },
        )
        .unwrap()
        .register(
            Route::new(
                "blocked",
                vec![Format::Text],
                vec![Goal::Markdown],
                ExecutionKind::External,
            ),
            Script {
                id: "blocked",
                step: Step::Text("no"),
                calls: calls.clone(),
            },
        )
        .unwrap()
        .build();
    let (_, err) = unpack(
        e.convert_with_fallback(
            &r,
            &FacadeContext::default(),
            &FallbackPolicy::new(["a", "blocked"]),
        )
        .unwrap_err(),
    );
    assert!(matches!(*err, Error::ExternalDisabled));
    assert!(calls.lock().unwrap().is_empty());
    let (_, err) = unpack(
        e.convert_with_fallback(
            &r.clone().with_route("a"),
            &FacadeContext::default(),
            &FallbackPolicy::new(["a"]),
        )
        .unwrap_err(),
    );
    assert!(matches!(*err, Error::Configuration(_)));
}
#[test]
fn unsafe_output_limits_opaque_errors_and_cancellation_never_retry() {
    for step in [
        Step::Limit,
        Step::Input,
        Step::Unsafe,
        Step::Opaque,
        Step::Cancel,
        Step::Timeout,
        Step::Spoof,
    ] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let e = engine(&[("a", step), ("b", Step::Text("ok"))], &calls);
        let (_dir, r) = input();
        let p = FallbackPolicy::new(["a", "b"]).with_acceptance(FallbackAcceptance::NonEmptyText);
        let (trace, _) = unpack(
            e.convert_with_fallback(&r, &FacadeContext::default(), &p)
                .unwrap_err(),
        );
        assert_eq!(calls.lock().unwrap().join(","), "a");
        assert_eq!(trace.attempts[0].outcome, FallbackOutcome::TerminalError);
    }
}
#[test]
fn snapshot_options_rejected_cleanup_and_selected_export_are_preserved() {
    use sha2::{Digest, Sha256};
    let (_dir, r) = input();
    let original = r.path.clone();
    let remembered = Arc::new(Mutex::new(None::<std::path::PathBuf>));
    let first_path = remembered.clone();
    let b = OmniAttach::builder()
        .register(
            route("a"),
            move |i: &StagedInput,
                  q: &ConversionRequest,
                  _: &JobContext|
                  -> Result<ConversionData> {
                assert_eq!(q.options, serde_json::json!({"native":true}));
                *first_path.lock().unwrap() = Some(i.output_dir.clone());
                fs::write(&original, b"changed original")?;
                fs::write(&i.path, b"changed attempt input")?;
                output(i, " ")
            },
        )
        .unwrap();
    let previous = remembered.clone();
    let e = b
        .register(
            route("b"),
            move |i: &StagedInput,
                  q: &ConversionRequest,
                  _: &JobContext|
                  -> Result<ConversionData> {
                assert_eq!(q.options, serde_json::json!({}));
                assert_eq!(fs::read(&i.path)?, b"stable");
                assert!(!previous.lock().unwrap().as_ref().unwrap().exists());
                output(i, "accepted")
            },
        )
        .unwrap()
        .build();
    let p = FallbackPolicy {
        candidates: vec![
            FallbackCandidate::new("a"),
            FallbackCandidate::new("b").with_options(serde_json::json!({})),
        ],
        acceptance: FallbackAcceptance::NonEmptyText,
        ..Default::default()
    };
    let r = r.with_options(serde_json::json!({"native":true}));
    let bundle = e
        .convert_with_fallback(&r, &FacadeContext::default(), &p)
        .unwrap();
    assert_eq!(bundle.manifest().source.name, r.path.to_string_lossy());
    assert_eq!(
        bundle.manifest().source.sha256,
        format!("{:x}", Sha256::digest(b"stable"))
    );
    assert_eq!(report(&bundle).selected_route.as_deref(), Some("b"));
    let export = tempfile::tempdir().unwrap();
    let destination = export.path().join("out");
    bundle.export(&destination).unwrap();
    drop(bundle);
    assert_eq!(
        fs::read_to_string(destination.join("result.md")).unwrap(),
        "accepted"
    );
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(json["route"]["id"], "b");
}
#[test]
fn deadlines_and_input_export_limits_are_terminal() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = engine(
        &[("a", Step::Text("long output")), ("b", Step::Text("ok"))],
        &calls,
    );
    let (_dir, r) = input();
    let p = FallbackPolicy::new(["a", "b"]);
    let mut ctx = FacadeContext::default();
    ctx.policy.timeout = Some(Duration::ZERO);
    let (trace, err) = unpack(e.convert_with_fallback(&r, &ctx, &p).unwrap_err());
    assert!(matches!(*err, Error::Deadline));
    assert!(trace.attempts.is_empty());
    let mut ctx = FacadeContext::default();
    ctx.policy.max_input_bytes = 1;
    let (_, err) = unpack(e.convert_with_fallback(&r, &ctx, &p).unwrap_err());
    assert!(matches!(*err, Error::Limit(_)));
    assert!(calls.lock().unwrap().is_empty());
    let mut ctx = FacadeContext::default();
    ctx.policy.max_export_bytes = 2;
    let (_, err) = unpack(e.convert_with_fallback(&r, &ctx, &p).unwrap_err());
    assert!(matches!(*err, Error::Limit(_)));
    assert_eq!(calls.lock().unwrap().join(","), "a");
}
#[test]
fn attempt_timeout_requires_explicit_retry_and_progress_has_one_final_event() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = engine(&[("a", Step::Timeout), ("b", Step::Text("ok"))], &calls);
    let (_dir, r) = input();
    let mut p = FallbackPolicy::new(["a", "b"]);
    p.per_attempt_timeout_ms = Some(30000);
    assert!(
        e.convert_with_fallback(&r, &FacadeContext::default(), &p)
            .is_err()
    );
    calls.lock().unwrap().clear();
    p.retry_on.push(FallbackRetry::AttemptTimeout);
    let events = Arc::new(Mutex::new(Vec::new()));
    let copy = events.clone();
    let ctx = FacadeContext::default()
        .with_progress(move |e: &ProgressEvent| copy.lock().unwrap().push(e.clone()));
    let b = e.convert_with_fallback(&r, &ctx, &p).unwrap();
    assert_eq!(
        report(&b).attempts[0].retry_class,
        Some(FallbackRetry::AttemptTimeout)
    );
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.stage == Stage::Completed)
            .count(),
        1
    );
    assert!(!events.iter().any(|e| e.stage == Stage::Failed));
    assert!(
        events
            .windows(2)
            .all(|e| e[0].elapsed_ms <= e[1].elapsed_ms)
    );
}
#[cfg(feature = "backends")]
mod external {
    use super::*;
    struct Runner {
        calls: Mutex<Vec<String>>,
    }
    impl backends::Runner for Runner {
        fn run(
            &self,
            s: &backends::CommandSpec,
            _: &backends::RunOptions,
        ) -> backends::Result<backends::ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(s.program.to_string_lossy().into_owned());
            if s.program == std::path::Path::new("marker_single") {
                return Err(backends::Error::Spawn {
                    program: s.program.clone(),
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                });
            }
            fs::create_dir_all(s.cwd.join("out/images"))?;
            fs::write(s.cwd.join("out/document.md"), "![figure](images/a.png)")?;
            fs::write(s.cwd.join("out/images/a.png"), b"png")?;
            fs::write(s.cwd.join("out/content.json"), b"{}")?;
            Ok(backends::ProcessOutput {
                stdout: vec![],
                stderr: vec![],
                elapsed: Duration::ZERO,
            })
        }
    }
    #[test]
    fn native_rejection_missing_marker_and_mineru_success_keep_asset_links() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(Runner {
            calls: Mutex::new(Vec::new()),
        });
        let e = OmniAttach::builder()
            .register(
                route("native"),
                Script {
                    id: "native",
                    step: Step::Text(""),
                    calls,
                },
            )
            .unwrap()
            .with_marker_runner(backends::Marker::default(), runner.clone())
            .unwrap()
            .with_mineru_runner(backends::MinerU::default(), runner.clone())
            .unwrap()
            .build();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        fs::write(&path, b"%PDF-1.7\nfixture").unwrap();
        let p = FallbackPolicy::new(["native", "marker", "mineru"])
            .with_acceptance(FallbackAcceptance::NonEmptyText);
        let b = e
            .convert_with_fallback(
                &ConversionRequest::new(&path, Goal::Markdown),
                &FacadeContext::default().with_external(true),
                &p,
            )
            .unwrap();
        let trace = report(&b);
        assert_eq!(trace.attempts.len(), 3);
        assert_eq!(trace.attempts[0].outcome, FallbackOutcome::Rejected);
        assert_eq!(
            trace.attempts[1].retry_class,
            Some(FallbackRetry::ToolUnavailable)
        );
        assert_eq!(trace.selected_route.as_deref(), Some("mineru"));
        let main = b.payload("omniattach/layout-package/1").unwrap()["markdown_artifact"]
            .as_str()
            .unwrap()
            .to_owned();
        let export = dir.path().join("export");
        b.export(&export).unwrap();
        drop(b);
        assert_eq!(
            fs::read(export.join(&main).parent().unwrap().join("images/a.png")).unwrap(),
            b"png"
        );
    }
    #[test]
    fn explicit_process_failures_retry_but_nested_limits_do_not() {
        for limited in [false, true] {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let seen = calls.clone();
            let b = OmniAttach::builder()
                .register(
                    route("a"),
                    move |_: &StagedInput,
                          _: &ConversionRequest,
                          _: &JobContext|
                          -> Result<ConversionData> {
                        seen.lock().unwrap().push("a".into());
                        Err(if limited {
                            backends::Error::Limit("fixture".into())
                        } else {
                            backends::Error::ProcessFailed {
                                code: Some(2),
                                stderr: "x".repeat(4096),
                            }
                        }
                        .into())
                    },
                )
                .unwrap();
            let e = b
                .register(
                    route("b"),
                    Script {
                        id: "b",
                        step: Step::Text("ok"),
                        calls: calls.clone(),
                    },
                )
                .unwrap()
                .build();
            let (_dir, r) = input();
            let result = e.convert_with_fallback(
                &r,
                &FacadeContext::default(),
                &FallbackPolicy::new(["a", "b"]),
            );
            if limited {
                let (_, err) = unpack(result.unwrap_err());
                assert!(matches!(*err, Error::Backend(backends::Error::Limit(_))));
                assert_eq!(calls.lock().unwrap().join(","), "a");
            } else {
                let b = result.unwrap();
                assert_eq!(calls.lock().unwrap().join(","), "a,b");
                assert_eq!(
                    report(&b).attempts[0]
                        .detail
                        .as_ref()
                        .unwrap()
                        .chars()
                        .count(),
                    1024
                );
            }
        }
    }
}
#[cfg(feature = "documents")]
#[test]
fn native_parse_errors_remain_typed_and_terminal() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = OmniAttach::builder()
        .with_document_defaults()
        .unwrap()
        .register(
            route("b"),
            Script {
                id: "b",
                step: Step::Text("ok"),
                calls: calls.clone(),
            },
        )
        .unwrap()
        .build();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.pdf");
    fs::write(&path, b"%PDF-broken").unwrap();
    let (_, err) = unpack(
        e.convert_with_fallback(
            &ConversionRequest::new(path, Goal::Markdown),
            &FacadeContext::default(),
            &FallbackPolicy::new(["documents", "b"]),
        )
        .unwrap_err(),
    );
    assert!(matches!(*err, Error::Document(_)));
    assert!(calls.lock().unwrap().is_empty());
}
