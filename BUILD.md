# Build and test guide

Run commands from the repository root. Release manifests for documents, facade and CLI target 0.3.0; see CHANGELOG.md and PUBLISHING.md.

## Toolchain

Use current stable Rust with edition-2024 support. This guide does not certify a workspace-wide minimum Rust version: source features and dependency versions may require a newer compiler than the edition baseline.

```sh
rustup update stable
rustup component add rustfmt clippy
rustc --version
cargo --version
```

Dependencies are not vendored; initial builds need registry access or an existing cache. External converters are runtime prerequisites, not Rust build prerequisites.

## Checkout and build

```sh
git clone https://github.com/bmihalik/omniattach.git
cd omniattach
# Existing checkout:
git pull --ff-only origin main
cargo build -p omniattach-cli
cargo build -p omniattach-cli --release --features workspace
```

Default CLI features enable document, PDF, EPUB, MOBI and experimental AZW3 routes. workspace also compiles external backend adapters and core integration. Binaries are under target/debug or target/release.

## Feature matrix

| Package | Feature | Meaning |
|---|---|---|
| CLI | facade | Conversion facade, default |
| CLI | documents | Document routes including ebooks, default |
| CLI | backends | Optional subprocess adapters |
| CLI | workspace | Documents/backends/core |
| Documents | office | Office, ODF, EPUB and workbooks |
| Documents | pdf | PDF extraction |
| Documents | epub | Alias for office |
| Documents | mobi | MOBI reader |
| Documents | azw3 | Experimental AZW3, implies mobi |
| Documents | ebooks | EPUB/MOBI/AZW3 |

The facade's documents feature enables the document library's office, pdf, mobi and azw3 features. Feature compilation and runtime route registration are separate; library callers register defaults explicitly.

## Local checks and lockfile

After version/dependency changes, let Cargo refresh the root Cargo.lock and review its diff. Do not edit checksums or resolved dependency entries by hand. Once current, repeat verification with --locked where applicable.

```sh
cargo check --workspace --all-features --all-targets
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Targeted checks:

```sh
cargo test -p omniattach-documents --features pdf --test pdf_spacing
cargo test -p omniattach-documents --features azw3
cargo test -p omniattach-documents --features ebooks
cargo test -p omniattach-cli --test ebooks
cargo test -p omniattach-cli --features workspace
cargo test -p omniattach
```

## Real-file smoke tests

```sh
./target/debug/omniattach probe book.azw3 --pretty
./target/debug/omniattach plan book.azw3 --goal markdown
./target/debug/omniattach convert book.azw3 --goal markdown --output new-book-export
```

Inspect document.json diagnostics and document.md. Ebooks run in process; --allow-external is unnecessary. A plan is not proof that conversion will succeed. AZW3 completeness/reading order is not universally certified.

For the supplied local PDF spacing regression:

```sh
OMNIATTACH_SPACING_PDF=/path/to/regression.pdf \
  cargo test -p omniattach-documents --features pdf --test pdf_spacing \
  supplied_timeline_pdf_recovers_the_reported_phrase -- --ignored
```

## External-tool tests

Install only tools required by selected routes: LibreOffice, Poppler (pdftotext/pdftoppm), Tesseract and language data, FFmpeg/FFprobe, or an explicitly configured whisper.cpp executable/model. No tools/models are downloaded by registration. The built-in process runner's platform limitations are documented in the backend crate.

scripts/test_pdf_ocr.sh additionally uses ImageMagick and a working PDF-creation route such as img2pdf/tiff2pdf where installed. To use an existing scan:

```sh
OMNIATTACH_SCANNED_PDF=/path/to/scan.pdf OMNIATTACH_EXPECTED_OCR_TEXT=word \
  cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored
```

scripts/smoke-net.sh downloads public samples and requires network and its selected converters. Review scripts before running. Native converter tests may require --ignored and installed filters/data; unit-test success does not establish real-tool quality.

## Excluded verification harnesses

crates/omniattach/verification and crates/omniattach-cli/verification are independent portable/inspection harnesses, not release workspace packages. Run each script in a separate subshell:

```sh
(cd crates/omniattach && ./verification/check.sh)
(cd crates/omniattach-cli && ./verification/check.sh)
```

They have separate manifests/lockfiles. Do not enable integration features on the isolated harnesses. Their package versions need not match release packages.

## Release checks and cleanup

See PUBLISHING.md for package inspection, dry runs and dependency order. This document does not assert that commands have passed for the current commit.

```sh
cargo clean
```
