# omniattach

Explicit, local file conversion for Rust, with a command-line interface that emits machine-readable JSON.

omniattach turns documents and media into text, Markdown, tables, and durable export bundles. Every conversion uses an explicitly chosen route. Nothing falls back silently to another converter, and external tools run only when you permit them.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/omniattach-core` | Shared bundle, progress, and backend contracts |
| `crates/omniattach-documents` | Local document extraction into structured data (DOCX, PPTX, XLSX, ODF, EPUB, PDF, CSV, XML, and more) |
| `crates/omniattach-backends` | Subprocess backends for installed tools such as LibreOffice, Poppler, Tesseract, and FFmpeg |
| `crates/omniattach` | Conversion facade that plans and runs routes |
| `crates/omniattach-cli` | The `omniattach` command-line binary |

## Quick start

```sh
cargo build -p omniattach-cli --features workspace
./target/debug/omniattach capabilities --pretty
./target/debug/omniattach plan notes.txt --goal markdown
./target/debug/omniattach convert notes.txt --goal markdown --output notes-export
```

`convert` writes a new directory with `manifest.json` and prints a JSON result. The output directory must not already exist.

## Legacy Office files

`.doc`, `.rtf`, and `.ppt` files are converted with LibreOffice. Two flags are required:

```sh
./target/debug/omniattach --external-backends convert report.doc --allow-external --output report-export
```

`--external-backends` registers the external routes, and `--allow-external` allows them to run. If LibreOffice is missing, the CLI returns `TOOL_UNAVAILABLE` (exit code 8) before it stages any files. Other formats are not affected.

## Documentation

- `BUILD.md` covers prerequisites, feature flags, build, test, lint, and formatting.
- `crates/omniattach-cli/README.md` documents the CLI commands, output contract, and exit codes.

## License

Apache-2.0. See each crate's manifest for its license metadata.
