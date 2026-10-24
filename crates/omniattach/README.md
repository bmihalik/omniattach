# omniattach

Public conversion facade for the `crates/omniattach/` position in the proposed omniattach workspace.

**Compatibility note:** the supplied attachment was a directory/module layout, not the existing Rust sources or public API signatures. The public GitHub repository had no committed files during this delivery. This release therefore contains a working, tested **portable facade**, optional sibling crate re-exports, a signature-neutral document bridge, and an optional backend bridge targeting the exact 0.1 API shown in the preceding backend delivery. It does not claim that the complete unpublished workspace was compiled.

There are no replacement `omniattach-core`, documents or backend crates and no mock siblings in this archive. Existing sibling files and the workspace root manifest are not overwritten.

## Install into the existing tree

The archive paths begin with `crates/omniattach/`. Extract from the repository root:

```sh
tar -xzf /path/to/downloaded-archive.tar.gz
```

The result is:

```text
omniattach/
├── Cargo.toml                       # your existing workspace root, unchanged
└── crates/
    ├── omniattach-core/             # your existing crate, unchanged
    ├── omniattach-documents/        # your existing crate, unchanged
    ├── omniattach-backends/         # your existing crate, unchanged
    └── omniattach/                  # this delivery
        ├── Cargo.toml
        ├── src/
        ├── tests/
        ├── examples/
        ├── docs/
        └── verification/
```

Ensure the root `[workspace].members` includes `crates/omniattach`, or uses the existing `crates/*` glob. Do not add an empty CLI crate until it has its own valid manifest. This crate's dependencies use literal sibling paths, so it does not require particular `[workspace.dependencies]` aliases.

```sh
# Actual workspace build; all sibling manifests must exist.
cargo test -p omniattach
cargo clippy -p omniattach --all-targets -- -D warnings

# Enable the real sibling re-exports and optional external adapter.
# This check must be run against your actual sources.
cargo test -p omniattach --features workspace
```

The root workspace owns its lockfile. This archive does not replace it. Sibling path dependencies are resolved by Cargo even when optional, so an isolated copied main manifest will not resolve without the sibling manifests. For isolated verification without siblings, use the independent manifest below; it compiles the **same facade source files**, without fake dependency crates:

```sh
cd crates/omniattach
cargo test --manifest-path verification/Cargo.toml --locked
cargo clippy --manifest-path verification/Cargo.toml --locked --all-targets -- -D warnings
```

Do not enable sibling features or `--all-features` on the isolated harness. Those features require the real workspace and are not substitutes for its crates.

## Included behavior

| Area | Implementation |
|---|---|
| Builder and registry | `OmniAttach`, `OmniAttachBuilder`, explicit `Route` records |
| Planning | Format + goal + optional route selection; errors on ambiguity |
| Portable defaults | Strict UTF-8/UTF-16 text/Markdown extraction and Markdown artifacts |
| General document bridge | `DocumentAdapter` preserves a serializable extractor result unchanged |
| Optional backend bridge | `ExternalAdapter`, external default registration, explicit Whisper model |
| Pipelines | `ArtifactPipeline`, explicit intermediate artifact and source relationship |
| Inputs | Job-local regular-file staging and source SHA-256 |
| Results | Owned `FacadeBundle`, namespaced payloads, text fragments, declared artifacts |
| Runtime | External-execution opt-in, cooperative cancellation/deadline, progress observer |
| Export | New destination only; declared artifacts plus `manifest.json` |

Not implemented here: parsers duplicating the document crate, a replacement core schema/planner, embeddings/chunking, CLI subcommands, async scheduling, automatic backend installation, cloud/network input, quality-based fallback, a cache, or inference quality checks.

## Minimal usage

```rust,no_run
use omniattach::{ConversionRequest, FacadeContext, Goal, OmniAttach};

fn main() -> omniattach::Result<()> {
    let engine = OmniAttach::new();
    let request = ConversionRequest::new("notes.txt", Goal::Markdown);
    let bundle = engine.convert(&request, &FacadeContext::default())?;
    bundle.export("new-output-directory")?;
    println!("{}", bundle.manifest().to_json()?);
    Ok(())
}
```

