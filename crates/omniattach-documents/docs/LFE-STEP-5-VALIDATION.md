# STEP-5 synthetic PDF CLI validation

Run from any directory using the repository script path:

```sh
python3 scripts/test_lfe_step5.py
cargo test --locked -p omniattach-documents --features pdf --example pdf-table
cargo test --locked -p omniattach-documents --features pdf --example pdf-ruled-table
cargo check --locked --workspace --all-features --all-targets
```

The standard-library-only Python runner builds both existing examples once with
Cargo, locates host debug executables via cargo metadata, creates complete temporary
PDFs with fonts, page resources and xref offsets, and invokes the actual binaries.
Cargo dependency resolution/building may require network if dependencies are not
cached; no model, OCR engine, renderer or external PDF generator is required.

Twelve end-to-end cases check candidate JSON, exact Markdown, source-hash provenance,
unchanged input bytes, agreement with the manual-grid example, transformed text/grid
alignment, multiple candidates, graphics-state restoration, empty/prose-only input,
boundary-crossing text, invalid selections, clipping, rotation and UserUnit rejection.
Negative cases require failure and empty stdout to prevent partial successful-looking
Markdown. Timeouts bound builds and invocations but are not a process sandbox.

Run with a native host build configuration; custom build.target/cross-compilation
configuration is not supported by this runner. Temporary PDFs are removed after
each test. No binary fixtures are committed. Unit tests remain separate and should
also run. These fixtures exercise narrow layouts, not real-world table accuracy.

This commit adds validation code only. The suite has not been run in the preparation
environment, and no passing result is claimed. Public typed API/facade and main CLI
integration, shared-interpreter discovery and broader table support remain pending.
