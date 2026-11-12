#![cfg(feature = "backends")]
use omniattach::*;
use std::{fs, sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Duration};
const MD: &str = "# Title\n\n![figure](images/a.png)\n\n| col |\n| --- |\n| 1 |\n\n$$x^2$$\n";
struct Fixture { marker: bool, calls: AtomicUsize }
impl backends::Runner for Fixture {
    fn run(&self, spec: &backends::CommandSpec, options: &backends::RunOptions) -> backends::Result<backends::ProcessOutput> {
        assert!(!options.cancellation.is_cancelled()); self.calls.fetch_add(1, Ordering::SeqCst);
        let root = if self.marker { spec.cwd.join("out/input") } else { spec.cwd.join("out") };
        fs::create_dir_all(root.join("images"))?;
        fs::write(root.join("document.md"), MD)?; fs::write(root.join("images/a.png"), b"png fixture")?; fs::write(root.join("content.json"), b"{\"blocks\":[] }")?;
        Ok(backends::ProcessOutput { stdout: Vec::new(), stderr: Vec::new(), elapsed: Duration::ZERO })
    }
}
fn input() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("paper.pdf"); fs::write(&path, b"%PDF-1.7\nfixture").unwrap(); (dir, path)
}
#[test]
fn both_layout_backends_export_intact_relative_images_after_backend_bundle_drops() {
    for marker in [false, true] {
        let runner = Arc::new(Fixture { marker, calls: AtomicUsize::new(0) });
        let builder = OmniAttach::builder();
        let engine = if marker { builder.with_marker_runner(backends::Marker::default(), runner.clone()) } else { builder.with_mineru_runner(backends::MinerU::default(), runner.clone()) }.unwrap().build();
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
        let (_dir, path) = input(); let id = if marker { "marker" } else { "mineru" };
        let bundle = engine.convert(&ConversionRequest::new(&path, Goal::Markdown).with_route(id), &FacadeContext::default().with_external(true)).unwrap();
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1); assert_eq!(bundle.plain_text(), MD);
        let payload = bundle.payload("omniattach/layout-package/1").unwrap();
        let main = payload["markdown_artifact"].as_str().unwrap().to_owned();
        assert!(main.starts_with("layout/result/")); assert_eq!(bundle.manifest().text[0].location["artifact"], main);
        assert!(bundle.manifest().artifacts.iter().all(|a| a.path.starts_with("layout/result/")));
        assert_eq!(bundle.manifest().source.sha256, bundle.payload("omniattach-backends/0.1").unwrap()["source"]["sha256"].as_str().unwrap());
        let dir = tempfile::tempdir().unwrap(); let output = dir.path().join("export"); bundle.export(&output).unwrap(); drop(bundle);
        let md_path = output.join(&main); assert_eq!(fs::read_to_string(&md_path).unwrap(), MD);
        assert_eq!(fs::read(md_path.parent().unwrap().join("images/a.png")).unwrap(), b"png fixture");
        assert!(md_path.parent().unwrap().join("content.json").is_file());
    }
}
#[test]
fn requires_external_permission_rejects_request_options_and_honors_budgets() {
    let runner = Arc::new(Fixture { marker: true, calls: AtomicUsize::new(0) });
    let engine = OmniAttach::builder().with_marker_runner(backends::Marker::default(), runner.clone()).unwrap().build();
    let (_dir, path) = input(); let request = ConversionRequest::new(&path, Goal::Markdown).with_route("marker");
    assert!(matches!(engine.convert(&request, &FacadeContext::default()), Err(Error::ExternalDisabled)));
    let options = ConversionRequest::new(&path, Goal::Markdown).with_route("marker").with_options(serde_json::json!({"dehyphenate_max_tokens":6}));
    assert!(matches!(engine.convert(&options, &FacadeContext::default().with_external(true)), Err(Error::Configuration(_))));
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    let mut context = FacadeContext::default().with_external(true); context.policy.max_export_files = 1;
    assert!(engine.convert(&request, &context).is_err());
    let context = FacadeContext::default().with_external(true); context.cancellation.cancel();
    assert!(matches!(engine.convert(&request, &context), Err(Error::Cancelled)));
}
#[test]
fn multiple_layout_routes_require_explicit_selection_and_defaults_do_not_add_them() {
    let engine = OmniAttach::builder().with_external_defaults().unwrap().build();
    assert!(!engine.capabilities().iter().any(|r| r.id == "mineru" || r.id == "marker"));
    let runner = Arc::new(Fixture { marker: false, calls: AtomicUsize::new(0) });
    let engine = OmniAttach::builder().with_mineru_runner(backends::MinerU::default(), runner.clone()).unwrap().with_marker_runner(backends::Marker::default(), runner.clone()).unwrap().build();
    let (_dir, path) = input();
    assert!(matches!(engine.plan(&ConversionRequest::new(&path, Goal::Markdown), &FacadeContext::default().with_external(true)), Err(Error::AmbiguousRoute(_))));
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
}
#[cfg(feature = "documents")]
#[test]
fn native_document_route_remains_available_without_external_permission() {
    let engine = OmniAttach::builder().with_document_defaults().unwrap().with_marker(backends::Marker::default()).unwrap().build();
    let (_dir, path) = input();
    assert_eq!(engine.plan(&ConversionRequest::new(&path, Goal::Markdown), &FacadeContext::default()).unwrap().route.id, "documents");
}
