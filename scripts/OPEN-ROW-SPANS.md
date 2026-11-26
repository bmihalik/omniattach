# Open-sided row-span table extraction (bounded fallback)

Targets the four-column, filled-rule table on PDF pages 2 and 3 of the locally
supplied 02-healthbig.pdf. Discovery first preserves the existing closed-grid
behavior. Only if that yields no candidates does this fallback run. It requires
both outside vertical sides to be absent, at least three matching full-width
horizontal rules, at least two interior vertical dividers, and complete divider
coverage over the candidate height. Endpoints of all relevant separators must
align with the inferred columns. Partial separators and overlapping candidates
are rejected. Coverage can bridge at most 1.6 PDF units, bounded by the existing
thin-rule allowance. Geometry work, dimensions, text and candidate counts are capped.

Missing horizontal separators create vertical row spans. This is not column-span,
general merged-cell, borderless, rotated, mixed-stroke or cross-page support.
Clipping and XObject restrictions remain unchanged, including errors on this
sample's non-table pages 1 and 4. No inference is made from the filename or text.

Discovery with spans uses omniattach/pdf-table-discovery/0.2 and exposes
candidate.cell_spans: zero-based top-down row/column anchors and row_span/
column_span extents. The current fallback always emits column_span=1 and a complete
non-overlapping partition. Older discovery output keeps schema 0.1; empty spans are
omitted and default when deserializing old JSON. Rust code constructing the public
PdfTableCandidate struct must now provide cell_spans (normally Vec::new()).

The existing conversion schema and PdfTableGrid API stay unchanged. Span-aware
conversion is through pdf-table --candidate, which rediscovers from the same bytes.
Markdown writes each cell at its anchor and leaves covered slots empty, with no
category duplication. Topology remains in discovery, not the conversion object's
grid: retain both outputs. The corpus report already retains both. Raw supplied-grid
conversion does not gain row-span support. Original text geometry stays unchanged;
column containment remains strict. Text can cross an undrawn row boundary only
within a single spanning cell. Existing estimated-height allowances are applied
against that real cell's outer bounds, not each artificial grid slot.

HealthPROMISE stays split: its name is retained at the end of page 2 with empty
country/description cells; the first body row on page 3 retains the country and
description with an empty name. Blank categories are not propagated across pages.
Repeated page headers stay page-local. No dehyphenation or header inference is added.
The MinerU adapter remains unchanged, so merged reference cells still make its
comparison inconclusive. The two non-table clipping errors can therefore remain in
the folder summary even when the new table pages convert correctly.

Preparation checks: a Python mirror of the geometry algorithm found one candidate
per table page: 21 rows, 4 columns and 67 cells on page 2; 14 rows, 4 columns and 45
cells on page 3. Row counts include the header and page-local continuation fragments.
Category spans are [1,5,5,10] and [1,7,6]. Each slot has exactly one owner. Three
synthetic geometry checks passed. These are not Rust compilation or text-conversion
results: cargo/rustc are unavailable in the preparation environment.

Run locally after rebuilding:

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
cargo build --locked -p omniattach-cli --bin omniattach
python3 scripts/test_lfe_open_rows.py --binary ./target/debug/omniattach --sample /path/to/02-healthbig.pdf
python3 scripts/test_lfe_filled_tables.py --binary ./target/debug/omniattach --sample /path/to/01-evibaseprac.pdf
cargo check --locked --workspace --all-features --all-targets
```

The new CLI tests cover synthetic row spans, partial dividers, clipping, actual page
shapes, 32 unique nonempty app-name rows, the split HealthPROMISE fragments and source
sequence uniqueness. The user PDF is not committed. Six new Rust tests cover geometry,
partition validation, text crossing an absent row line and strict column containment;
one additional discovery test checks backward JSON defaults. Actual sample text
conversion and all Rust checks remain to be run locally.
