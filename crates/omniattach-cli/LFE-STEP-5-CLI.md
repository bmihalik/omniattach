# STEP-5 main CLI table commands

The default CLI documents feature now exposes opt-in native PDF table commands:

```sh
cargo run -p omniattach-cli -- pdf-tables input.pdf --page 1 --pretty
cargo run -p omniattach-cli -- pdf-table input.pdf --page 1 --candidate 1 --pretty
cargo run -p omniattach-cli -- pdf-table input.pdf --page 1 --candidate 1 --output-format markdown
```

pdf-tables returns the library discovery JSON. pdf-table defaults to the typed JSON
conversion result, including source name/hash, grid, text sequences, Markdown and
warnings. Explicit --output-format markdown emits only Markdown. --pretty affects
JSON only. These commands write stdout, not a durable export directory. The normal
convert command and its durable export contract are unchanged. No automatic table
replacement in document prose, continuation joining or new conversion route is added.

Both commands use omniattach::documents::pdf_tables::discovery through the existing
facade documents re-export. They bypass conversion engine construction, route/model
registration and external execution. Builds without documents return UNSUPPORTED.
The capabilities JSON reports pdf_tables_enabled. Candidate numbers are page-local,
one-based and must be rediscovered after source changes, not inspection element IDs.

The existing narrow grid scope remains: one S/s stroked full-span rectangular grid
with native text, no XObjects/clipping/non-default Rotate/UserUnit. The first row is
always the header; estimated text bounds can reject real tables. No model is needed.

--max-input-bytes can lower the default document input cap. --max-output-bytes
(default 16 MiB) checks the buffered output before writing; this is not a memory
sandbox and serialization may allocate more before rejection. Cancellation is checked
before and after synchronous parsing/conversion, not continuously within the parser.
No hard-timeout guarantee is made. File-system write errors may still partially write
stdout. Parse/unsupported-layout errors use INVALID_INPUT; budget errors use
LIMIT_EXCEEDED. Feature absence uses UNSUPPORTED. Errors retain the normal JSON stderr
format. Main CLI full-PDF subprocess end-to-end validation remains a next test task.

```sh
cargo fmt --all
cargo test --locked -p omniattach-cli --test pdf_table_commands
cargo test --locked -p omniattach-cli --no-default-features --test pdf_table_commands
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
python3 scripts/test_lfe_step5.py
cargo check --locked --workspace --all-features --all-targets
```

This integration and its parser/dispatch tests have not been run in preparation.
Previously passing library/example tests were reported by the user before this change.
