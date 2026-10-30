# omniattach

Local document and media conversion for Rust and LLM ingestion, with explicit routes, provenance and durable exports. The command-line package is `omniattach-cli`; its executable is `omniattach`.

The 0.3.0 release line adds main-CLI MOBI/AZW3 extraction, bounded HUFF/CDIC decoding, KF8 flow selection and a targeted PDF word-spacing fix. This is text/semantic extraction, not an ebook renderer or a guarantee of visual fidelity.

## Quick start

```sh
cargo build -p omniattach-cli
./target/debug/omniattach capabilities --pretty
./target/debug/omniattach plan notes.txt --goal markdown
./target/debug/omniattach convert notes.txt --goal markdown --output notes-export
./target/debug/omniattach convert book.azw3 --goal markdown --output book-export
```

Default CLI builds include document, PDF, EPUB, MOBI and experimental AZW3 extraction. The output directory must be new and its parent must exist. Markdown conversion exports `document.md`, `document.json` and `manifest.json`; stdout is a JSON result envelope. `--stdout text` emits extracted plain text while retaining the export.

## Workspace

| Package | Version | Role |
|---|---|---|
| omniattach-core | 0.1.0 | Shared contracts |
| omniattach-documents | 0.3.0 | Local parsing and semantic projections |
| omniattach-backends | 0.1.0 | Installed subprocess-tool adapters |
| omniattach | 0.3.0 | Route planning, staging and export facade |
| omniattach-cli | 0.3.0 | Machine-friendly command line |

Package versions are independent. The root is a virtual workspace, not a separate publishable package. See [CHANGELOG.md](CHANGELOG.md) for migration notes.

## Ebooks and PDF

MOBI/AZW3 run in process without external permission. The reader handles uncompressed, PalmDOC and HUFF/CDIC text, record trailers and validated FDST primary-flow selection. DRM is rejected. AZW3 does not reconstruct KF8 skeletons/fragments, images, styling or navigation; inspect diagnostics. Detection is extension-hinted and planning is not content validation.

PDF extraction retains content-stream order and page provenance. A compatibility shim restores real-valued TJ word gaps using lopdf's existing spacing heuristic. This is not full geometric layout reconstruction. PDF OCR requires an explicitly configured callback or the supported facade OCR path and installed tools.

## External tools

```sh
cargo build -p omniattach-cli --features workspace
./target/debug/omniattach --external-backends convert legacy.doc \
  --allow-external --output legacy-export
```

Registration and execution permission are separate. Legacy DOC/RTF/PPT requests probe LibreOffice on demand; a failed probe returns `TOOL_UNAVAILABLE`/8 before conversion staging. Registration and capabilities do not probe tools. Other backend failures retain their existing error mapping; not every missing executable is classified as `TOOL_UNAVAILABLE`.

## Documentation

- [BUILD.md](BUILD.md): toolchain, features and validation commands.
- [PUBLISHING.md](PUBLISHING.md): versions, lockfile preparation and publication order.
- [CLI README](crates/omniattach-cli/README.md) and [ebook CLI guide](crates/omniattach-cli/docs/EBOOKS.md).
- [Document library](crates/omniattach-documents/README.md) and [AZW3 limitations](crates/omniattach-documents/docs/AZW3.md).

Resource limits are not an OS sandbox. Use isolated, resource-limited workers for hostile inputs. No automatic converter/model downloads or silent converter fallback are included.

Apache-2.0; dependencies, external tools and models retain their own licenses. Manifest/documentation updates do not imply tests have run or packages have been published.
