# omniattach-cli

Machine-friendly command-line interface for omniattach. The binary is named `omniattach`. Version 0.2.0.

## Build

```sh
cargo build -p omniattach-cli --release
cargo build -p omniattach-cli --features workspace
```

The `workspace` feature enables the document and backend routes. See `BUILD.md` in the repository root for the full build and test matrix.

## Commands

| Command | Purpose |
|---|---|
| `capabilities` | Compiled support and registered routes. Does not probe installed tools. |
| `probe INPUT` | Signature and extension detection, without conversion. |
| `xml-probe INPUT` | Namespace-aware XML structure, without text or attribute values. |
| `plan INPUT` | Resolve the route for a request, without running a converter. |
| `convert INPUT --output NEW_DIR` | Convert, export a new directory with `manifest.json`, then print the result. |

## Examples

```sh
omniattach capabilities --pretty
omniattach probe report.pdf
omniattach plan notes.txt --goal markdown
omniattach convert notes.txt --goal markdown --output notes-export
```

## Legacy Office files

`.doc`, `.rtf`, and `.ppt` conversion uses LibreOffice. It needs both flags:

```sh
omniattach --external-backends convert report.doc --allow-external --output report-export
```

`--external-backends` registers the routes, and `--allow-external` permits running them. If LibreOffice cannot run, the CLI returns `TOOL_UNAVAILABLE` with exit code 8 before staging any files. Other formats are unaffected.

## Output contract

Success prints one compact JSON record to stdout (`--pretty` for multi-line). Errors print one JSON record to stderr with `schema`, `error.code`, `error.message`, and `exit_code`. `--progress ndjson` writes progress events to stderr.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Conversion or output failure |
| 2 | Invalid arguments or configuration |
| 3 | File I/O or invalid input |
| 4 | No route, route mismatch, ambiguous route, or unknown format |
| 5 | External execution not permitted |
| 6 | Size or count limit exceeded |
| 8 | Required tool unavailable (`TOOL_UNAVAILABLE`) |
| 124 | Deadline exceeded |
| 130 | Cancelled |

## License

Apache-2.0.
