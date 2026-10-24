# Verification

Verified on Linux x86_64 using Rust/Cargo 1.99.1.

| Check | Observed result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked` | 38 integration tests passed; 1 compiled documentation example passed; 5 native-tool tests intentionally ignored by default |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps` | Passed |
| `cargo test --locked --test native_tools -- --ignored --test-threads=1` | 5 native-tool smoke tests passed |
| Example: `/usr/bin/cat` as literal custom Markdown program | Passed; parsed JSON text checked |
| Example: FFmpeg audio extraction + export | Passed; JSON media type and WAV output checked |

Native smoke coverage:

1. FFmpeg normalizes a generated stereo 44.1 kHz tone; FFprobe confirms mono 16 kHz PCM s16le.
2. Scene-cut extraction of generated black/white video returns frame times 0 and 1000 ms.
3. Interval extraction returns 0, 500, 1000 ms under a three-frame cap and reports the cap diagnostic.
4. Poppler extracts text from a generated two-page PDF and preserves a blank second page.
5. LibreOffice Impress converts a generated flat OpenDocument presentation to PDF in a private profile; Poppler confirms the text survives.

Generated media/PDF documents here are deterministic **test fixtures**, not measurements or corpus benchmarks. They establish command/data plumbing, not general conversion accuracy. The initially attempted RTF native test could not load because this environment has Impress/Draw but no Writer filter. It was replaced by the valid FODP/Impress native smoke test rather than claiming Writer conversion was verified.

Contract tests use injected runners for LibreOffice expected-output failure/success, whisper.cpp model/argv/JSON handling, Tesseract language/TSV handling and custom Markdown/transcript/file results. These are not actual OCR or ASR inference runs. No Whisper model was downloaded and no speech recognition accuracy is claimed. Actual Tesseract word recognition was not executed. No native Marker/MinerU/faster-whisper or Windows/macOS run was performed.

Process tests execute real local Unix programs to check argument literalness, dual-stream capture, failures/missing tools, pipe-byte caps, cancellation before/during execution, timeout with inherited pipes, descendant cleanup, strict UTF-8 and environment overrides. Shell is used **only as an explicitly selected test program**, not by production adapter command construction.

Artifact tests verify private lifetime/cleanup, source staging, export without source leakage, input/individual/total output limits, relative path enforcement, symlink rejection, final size validation, registry duplicate handling and explicit selection. This is regression coverage, not proof of hostile input isolation.

The tar archive excludes `target/`, upstream binaries, model weights and downloaded registry sources. Cargo.lock is supplied. A clean extraction was independently verified using the same lockfile and local toolchain/dependency cache; this is not an air-gapped or vendored build claim.
