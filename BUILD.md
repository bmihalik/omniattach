# Build and Test Guide

This guide covers building omniattach, running its tests, and formatting the sources. Run every command from the repository root unless noted.

## 1. Prerequisites

- Rust 1.85 or newer. The workspace uses edition 2024.
- `rustfmt` and `clippy`:

```bash
rustup update stable
rustup component add rustfmt clippy
```

- Optional external tools for the `backends` feature, all found on `PATH`:
  - `libreoffice` for `.doc`, `.rtf`, `.ppt` and Office conversion
  - `pdftotext` (Poppler) for PDF text
  - `tesseract` for OCR
  - `ffmpeg` and `ffprobe` for audio and video
- Optional: `sqlite3` for inspecting databases.

External tools are needed only at runtime, not to compile. Missing tools cause no build failure. The CLI reports `TOOL_UNAVAILABLE` only when a request needs one.

## 2. Get the Sources

```bash
git clone https://github.com/bmihalik/omniattach.git   # first time
cd omniattach
git pull --ff-only origin main                         # existing checkout
```

`--ff-only` avoids accidental merge commits.

## 3. Workspace Layout

| Crate | Purpose |
|---|---|
| `omniattach-core` | Shared bundle, progress, and backend contracts |
| `omniattach-documents` | Local document extraction (feature `office`, `pdf`) |
| `omniattach-backends` | Subprocess conversion backends (LibreOffice, Poppler, FFmpeg, Tesseract, Whisper) |
| `omniattach` | Conversion facade and feature gates |
| `omniattach-cli` | The `omniattach` binary |

`crates/omniattach/verification` and `crates/omniattach-cli/verification` are standalone harnesses. They are excluded from the workspace.

## 4. Feature Flags

| Crate | Feature | Enables |
|---|---|---|
| `omniattach-cli` | `facade` (default) | Conversion commands |
| `omniattach-cli` | `documents` (default) | Document routes |
| `omniattach-cli` | `backends` | External backend routes |
| `omniattach-cli` | `workspace` | Documents, backends, and core re-exports |
| `omniattach-documents` | `office` | DOCX, PPTX, XLSX, ODF, EPUB |
| `omniattach-documents` | `pdf` | PDF text extraction |

The `workspace` feature is the usual choice for a full local build.

## 5. Build

```bash
cargo build                                   # debug, default members
cargo build --release                         # optimized
cargo build -p omniattach-cli --features workspace
```

Binaries go to `target/debug/` or `target/release/`. The CLI binary is `omniattach`.

## 6. Test

Run the full workspace suite:

```bash
cargo test --workspace --all-features
cargo test -p omniattach-cli --features workspace
```

Test the document crate with its optional families:

```bash
cargo test -p omniattach-documents --features office
cargo test -p omniattach-documents --all-features
```

Run the isolated facade harness. It needs no sibling crates:

```bash
cd crates/omniattach
./verification/check.sh
```

Run the CLI harness the same way:

```bash
cd crates/omniattach-cli
./verification/check.sh
```

### Opt-in OCR test

This test needs `pdftoppm`, `tesseract`, and ImageMagick (`magick` or `convert`):

```bash
scripts/test_pdf_ocr.sh
```

For a specific scanned PDF:

```bash
OMNIATTACH_SCANNED_PDF=/path/to/scan.pdf OMNIATTACH_EXPECTED_OCR_TEXT=word \
  cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored
```

### Opt-in network smoke test

`scripts/smoke-net.sh` downloads sample files and runs the CLI against them. It needs network access and the external tools above.

## 7. Lint

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## 8. Format

```bash
cargo fmt --all                # rewrite files
cargo fmt --all -- --check     # verify only; non-zero exit on differences
```

Rustfmt reads `rustfmt.toml` or `.rustfmt.toml` from the repository, or from a parent directory. Without one it uses defaults.

## 9. Full Check Before Pushing

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Review changes with `git diff` before committing.

## 10. Cleanup

```bash
cargo clean
```
