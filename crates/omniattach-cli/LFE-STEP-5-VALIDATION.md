# STEP-5 main binary end-to-end validation

```sh
python3 scripts/test_lfe_step5_cli.py
```

This separate standard-library Python runner imports the fixture generator/constants
from scripts/test_lfe_step5.py without running its example tests. It builds the main
omniattach binary with default CLI features, obtains its executable path from Cargo
compiler-artifact JSON, then executes the binary as a subprocess against temporary
synthetic PDF files (including a filename with spaces). Cargo may need network to
obtain uncached build dependencies. No model or external PDF generator is required.
A native runnable build is required; cross-compiled executables may not run locally.

Eleven tests cover candidate listing/provenance, JSON versus exact Markdown output,
pretty printing, output caps (including exact Markdown byte boundaries), input caps,
invalid/missing selection, empty/prose-only grids, crossing text and clipping,
transforms/multiple candidates, and absence of input/export/cache modifications.
Errors must have the documented exit code, empty stdout, and one valid structured
JSON stderr document. Success must have empty stderr. Build and invocation timeouts
are test-runner bounds, not guarantees supplied by the conversion API.

Invocations use temporary HF_HOME/HF_HUB_CACHE/HUGGINGFACE_HUB_CACHE paths and offline
environment flags; the side-effect test checks these remain empty. This is not a
network interception test or OS filesystem/network sandbox and does not prove that
arbitrary model code cannot access other locations. The table code path needs no
model. Temporary fixtures are removed after each test; no binary PDFs are committed.

Keep the library, parser/feature-gating and example suites in the validation set:

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
cargo test --locked -p omniattach-cli --test pdf_table_commands
cargo test --locked -p omniattach-cli --no-default-features --test pdf_table_commands
python3 scripts/test_lfe_step5.py
python3 scripts/test_lfe_step5_cli.py
cargo check --locked --workspace --all-features --all-targets
```

This commit adds validation only. Tests have not been run in the preparation
environment. Prior passing results were reported by the user before these new tests.
Real-PDF validation and wider table support remain separate work; passing synthetic
fixtures alone does not establish general table accuracy or multi-page continuation.
