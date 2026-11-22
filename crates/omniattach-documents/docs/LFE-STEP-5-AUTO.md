# STEP-5 automatic grid example

This extends the explicit-grid example without changing library extraction or model provisioning.
Grid discovery no longer requires users to enter coordinates:

```sh
cargo run -p omniattach-documents --features pdf --example pdf-ruled-table -- input.pdf 1 list
cargo run -p omniattach-documents --features pdf --example pdf-ruled-table -- input.pdf 1 1
cargo test -p omniattach-documents --features pdf --example pdf-ruled-table
cargo fmt --all
cargo check --workspace --all-features --all-targets
```

Candidate numbers are one-based and local to the selected page. They are assigned
in content stroke order, and are not LFE inspection element sequences. Listing emits
JSON with page, source name and discovered grid boundaries. Conversion emits Markdown
on stdout and page/candidate/source-hash notes on stderr. First row becomes the header.

Supported grids have at least three full-span rulings on each axis, including outer
borders, drawn in one S/s stroke in page content. The parser applies cm and q/Q
transforms. Geometry matching uses a strict 0.01-unit tolerance. Every segment in the
candidate path must be a full horizontal or vertical ruling. Curves disqualify a path.
Separately stroked lines and filled paths are not combined. Existing text bounds are
estimates; boundary-crossing text fails instead of being guessed into a cell.

The selected page is rejected if it invokes any XObject (Do), including images, or
uses clipping. Page ancestry with non-default Rotate/UserUnit is rejected. This is
intentionally restrictive until discovery shares the full interpreter's Form traversal.
Discovery and text inspection use the same input bytes. Parsing/decompression and
operation/path/candidate budgets apply, but this is not a process resource sandbox.

Engine tests and three discovery tests are included. They have not been executed
in the preparation environment; no real-PDF accuracy claim is made. Synthetic PDF
end-to-end fixtures, shared typed library API, main CLI/facade integration, Form
support and improved diagnostics remain pending. Continuations and merged, borderless,
scanned or rotated tables remain out of scope. The original explicit-grid example
is retained for manual control.
