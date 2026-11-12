# Line-aware native PDF extraction

src/pdf_spacing.rs now contains the line-aware reader (imported as pdf::lines), replacing the old TJ-rewriting shim in place. pdf.rs retains outlines, images and OCR. Original bytes and hashes are unchanged; package/schema versions are not bumped.

The reader follows graphics/text matrices, text state, glyph widths and integer/real TJ adjustments. Form XObjects apply Matrix/resources with cycle detection, depth 32 and graphics-stack depth 128. Simple fonts use Widths/FirstChar; CID fonts use compact W/DW ranges, assuming two-byte codes. Missing metrics fall back to 500/1000 em; zero widths remain valid. General custom variable-length CMaps, vertical writing, rotated layouts and column sorting are not supported.

Lines use baseline changes above 0.5 x height; spaces use gaps above 0.18 x height or TJ below -180. Paragraph gaps use 1.45 x the modal page gap (half-point bins) or 2.6 x height. Repeated edge furniture is removed in two snapshot-based passes at max(pages/4,3) occurrences; edge arabic/lowercase roman numbers are removed separately. This is heuristic and can misclassify repeated body text. PDF_PAGE_FURNITURE_REMOVED reports the count.

Paragraphs preserve source newlines and line-end hyphens, including across pages. source.page records the first page. Missing-text pages and native/OCR transitions form conservative boundaries. No de-hyphenation occurs; Markdown source newlines are not forced visual hard breaks in all renderers. Outline headings remain before body text.

Empty pages use the existing optional PdfOcr callback and diagnostics. PDF_UNDECODED_TEXT counts strings that cannot be fully decoded. For existing clients, PDF_TJ_SPACING_NORMALIZED remains when real-valued word gaps are interpreted, with updated wording: there is no stream rewriting. PDF_TEXT_ONLY describes current scope.

Limits reuse archive_bytes for per-page content plus repeated Form expansion and total decoded text, archive_entries for per-page Form calls, xml_nodes for per-page content/text-item operations, and blocks for pages/paragraphs. Parser allocations are not an OS sandbox.

Run locally:

```sh
cargo fmt --all
cargo test -p omniattach-documents --features pdf --test pdf
cargo test -p omniattach-documents --features pdf --test pdf_spacing
cargo test --workspace --all-features
cargo check --workspace --all-features --all-targets
cargo clippy --workspace --all-features --all-targets -- -D warnings
OMNIATTACH_PDF_LINES_DIR=/path/to/handoff cargo test -p omniattach-documents --features pdf --test pdf handoff_pdfs_match_poppler_words_and_source_lines -- --ignored
```

The ignored test requires Poppler and the authorized local hpl.pdf, hpl_tm.pdf, hpl_td.pdf and lo.pdf; it compares words and nonempty lines after reference furniture filtering. These PDFs are not redistributed. The preparation environment had no Rust compiler/Cargo/rustfmt; no compile or acceptance-test pass is claimed. Existing spacing regressions remain applicable.
