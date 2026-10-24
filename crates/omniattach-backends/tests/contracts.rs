use omniattach_backends::*;
use std::{path::Path, time::Duration};
struct Fake<F>(F);
impl<F> Runner for Fake<F>
where
    F: Fn(&CommandSpec) -> Result<ProcessOutput> + Send + Sync,
{
    fn run(&self, command: &CommandSpec, _: &RunOptions) -> Result<ProcessOutput> {
        (self.0)(command)
    }
}
fn output(text: &str) -> ProcessOutput {
    ProcessOutput {
        stdout: text.as_bytes().to_vec(),
        stderr: b"backend diagnostic".to_vec(),
        elapsed: Duration::from_millis(2),
    }
}
fn source() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("a $(echo unsafe).pdf");
    std::fs::write(&file, b"sample input").unwrap();
    (temp, file)
}
fn custom(output: CustomOutput) -> CustomBackend {
    CustomBackend {
        name: "custom".into(),
        tool: Tool::new("fake-tool"),
        args: vec!["--input".into(), "{input}".into(), "{output_dir}".into()],
        output,
    }
}
#[test]
fn pdf_stages_hashes_input_and_records_invocation() {
    let (_dir, source) = source();
    let fake = Fake(|command: &CommandSpec| {
        assert_eq!(command.args.last().unwrap(), "-");
        assert_eq!(
            std::fs::read(command.cwd.join("input.pdf")).unwrap(),
            b"sample input"
        );
        assert!(
            !command
                .args
                .iter()
                .any(|v| v.to_string_lossy().contains("echo unsafe"))
        );
        Ok(output("Page 1\u{c}\u{c}"))
    });
    let result = Poppler::default()
        .convert(&source, &BackendContext::new(&fake))
        .unwrap();
    assert_eq!(result.manifest.text.len(), 2);
    assert_eq!(result.manifest.source.bytes, 12);
    assert_eq!(result.manifest.source.sha256.len(), 64);
    assert!(
        result
            .manifest
            .diagnostics
            .iter()
            .any(|d| d.code == "EMPTY_PAGES")
    );
    assert_eq!(
        result.manifest.invocations[0].stderr_tail,
        "backend diagnostic"
    );
}
#[test]
fn artifact_bundle_owns_lifetime_and_exports_only_declared_artifacts() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(c.cwd.join("out/result.bin"), b"result")?;
        Ok(output(""))
    });
    let result = custom(CustomOutput::File {
        path: "out/result.bin".into(),
        media_type: "application/octet-stream".into(),
    })
    .convert(&source, &BackendContext::new(&fake))
    .unwrap();
    let root = result.root().to_owned();
    let asset = result.asset_path(0).unwrap();
    assert!(asset.exists());
    let export_parent = tempfile::tempdir().unwrap();
    let dest = export_parent.path().join("export");
    result.export(&dest).unwrap();
    assert!(dest.join("out/result.bin").is_file());
    assert!(!dest.join("input.pdf").exists());
    assert!(dest.join("manifest.json").is_file());
    assert!(result.export(&dest).is_err());
    let json = result.manifest.to_json().unwrap();
    let manifest: Manifest = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest, result.manifest);
    drop(result);
    assert!(!root.exists());
    assert_eq!(
        std::fs::read(dest.join("out/result.bin")).unwrap(),
        b"result"
    );
}
#[test]
fn failure_cleans_scratch_and_input_has_no_sidecar() {
    let (dir, source) = source();
    let scratch = tempfile::tempdir().unwrap();
    let fake = Fake(|_: &CommandSpec| Err(Error::InvalidOutput("test failure".into())));
    let mut ctx = BackendContext::new(&fake);
    ctx.scratch_parent = Some(scratch.path().into());
    assert!(Poppler::default().convert(&source, &ctx).is_err());
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn input_output_limits_and_missing_expected_output_are_errors() {
    let (_dir, source) = source();
    let fake = Fake(|_: &CommandSpec| Ok(output("")));
    let mut ctx = BackendContext::new(&fake);
    ctx.limits.input_bytes = 3;
    assert!(matches!(
        Poppler::default().convert(&source, &ctx),
        Err(Error::Limit(_))
    ));
    let ctx = BackendContext::new(&fake);
    assert!(
        custom(CustomOutput::File {
            path: "out/missing".into(),
            media_type: "text/plain".into()
        })
        .convert(&source, &ctx)
        .is_err()
    );
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(c.cwd.join("out/result"), b"123456")?;
        Ok(output(""))
    });
    let mut ctx = BackendContext::new(&fake);
    ctx.limits.artifact_bytes = 5;
    assert!(matches!(
        custom(CustomOutput::File {
            path: "out/result".into(),
            media_type: "text/plain".into()
        })
        .convert(&source, &ctx),
        Err(Error::Limit(_))
    ));
}
#[test]
fn custom_output_cannot_escape_workspace() {
    let (_dir, source) = source();
    let fake = Fake(|_: &CommandSpec| Ok(output("")));
    for path in [
        "../outside",
        "/etc/passwd",
        "out/../input.pdf",
        "out\\escape",
    ] {
        assert!(
            custom(CustomOutput::File {
                path: path.into(),
                media_type: "text/plain".into()
            })
            .convert(&source, &BackendContext::new(&fake))
            .is_err()
        );
    }
}
#[cfg(unix)]
#[test]
fn symlink_artifacts_are_rejected() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        std::os::unix::fs::symlink("/etc/passwd", c.cwd.join("out/link"))?;
        Ok(output(""))
    });
    assert!(matches!(
        custom(CustomOutput::File {
            path: "out/link".into(),
            media_type: "text/plain".into()
        })
        .convert(&source, &BackendContext::new(&fake)),
        Err(Error::InvalidOutput(_))
    ));
}
#[test]
fn successful_exit_is_not_proof_of_office_conversion() {
    let (_dir, source) = source();
    let fake = Fake(|_: &CommandSpec| Ok(output("converted")));
    assert!(
        LibreOffice::new(OfficeFormat::Pdf)
            .convert(&source, &BackendContext::new(&fake))
            .is_err()
    );
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(c.cwd.join("out/input.pdf"), b"wrong")?;
        Ok(output(""))
    });
    assert!(matches!(
        LibreOffice::new(OfficeFormat::Pdf).convert(&source, &BackendContext::new(&fake)),
        Err(Error::InvalidOutput(_))
    ));
}
#[test]
fn libreoffice_uses_fresh_profile_and_expected_output() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        assert!(
            c.args[0]
                .to_string_lossy()
                .starts_with("-env:UserInstallation=file:///")
        );
        assert!(c.args.iter().any(|v| v == "--headless"));
        std::fs::write(c.cwd.join("out/input.docx"), b"PK\x03\x04minimal")?;
        Ok(output(""))
    });
    let result = LibreOffice::new(OfficeFormat::Docx)
        .convert(&source, &BackendContext::new(&fake))
        .unwrap();
    assert_eq!(result.manifest.assets[0].kind, AssetKind::ConvertedDocument);
}
#[test]
fn custom_markdown_and_transcript_contracts() {
    let (_dir, source) = source();
    let fake = Fake(|_: &CommandSpec| Ok(output("# Title\n\nBody")));
    let result = custom(CustomOutput::MarkdownStdout)
        .convert(&source, &BackendContext::new(&fake))
        .unwrap();
    assert_eq!(result.manifest.text[0].kind, TextKind::Markdown);
    let fake = Fake(|_: &CommandSpec| Ok(output(" ")));
    assert!(
        custom(CustomOutput::MarkdownStdout)
            .convert(&source, &BackendContext::new(&fake))
            .is_err()
    );
    let fake = Fake(|_: &CommandSpec| {
        Ok(output(
            r#"{"segments":[{"text":"hello","start":0,"end":1}]}"#,
        ))
    });
    let result = custom(CustomOutput::TranscriptStdout {
        format: TranscriptFormat::WhisperSegments,
    })
    .convert(&source, &BackendContext::new(&fake))
    .unwrap();
    assert_eq!(result.manifest.text[0].location.end_ms, Some(1000));
}
#[test]
fn whisper_cpp_command_contract_is_explicit() {
    let (dir, source) = source();
    let model = dir.path().join("model.bin");
    std::fs::write(&model, b"fake model fixture").unwrap();
    let fake = Fake(|c: &CommandSpec| {
        if c.program == Path::new("ffmpeg") {
            let mut wav = b"RIFF".to_vec();
            wav.extend_from_slice(&[0; 4]);
            wav.extend_from_slice(b"WAVE");
            wav.extend_from_slice(&[0; 32]);
            std::fs::write(c.cwd.join("audio.wav"), wav)?;
        } else {
            assert!(c.args.iter().any(|v| v == "--output-json"));
            std::fs::write(
                c.cwd.join("transcript.json"),
                br#"{"transcription":[{"text":"hello","offsets":{"from":0,"to":10}}]}"#,
            )?;
        }
        Ok(output(""))
    });
    let mut backend = WhisperCpp::new(model);
    backend.keep_audio = true;
    let result = backend
        .convert(&source, &BackendContext::new(&fake))
        .unwrap();
    assert_eq!(result.manifest.invocations.len(), 2);
    assert_eq!(result.manifest.assets.len(), 2);
}
#[test]
fn registry_is_explicit_and_registration_has_no_tool_side_effect() {
    let mut registry = Registry::new();
    registry.register(Poppler::default()).unwrap();
    assert!(registry.register(Poppler::default()).is_err());
    assert_eq!(
        registry.capabilities(),
        vec![("poppler", Capability::PdfText)]
    );
    let fake = Fake(|_: &CommandSpec| panic!("unknown backend must not invoke runner"));
    assert!(matches!(
        registry.convert("absent", "missing", &BackendContext::new(&fake)),
        Err(Error::UnknownBackend(_))
    ));
}
#[test]
fn invalid_frame_configuration_fails_before_execution() {
    let fake = Fake(|_: &CommandSpec| panic!("invalid config must not invoke runner"));
    let ctx = BackendContext::new(&fake);
    let backend = FfmpegFrames {
        selection: FrameSelection::SceneCuts {
            threshold: f64::NAN,
        },
        ..Default::default()
    };
    assert!(backend.convert(Path::new("missing"), &ctx).is_err());
    let backend = FfmpegFrames {
        selection: FrameSelection::Interval { interval_ms: 0 },
        ..Default::default()
    };
    assert!(backend.convert(Path::new("missing"), &ctx).is_err());
}
#[test]
fn third_party_backend_can_create_and_finish_its_own_workspace() {
    let (_dir, source) = source();
    let limits = Limits::default();
    let job = JobWorkspace::new(&source, None, &limits).unwrap();
    std::fs::write(job.root().join("result.txt"), b"external adapter").unwrap();
    let mut manifest = Manifest::new("third-party", job.source.clone());
    manifest.assets.push(
        job.asset(
            "result.txt",
            "text/plain",
            AssetKind::Attachment,
            Location::default(),
            &limits,
        )
        .unwrap(),
    );
    let bundle = job.finish(manifest, &limits).unwrap();
    assert_eq!(bundle.manifest.backend, "third-party");
}
#[test]
fn final_manifest_validation_rejects_forged_sizes_and_duplicate_paths() {
    let (_dir, source) = source();
    let limits = Limits::default();
    for duplicate in [false, true] {
        let job = JobWorkspace::new(&source, None, &limits).unwrap();
        std::fs::write(job.root().join("result.txt"), b"result").unwrap();
        let mut manifest = Manifest::new("external", job.source.clone());
        let mut asset = job
            .asset(
                "result.txt",
                "text/plain",
                AssetKind::Attachment,
                Location::default(),
                &limits,
            )
            .unwrap();
        if duplicate {
            manifest.assets.push(asset.clone());
        } else {
            asset.bytes = 1;
        }
        manifest.assets.push(asset);
        assert!(matches!(
            job.finish(manifest, &limits),
            Err(Error::InvalidOutput(_))
        ));
    }
}
#[test]
fn tesseract_adapter_uses_tsv_language_and_segmentation_arguments() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        let args: Vec<_> = c
            .args
            .iter()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|v| v == ["-l", "hun+eng"]));
        assert!(args.windows(2).any(|v| v == ["--psm", "6"]));
        assert_eq!(args.last().unwrap(), "tsv");
        Ok(output(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t85\tword\n",
        ))
    });
    let backend = Tesseract {
        language: "hun+eng".into(),
        page_segmentation: Some(6),
        ..Default::default()
    };
    let result = backend
        .convert(&source, &BackendContext::new(&fake))
        .unwrap();
    assert_eq!(result.manifest.text[0].confidence, Some(85.0));
}
#[test]
fn custom_file_contracts_read_specific_outputs_not_other_candidates() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(c.cwd.join("out/document.md"), "# Chosen")?;
        std::fs::write(c.cwd.join("out/other.md"), "Wrong")?;
        Ok(output(""))
    });
    let result = custom(CustomOutput::MarkdownFile {
        path: "out/document.md".into(),
    })
    .convert(&source, &BackendContext::new(&fake))
    .unwrap();
    assert_eq!(result.manifest.text[0].text, "# Chosen");
    assert_eq!(result.manifest.assets.len(), 1);
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(
            c.cwd.join("out/transcript.json"),
            br#"{"segments":[{"text":"Hi","start":0.25,"end":0.9}]}"#,
        )?;
        Ok(output(""))
    });
    let result = custom(CustomOutput::TranscriptFile {
        path: "out/transcript.json".into(),
        format: TranscriptFormat::WhisperSegments,
    })
    .convert(&source, &BackendContext::new(&fake))
    .unwrap();
    assert_eq!(result.manifest.text[0].location.start_ms, Some(250));
}
#[test]
fn total_artifact_budget_counts_all_declared_files() {
    let (_dir, source) = source();
    let limits = Limits {
        artifact_bytes: 5,
        ..Default::default()
    };
    let job = JobWorkspace::new(&source, None, &limits).unwrap();
    let mut manifest = Manifest::new("test", job.source.clone());
    for path in ["a", "b"] {
        std::fs::write(job.root().join(path), b"123").unwrap();
        manifest.assets.push(
            job.asset(
                path,
                "text/plain",
                AssetKind::Attachment,
                Location::default(),
                &limits,
            )
            .unwrap(),
        );
    }
    assert!(matches!(
        job.finish(manifest, &limits),
        Err(Error::Limit(_))
    ));
}
#[test]
fn frame_output_requires_timestamp_correspondence() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(
            c.cwd.join("frames/frame-000001.png"),
            b"\x89PNG\r\n\x1a\nfixture",
        )?;
        Ok(output(""))
    });
    assert!(matches!(
        FfmpegFrames::default().convert(&source, &BackendContext::new(&fake)),
        Err(Error::InvalidOutput(_))
    ));
}
#[test]
fn export_of_tampered_manifest_rejects_traversal_before_writing_outside() {
    let (_dir, source) = source();
    let fake = Fake(|c: &CommandSpec| {
        std::fs::write(c.cwd.join("out/result"), b"ok")?;
        Ok(output(""))
    });
    let mut bundle = custom(CustomOutput::File {
        path: "out/result".into(),
        media_type: "text/plain".into(),
    })
    .convert(&source, &BackendContext::new(&fake))
    .unwrap();
    bundle.manifest.assets[0].path = "../escape".into();
    let parent = tempfile::tempdir().unwrap();
    assert!(bundle.export(parent.path().join("export")).is_err());
    assert!(!parent.path().join("escape").exists());
    assert!(!parent.path().join("export").exists());
}
