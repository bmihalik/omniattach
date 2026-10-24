# omniattach-backends

Opt-in **local external conversion backends** for the omniattach ecosystem. This is an implemented standalone Rust crate, complementary to `omniattach-documents`: documents handles parsing/rendering; backends runs installed converters and returns text units plus owned artifacts.

No tools or models are downloaded, no process starts during registration, and no backend silently falls back to another. There are no embedded native tool bindings or GPU runtimes.

## Implemented adapters

| Adapter | Rust type / registry ID | Output | Qualification |
|---|---|---|---|
| Poppler | `Poppler` / `poppler` | Page-located plain text | `pdftotext`, no OCR or Markdown structure inference |
| Tesseract | `Tesseract` / `tesseract` | Words, pixel boxes, native confidence | Image input, TSV contract; language data must be installed |
| LibreOffice | `LibreOffice` / `libreoffice` | DOCX/PPTX/XLSX/ODT/ODP/ODS/PDF artifact | Fresh temporary user profile, explicit output format |
| FFmpeg audio | `FfmpegAudio` / `ffmpeg-audio` | 16 kHz mono PCM s16le WAV | First audio stream, local file input |
| whisper.cpp | `WhisperCpp` / `whisper-cpp` | Millisecond transcript segments + raw JSON | Explicit local model, FFmpeg preprocessing |
| FFmpeg frames | `FfmpegFrames` / `ffmpeg-frames` | PNGs and selected-frame timestamps | First video stream; scene cuts or interval sampling |
| FFprobe | `Ffprobe` / `ffprobe` | Raw JSON metadata | Stream/container information |
| Custom command | `CustomBackend` / caller-defined | Markdown, transcript JSON or a declared file | Explicit trusted wrapper contract |

Marker, MinerU, Python Whisper and faster-whisper can be integrated through `CustomBackend` wrappers. There is deliberately **no claim of universal native command compatibility** for their differing CLI variants. A transcript wrapper may return a `segments` array in seconds or the whisper.cpp `transcription`/`offsets` contract in milliseconds. For arbitrary compressed media, the wrapper must preprocess it or call `FfmpegAudio` first.

Not included: OCR-PDF page rendering pipelines, live network feeds, semantic scene understanding/captioning, diarization, speaker alignment, cloud APIs, automatic model installation, automatic PDF backend selection, an async scheduler, FFI/Python bindings or direct `omniattach-core` dependency.

## Build

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

A current stable Rust toolchain is recommended. Verification used Rust/Cargo 1.99.1 on Linux x86_64. No minimum supported toolchain is claimed. Dependencies are not vendored; a first build requires Cargo registry access/cache. External converters are runtime prerequisites, not Rust build prerequisites.

```toml
[dependencies]
omniattach-backends = { path = "../omniattach-backends" }
```

The built-in `ProcessRunner` currently supports Unix only. On other platforms it returns `Error::Unsupported` rather than pretending to enforce process-tree cleanup. The `Runner` trait can be implemented for a native job-object/container runner.

## Rust usage

```rust,no_run
use omniattach_backends::*;
use std::path::Path;

fn main() -> Result<()> {
    let runner = ProcessRunner;
    let context = BackendContext::new(&runner);
    let bundle = Poppler::default().convert(Path::new("paper.pdf"), &context)?;
    println!("{}", bundle.manifest.to_json()?);
    Ok(())
}
```

For explicit registry dispatch:

```rust,no_run
use omniattach_backends::*;

fn main() -> Result<()> {
    let mut registry = Registry::new();
    registry.register(Poppler::default())?;
    registry.register(FfmpegFrames::default())?;
    let context = BackendContext::new(&ProcessRunner);
    let bundle = registry.convert("ffmpeg-frames", "video.mp4", &context)?;
    bundle.export("new-export-directory")?;
    Ok(())
}
```

