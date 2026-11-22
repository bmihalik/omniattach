# STEP-5 typed conversion API

With feature pdf, `omniattach_documents::pdf_tables` exports PdfTableGrid,
PdfTableConversion and convert_inspection. Grid boundaries are finite ascending
PDF user-space coordinates; page is one-based. The API consumes an existing typed
PdfInspection, does not reserialize inspection JSON, and never downloads or writes.

```rust,ignore
use omniattach_documents::{inspect_pdf, Options, pdf_tables::{PdfTableGrid, convert_inspection}};
let options = Options::default();
let report = inspect_pdf("input.pdf", &options)?;
let grid = PdfTableGrid { page: 1, x_boundaries: vec![50.,150.,250.], y_boundaries: vec![100.,130.,160.] };
let result = convert_inspection(&report, &grid, &options)?;
println!("{}", result.markdown);
```

Coordinates above are illustrative. The serialized result records source name/hash,
page and boundaries, contributing nonblank text sequences, the first-row-as-header
policy, Markdown and warnings. Provenance refers to the supplied report, not an
independent verification of arbitrary user-constructed inspection values. Supplied
grids are explicit selections, not proof that a real table exists. Estimated text
bounds and fixed 1-unit line grouping remain limitations. Unpositioned text anywhere
on the selected page causes an error; undecodable text intersecting the grid also
causes an error. Empty selections fail. Configured cell/element/text/output budgets
apply alongside fixed engine caps. No changes to ordinary extraction are made.

Both examples now share the library reconstruction engine. pdf-table calls the typed
API; pdf-ruled-table retains its existing discovery and selection wrapper for now.
The low-level Run/convert interface remains available for that wrapper, but callers
wanting validation/provenance should use convert_inspection. Discovery, facade and
main CLI integration remain pending. Engine tests now live in the library:

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
python3 scripts/test_lfe_step5.py
cargo test --locked -p omniattach-documents --features pdf --example pdf-ruled-table
cargo check --locked --workspace --all-features --all-targets
cargo fmt --all
```

Library unit tests and this refactor have not been run in the preparation environment.
Earlier example tests were reported passing by the user before this change; that
report does not validate this new API or refactor. No dependencies were added.
