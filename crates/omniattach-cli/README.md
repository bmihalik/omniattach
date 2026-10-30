# omniattach-cli

Version 0.3.0. Machine-friendly CLI for local document and media conversion. The executable is named `omniattach`.

## Build

```sh
cargo build -p omniattach-cli --release
# Also compile optional external-tool adapters:
cargo build -p omniattach-cli --release --features workspace
```

Default features include the facade and documents, including Office, PDF, EPUB, MOBI and experimental AZW3. External binaries are runtime requirements only for routes that invoke them. See the repository BUILD.md.

## Commands

| Command | Purpose |
|---|---|
| capabilities | Registered routes and compiled support, without health probes |
| probe INPUT | Conservative signature/extension inspection, not conversion validation |
| xml-probe INPUT | Namespace-aware XML structure, without text/attribute values |
| plan INPUT | Advisory route selection; legacy permitted Office requests may probe LibreOffice |
| convert INPUT --output NEW_DIR | Convert, durably export and print the result |

```sh
omniattach capabilities --pretty
omniattach probe book.azw3 --pretty
omniattach plan book.azw3 --goal markdown
omniattach convert book.azw3 --goal markdown --output book-export
omniattach convert book.mobi --goal markdown --output mobi-export
omniattach convert renamed.bin --format azw3 --output explicit-export
```

MOBI/AZW3 use the in-process documents route: no external flags or Calibre required. The library's HUFF/CDIC, trailer and FDST handling and its limitations remain unchanged. Inspect document.json diagnostics and manifest warnings. Successful text recovery is not full KF8 reconstruction. See [docs/EBOOKS.md](docs/EBOOKS.md).

## External conversion

```sh
omniattach --external-backends convert report.doc --allow-external --output report-export
```

`--external-backends` registers routes; `--allow-external` permits execution. Registration does not probe tools. Legacy DOC/RTF/PPT requests probe LibreOffice on demand and report `TOOL_UNAVAILABLE`/8 if it cannot run. Other backend failures use their existing mapping and are not all classified as tool-unavailable.

## Output contract

Default successful stdout is one compact JSON value plus newline; `--pretty` produces multi-line JSON. Conversion stdout is a result envelope, not the canonical document alone. The export contains manifest.json and declared artifacts; a Markdown request includes document.md and document.json. Output directories must be new, and their parent must exist.

`--stdout text` prints extracted plain text while still exporting; `--stdout none` suppresses success output. Errors are JSON on stderr. With `--progress ndjson`, stderr contains progress records and, if needed, a final error record. Help/version use human-readable output.

Package version 0.3.0 does not imply a schema bump: result/error/probe schema identifiers retain their existing versions. Diagnostics can contain sensitive text or paths. Limits and cooperative cancellation are not an OS sandbox.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success/help/version |
| 1 | Conversion/output failure |
| 2 | Invalid arguments/configuration |
| 3 | I/O or invalid input |
| 4 | Unsupported build, route/format selection failure |
| 5 | External execution denied |
| 6 | Directly classified size/count limit |
| 8 | On-demand legacy LibreOffice probe failed |
| 124 | Deadline |
| 130 | Cancellation |

Nested converter errors, including document-library errors, may map to CONVERSION_FAILED/1 rather than a specialized status.

## Development

```sh
cargo test -p omniattach-cli --test ebooks
cargo test -p omniattach-cli --features workspace
cargo clippy -p omniattach-cli --all-features --all-targets -- -D warnings
```

The excluded standalone verification harness tests inspection/transport separately; it is not a release package. See repository PUBLISHING.md for Cargo.lock refresh and dry-run requirements. Apache-2.0.