`Registry::capabilities()` describes **registered contracts**, not installed tool health. `Tool::probe(args, context)` is an explicit bounded process call. A successful version probe does not establish that codecs, language packs, Office document filters, models or GPU support are usable. Configure absolute executable paths in deployed services; relative executable paths are resolved relative to the job directory, so use a PATH name or an absolute path.

## Artifact ownership

`ArtifactBundle` owns a private temporary workspace. Its relative asset paths remain valid until the bundle is dropped. Use `bundle.asset_path(index)` while it lives, or call `export(new_directory)` to retain output. Export copies only declared artifacts and writes `manifest.json`; it does not export staged source files or private profiles. Existing destination directories are refused. Failed exports attempt cleanup; export is not atomic.

The public `JobWorkspace` API supports third-party `Backend` implementations without needing private constructors. It offers source staging/hashing, checked reads, asset construction and final manifest validation. `Runner` is injectable for command-contract tests, worker services or alternative execution policies.

All built-ins stage and hash the actual copied input before launching tools. Renaming to `input.EXT` avoids forwarding user-controlled basenames as command flags. This also means **external relative links/resources are not copied**; self-contained documents/media are the supported input model. Inputs are local regular files; source symlinks may resolve to regular files. No input URI/network stream API is provided.

## Media examples

```rust,no_run
use omniattach_backends::*;
use std::path::Path;

fn main() -> Result<()> {
    let runner = ProcessRunner;
    let context = BackendContext::new(&runner);
    let frames = FfmpegFrames {
        selection: FrameSelection::SceneCuts { threshold: 0.35 },
        max_frames: 250,
        width: 1280,
        ..Default::default()
    }.convert(Path::new("video.mp4"), &context)?;
    frames.export("video-frames")?;

    let mut whisper = WhisperCpp::new("/models/ggml-base.bin");
    whisper.language = "hu".into();
    let speech = whisper.convert(Path::new("speech.flac"), &context)?;
    for segment in speech.manifest.text {
        println!("{:?}: {}", segment.location.start_ms, segment.text);
    }
    Ok(())
}
```

Scene mode includes the first decoded frame plus cuts above FFmpeg's native scene-score threshold. It is a heuristic: fades, flashes, motion and similar-looking cuts can produce misses/extra selections. It does not guarantee one representative image of every semantic scene. Frame timestamps are selected decoded PTS, normalized with `setpts=PTS-STARTPTS`; there are no fabricated scene-end timestamps. Interval mode also uses decoded PTS, not an assumed constant frame rate. Frames are PNG, scaled down to a configured width without intentional upscaling. Reaching the cap produces a diagnostic.

Audio is normalized from the first audio stream. Speech milliseconds refer to normalized audio start; video milliseconds refer to the first decoded video frame. **These are not guaranteed to share a container-wide clock**, particularly for nonzero stream offsets/edit lists. Cross-modal alignment is a downstream responsibility.

## Custom wrapper

```rust,no_run
use omniattach_backends::*;
use std::path::Path;

fn main() -> Result<()> {
    let backend = CustomBackend {
        name: "my-pdf-markdown".into(),
        tool: Tool::new("/opt/bin/pdf-to-markdown-wrapper"),
        args: vec!["{input}".into(), "{output_dir}".into()],
        output: CustomOutput::MarkdownFile { path: "out/document.md".into() },
    };
    let bundle = backend.convert(Path::new("paper.pdf"), &BackendContext::new(&ProcessRunner))?;
    println!("{}", bundle.manifest.to_json()?);
    Ok(())
}
```

Arguments are literal `argv` entries, never shell commands. Only an **entire argument** equal to `{input}`, `{output_dir}` or `{work_dir}` is substituted. `--output={output_dir}` is not interpolated. Output paths are fixed relative paths inside the workspace, not recursive searches for whichever Markdown file happens to exist. A configured program may itself be an interpreter/wrapper; configuration is trusted executable code.