`OmniAttach::new()` registers only `portable-text`, not imaginary installed converters. `OmniAttach::builder()` starts empty. Call `with_portable_defaults()` explicitly on the builder to add those defaults.

A text-to-Markdown conversion uses a fenced block long enough to preserve backticks in the source. Existing Markdown bytes are decoded and retained without parsing/reconstructing headings. This is preservation, not document layout inference. A whole-source fragment location is explicitly `{"kind":"whole_source"}` rather than invented source line/page positions.

## Strict route selection

Each route declares supported `Format` values and `Goal` values, plus `ExecutionKind`. Automatic selection succeeds only when exactly one eligible permitted route exists. Multiple eligible routes are an error; insertion order never chooses a winner. An explicitly selected route must match the format and goal and pass policy. It will not fall back on failure.

`Goal::Extract`, `PdfLayout`, `PdfScientific`, `Ocr`, `Transcribe`, `VideoFrames`, `MediaProbe`, `Normalize`, `Tables`, `Markdown` and `Custom` are separate route goals. Merely naming a goal does not install support for it. Registered capabilities describe contracts, not installed tool health or model availability.

Detection is a conservative facade prefix/extension router, not the richer document crate detector. A few signatures override conflicting extensions. ZIP Office families use extension hints, explicitly diagnosed as unverified; archives are not unpacked or classified here. Unknown inputs require `request.with_format(...)`. Explicit overrides are retained as warnings. `plan()` is advisory; `convert()` stages then redetects the actual copied bytes.

## Connecting omniattach-documents

The actual extraction signature was not supplied, so no guessed function names appear in production code. Adapt your real API with one closure:

```rust,no_run
use omniattach::*;
use serde_json::{Value, json};
use std::path::Path;

fn main() -> Result<()> {
    // This runnable example uses text; replace ONLY the closure body with your
    // actual documents:: extraction API and return its serializable document.
    let adapter = DocumentAdapter::new(
        "my-document-schema/1",
        |path: &Path, options: &Value| {
            std::fs::read_to_string(path)
                .map(|text| json!({"text": text, "options": options}))
        },
    );
    let engine = OmniAttach::builder().register(
        Route::new("my-documents", vec![Format::Text], vec![Goal::Extract], ExecutionKind::InProcess),
        adapter,
    )?.build();
    let output = engine.convert(
        &ConversionRequest::new("notes.txt", Goal::Extract),
        &FacadeContext::default(),
    )?;
    println!("{}", output.payload("my-document-schema/1").unwrap());
    Ok(())
}
```

The result must implement `Serialize`. Options are forwarded unchanged to your extractor closure. The bridge does not inspect guessed `blocks`, `text` or `table` fields: it preserves the actual schema as a payload. To populate facade text/artifacts as well, implement `Converter` directly or wrap your document extraction closure with your own mapping. XML probe/table specs are passed through the real document/core API, not recreated or silently ignored by this facade.

## Sibling re-exports

| Cargo feature | Public namespace | Automatically registered? |
|---|---|---|
| `core` | `omniattach::core` | No; actual core types are re-exported without assumptions |
| `documents` | `omniattach::documents` | No; supply the actual extraction closure |
| `backends` | `omniattach::backends` plus `ExternalAdapter` | No; registration is explicit |
| `workspace` | All of the above | Still explicit |

Facade types intentionally use `FacadeBundle`, `FacadeManifest`, `FacadeContext` and `JobContext` rather than claiming they equal the unpublished core's `ConversionBundle`, `Document`, `ExecutionContext` or `BackendRegistry`. This version's planner/progress implementation is independent. Consolidating it onto the existing core planner/progress traits requires the actual core signatures. That is a known integration task, not a hidden compatibility promise.

## External backend integration

With the real backend sibling and `--features backends`:

