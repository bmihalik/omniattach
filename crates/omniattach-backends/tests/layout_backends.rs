use omniattach_backends::*;
use std::{fs, sync::Mutex, time::Duration};

struct Fake<'a> {
    files: &'a [(&'a str, &'a [u8])],
    commands: Mutex<Vec<CommandSpec>>,
    #[cfg(unix)] link: bool,
}
impl<'a> Fake<'a> {
    fn new(files: &'a [(&'a str, &'a [u8])]) -> Self { Self { files, commands: Mutex::new(Vec::new()), #[cfg(unix)] link: false } }
}
impl Runner for Fake<'_> {
    fn run(&self, spec: &CommandSpec, options: &RunOptions) -> Result<ProcessOutput> {
        if options.cancellation.is_cancelled() { return Err(Error::Cancelled); }
        self.commands.lock().unwrap().push(spec.clone());
        for (path, bytes) in self.files {
            let path = spec.cwd.join(path);
            fs::create_dir_all(path.parent().unwrap())?; fs::write(path, bytes)?;
        }
        #[cfg(unix)]
        if self.link { std::os::unix::fs::symlink(&spec.cwd, spec.cwd.join("out/images"))?; }
        Ok(ProcessOutput { stdout: b"converter log".to_vec(), stderr: Vec::new(), elapsed: Duration::ZERO })
    }
}
fn input() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("book ü.pdf"); fs::write(&path, b"%PDF-1.7\nfixture").unwrap(); (dir, path)
}
fn convert(backend: &dyn Backend, files: &[(&str, &[u8])]) -> Result<(ArtifactBundle, Vec<CommandSpec>)> {
    let (_dir, path) = input(); let runner = Fake::new(files);
    let bundle = backend.convert(&path, &BackendContext::new(&runner))?;
    Ok((bundle, runner.commands.into_inner().unwrap()))
}
#[test]
fn mineru_sdk_preserves_images_json_markdown_and_export_after_drop() {
    let md = b"# Title\n\n![figure](images/a.png)\n\n| col |\n| --- |\n| 1 |\n\n$$x^2$$\n";
    let (bundle, commands) = convert(&MinerU::default(), &[("out/markdown.md", md), ("out/images/a.png", b"png fixture"), ("out/middle_json.json", b"{\"pages\":[]}")]).unwrap();
    assert_eq!(commands.len(), 1); assert_eq!(commands[0].program, std::path::PathBuf::from("python3"));
    assert_eq!(commands[0].args[0], "-c"); assert!(commands[0].args[1].to_string_lossy().contains(".save("));
    assert_eq!(bundle.manifest.text[0].text.as_bytes(), md);
    assert_eq!(bundle.manifest.metadata["layout_package"]["markdown_asset"], "result/markdown.md");
    assert_eq!(bundle.manifest.assets.len(), 3);
    let dir = tempfile::tempdir().unwrap(); let export = dir.path().join("export"); bundle.export(&export).unwrap(); drop(bundle);
    assert_eq!(fs::read(export.join("result/images/a.png")).unwrap(), b"png fixture");
    assert_eq!(fs::read(export.join("result/markdown.md")).unwrap(), md);
    assert!(export.join("result/middle_json.json").is_file());
}
#[test]
fn marker_and_legacy_mineru_have_separate_explicit_command_contracts() {
    let files = [("out/input/input.md", b"![fig](_page.png)".as_slice()), ("out/input/_page.png", b"png".as_slice()), ("out/input/input_meta.json", b"{}".as_slice())];
    let marker = Marker { mode: Some(MarkerMode::Fast), force_ocr: true, ..Default::default() };
    let (bundle, commands) = convert(&marker, &files).unwrap();
    let args: Vec<_> = commands[0].args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(commands[0].program, std::path::PathBuf::from("marker_single"));
    assert!(args.windows(2).any(|a| a == ["--output_format", "markdown"]));
    assert!(args.windows(2).any(|a| a == ["--mode", "fast"])); assert!(args.contains(&"--force_ocr".into()));
    assert_eq!(bundle.manifest.metadata["layout_package"]["markdown_asset"], "result/input/input.md");
    let (_, commands) = convert(&MinerU::new(MinerUProfile::LegacyPipeline), &files).unwrap();
    assert_eq!(commands[0].program, std::path::PathBuf::from("mineru")); assert_eq!(commands[0].args[0], "-p");
    assert!(commands[0].args.iter().any(|a| a == "pipeline"));
}
#[test]
fn validates_reference_links_html_images_encoded_names_and_keeps_code_literal() {
    let md = b"![a][image]\n\n[image]: images/a%20b.png\n\n<img src=\"images/a%20b.png\">\n\n[site](https://example.org)\n\n```\n![not a real image](missing.png)\n```\n";
    let (bundle, _) = convert(&Marker::default(), &[("out/main.md", md), ("out/images/a b.png", b"png")]).unwrap();
    assert!(bundle.manifest.diagnostics.iter().any(|d| d.code == "LAYOUT_EXTERNAL_LINKS"));
}
#[test]
fn missing_ambiguous_empty_and_invalid_sidecar_outputs_fail() {
    for files in [vec![], vec![("out/main.md", b"".as_slice())], vec![("out/a.md", b"one".as_slice()), ("out/b.md", b"two".as_slice())], vec![("out/a.md", b"body".as_slice()), ("out/a.json", b"invalid".as_slice())]] {
        assert!(convert(&Marker::default(), &files).is_err());
    }
    let backend = Marker { output: LayoutOutputConfig { markdown_path: Some("a.md".into()) }, ..Default::default() };
    assert!(convert(&backend, &[("out/a.md", b"one"), ("out/b.md", b"two")]).is_ok());
}
#[test]
fn rejects_missing_traversing_absolute_remote_and_unsupported_image_references() {
    for md in ["![x](missing.png)", "![x](../outside.png)", "![x](%2e%2e/outside.png)", "![x](/etc/passwd)", "![x](file:///etc/passwd)", "![x](https://example.org/a.png)", "<img src=\"missing.png\">", "<img srcset=\"a.png 2x\" src=\"a.png\">"] {
        assert!(convert(&Marker::default(), &[("out/main.md", md.as_bytes())]).is_err(), "{md}");
    }
}
#[test]
fn enforces_artifact_budgets_and_cancellation() {
    let (_dir, path) = input();
    let files = [("out/main.md", b"![x](a.png)".as_slice()), ("out/a.png", b"png".as_slice())];
    let runner = Fake::new(&files); let mut ctx = BackendContext::new(&runner); ctx.limits.artifact_count = 1;
    assert!(matches!(Marker::default().convert(&path, &ctx), Err(Error::Limit(_))));
    let mut ctx = BackendContext::new(&runner); ctx.limits.artifact_bytes = 2;
    assert!(matches!(Marker::default().convert(&path, &ctx), Err(Error::Limit(_))));
    let ctx = BackendContext::new(&runner); ctx.run_options.cancellation.cancel();
    assert!(matches!(Marker::default().convert(&path, &ctx), Err(Error::Cancelled)));
}
#[cfg(unix)]
#[test]
fn rejects_symlinked_output_directories() {
    let (_dir, path) = input(); let files = [("out/main.md", b"body".as_slice())];
    let mut runner = Fake::new(&files); runner.link = true;
    assert!(Marker::default().convert(&path, &BackendContext::new(&runner)).is_err());
}
struct Failed;
impl Runner for Failed {
    fn run(&self, _: &CommandSpec, _: &RunOptions) -> Result<ProcessOutput> { Err(Error::ProcessFailed { code: Some(2), stderr: "fixture failure".into() }) }
}
#[test]
fn propagates_converter_failures_without_fallback() {
    let (_dir, path) = input();
    assert!(matches!(MinerU::default().convert(&path, &BackendContext::new(&Failed)), Err(Error::ProcessFailed { .. })));
}