Custom Markdown remains one Markdown text unit with no invented original page/region provenance. Relative embedded images are not automatically relocated; a wrapper should make a self-contained output or manage assets through its own backend implementation. Generic file output is checked for containment/existence/size, not format-specific correctness.

## Runtime control

```rust,no_run
use omniattach_backends::*;
use std::time::Duration;
let runner = ProcessRunner;
let mut context = BackendContext::new(&runner);
context.run_options.timeout = Duration::from_secs(120);
context.run_options.max_stdout_bytes = 8 * 1024 * 1024;
context.limits.input_bytes = 128 * 1024 * 1024;
let cancellation = context.run_options.cancellation.clone();
// A separate worker/controller may call cancellation.cancel().
```

The runner concurrently drains nonblocking stdout/stderr, enforces independent pipe byte caps, polls cancellation, and kills/reaps the process group on every return path, including when the parent exits before descendants release inherited pipes. Timeout applies **per invocation**, not the entire conversion. Source staging, parsing, export and OS spawn itself are not cancellable/time-bounded. There is no silent pipe truncation: exceeding a cap fails the operation.

The environment is cleared except PATH/HOME/TMPDIR/XDG_RUNTIME_DIR/SYSTEMROOT; locale/GIO settings and explicit `Tool::env` entries are then applied. HOME is not redirected into scratch. Model runtime settings (e.g. CUDA-related configuration) must be passed explicitly. Successful stderr tails are retained in invocation metadata and may contain sensitive content.

Post-process file limits reject oversized accepted artifacts but are **not a live disk quota or sandbox**. This crate does not constrain converter network access, CPU, RAM, syscalls, arbitrary file access or descendants that escape their process group. FFmpeg receives a restrictive protocol list, but that is not a general filesystem/security boundary. See `docs/SECURITY.md`.

## Integration with documents

Pass a LibreOffice output to the document parser while the bundle lives:

```rust,no_run
use omniattach_backends::*;
use std::path::Path;

fn main() -> Result<()> {
    let context = BackendContext::new(&ProcessRunner);
    let converted = LibreOffice::new(OfficeFormat::Docx)
        .convert(Path::new("legacy.doc"), &context)?;
    let docx_path = converted.asset_path(0)?;
    // Pass docx_path to your document crate's extraction API here.
    // Keep converted alive until the document parser finishes reading.
    println!("{}", docx_path.display());
    Ok(())
}
```

The crates intentionally have no dependency on each other's unpublished 0.1 release. Backend manifests are **not** `omniattach_documents::Document`; a downstream orchestrator maps text units to its chosen core types. Keep the original backend source/hash and the conversion relationship: the converted document's own parser will correctly hash converted bytes, which is a different source identity.

## Examples and verification

```sh
cargo run --locked --example convert -- poppler paper.pdf
cargo run --locked --example convert -- audio speech.mp3 extracted-audio
cargo run --locked --example convert -- frames video.mp4 extracted-frames
cargo run --locked --example convert -- office-pdf legacy.ppt converted-office
cargo run --locked --example transcribe -- speech.wav /models/ggml-base.bin hu transcript-output
cargo run --locked --example custom_markdown -- /opt/bin/wrapper input.pdf

# Explicit real-tool tests (require installed tools/filters)
cargo test --locked --test native_tools -- --ignored --test-threads=1
```

The last example argument is an optional **new** export directory. Without export, JSON containing asset paths is printed but the files are removed when the example exits; export when assets must survive.

Default tests run parser fixtures, injectable command-contract tests and real Unix process-runner tests. External-tool smoke tests are opt-in and were separately executed against the installed FFmpeg/FFprobe, Poppler and LibreOffice Impress filter. Actual Tesseract OCR and Whisper inference were **not** executed in this environment; those contracts are tested through parser fixtures/injected runners. See `docs/TESTING.md` for precise results and qualifications.

## License

Apache-2.0 for this crate; dependencies and external tools retain their respective licenses. No RagDBMan code is embedded. No upstream repository modification or crates.io publication is performed by building this archive.