```rust,ignore
use omniattach::*;
let engine = OmniAttach::builder()
    .with_portable_defaults()?
    .with_external_defaults()?
    .with_whisper_cpp("/models/ggml-base.bin", "hu")?
    .build();
let context = FacadeContext::default().with_external(true);
let bundle = engine.convert(
    &ConversionRequest::new("video.mp4", Goal::VideoFrames)
        .with_route("ffmpeg-frames"),
    &context,
)?;
bundle.export("video-output")?;
```

Optional external defaults target the previously delivered backend 0.1 public API: Poppler, Tesseract, FFmpeg audio/frames, FFprobe, LibreOffice DOC/RTF-to-DOCX, PPT-to-PPTX, XLS-to-XLSX. Whisper is separate because a model must be chosen explicitly. This module and `tests/backend_bridge.rs` are provided for integration but **not compiled in the isolated verification** because the actual sibling source files were unavailable. No MinerU/Marker/native Whisper/PDFium methods are invented from filenames in the layout.

`ExternalAdapter::with_runner(Arc<dyn backends::Runner>)` supports your real backend runner/policy. Backend assets are copied into the facade output directory before the backend bundle drops. The nested original manifest is retained unchanged, and a warning states that its original asset paths are no longer the exported paths. Use facade `artifacts` (same index order) for file access; use the payload for original converter details. Original source identity remains at the facade top level; nested backend source names refer to staged files.

Per-request `options` are rejected by the default external bridge unless empty/null, rather than silently ignored. Configure the concrete backend first for OCR language, scene threshold, tool path or model choices. External execution defaults to disabled and must be explicitly allowed. ExecutionKind is a routing declaration, not a hostile-code security guarantee.

## Explicit multi-step conversion

`ArtifactPipeline` composes two real `Converter`s, with an explicit `artifact_index`, `first_goal` and `intermediate_format`. The second stage receives the selected intermediate file, its actual hash/size, and the final request options. Intermediate and final declared artifacts are retained under `stage-1/` and `stage-2/` and count toward the same final export budget. The first-stage text is preserved in a payload rather than mixed with final text. Any failure stops the pipeline.

For example, configure a real LibreOffice `ExternalAdapter` as the first stage and your real document adapter as the second stage. The enclosing route **must be declared External** if either stage runs external tools. There is no implicit conversion graph that chooses a lossy intermediate behind the caller's back.

## Progress and runtime policy

`FacadeContext` carries `Policy`, a cloneable cancellation token and an `Arc<dyn ProgressObserver>`. `NdjsonProgress` emits one locked stderr JSON line per event; stdout remains available for the result. The default observer is silent. Progress stages are staging, converting, completed or failed; no unmeasured percentage is invented. Errors rejected before staging do not produce a full event sequence.

The deadline includes source staging and is checked between cooperative steps and after converter return. It cannot forcibly stop arbitrary in-process converter code, blocking filesystem operations or observer callbacks. An optional backend's process timeout is bounded by remaining facade time, and shares cancellation; backend multi-process work may still finish a later invocation before the facade rejects an overdue result. No hard whole-job execution guarantee is claimed.

Input byte limits are enforced while staging. Declared output count/total bytes are checked after conversion. These are **not** a live disk quota, process memory budget or JSON payload size bound. Converters/observers are trusted code. See `docs/SECURITY.md`.

## Verification

The portable facade was compiled on Linux x86_64 with Rust 1.99.1. Its **39 integration tests and one compiled documentation example passed**, as did formatting, warning-denying Clippy and warning-denying rustdoc. Both portable examples were executed and their JSON/artifact outputs checked.

The complete sibling workspace, optional backend bridge, actual document extraction, core planner compatibility, external tool inference and native media conversion were **not** verified in this delivery. The optional bridge tests are supplied so your actual workspace can establish that compatibility without fake stand-in crates. The independent `verification/Cargo.lock` is included; no root Cargo.lock is generated against missing source crates.

License: Apache-2.0 for this crate. No model weights, binaries, dependency vendor tree, generated sibling crate or root-manifest replacement is included.
