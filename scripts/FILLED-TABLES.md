# Filled-border real-PDF extension

This is an additive, deliberately bounded mode in table discovery. Pages containing
both re and f/F/f* operators route to filled-border discovery; the legacy S/s-only
mode and its clipping restrictions remain unchanged. Mixed filled/stroked pages
are rejected explicitly rather than silently dropping one kind of border.

Recognized borders are single, axis-aligned filled rectangles no thicker than
2 PDF units and at least 3 times longer than thick. Square corner patches are
ignored. Collinear segments are merged across paints with a fixed 0.8-unit axis/gap
tolerance. Horizontal/vertical coverage must close each row; partial internal
verticals are not extended. Consecutive rows with identical column boundaries
become one candidate. A full-width section heading becomes a separate one-column
candidate, not a fabricated merged cell. IDs are page-local, top-to-bottom in this
mode; the original grid mode keeps its paint-order numbering.

Only simple rectangular W/W* clipping paths terminated by n are accepted. Every
candidate must lie within every encountered clip, even clips later restored by Q.
This conservative condition can reject unrelated local clips but avoids assuming
that clipped text was visible. Borders cut by a clip are rejected, not extended.
Complex clips, XObjects, mixed strokes, normalized rotations, general merged cells,
compound filled paths and multi-page continuation remain outside scope. Border
paint visibility (alpha, blend modes, overpainting) is not verified. Detection is
geometric evidence, not a rendering guarantee.

Warnings are copied into conversion JSON as well as discovery output. Markdown
still formats the first row as a header; this is not semantic header inference.
All rows remain present. Section heading and body bands are independent candidates;
there is no parent-table grouping. MinerU may return one merged table instead,
so per-table comparison can be different/inconclusive even when all text survives.

The corpus runner now includes exit status and up to 2000 characters of normalized
stderr in report errors, retaining full logs. It also decodes numeric/HTML entities
in Markdown cells (for example &#124;) before reference comparison.

## Validation

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
cargo test --locked -p omniattach-cli --test pdf_table_commands
cargo test --locked -p omniattach-cli --no-default-features --test pdf_table_commands
cargo build --locked -p omniattach-cli --bin omniattach
python3 scripts/test_lfe_filled_tables.py --binary ./target/debug/omniattach
python3 scripts/test_lfe_filled_tables.py --binary ./target/debug/omniattach --sample /path/to/01-evibaseprac.pdf
python3 -m unittest discover -s scripts -p test_table_corpus_unit.py -v
python3 scripts/test_lfe_step5.py
python3 scripts/test_lfe_step5_cli.py
cargo check --locked --workspace --all-features --all-targets
```

Without --binary the three Python reporting/normalization tests run, and Rust CLI
tests skip. With --binary two synthetic CLI tests run; --sample adds a real-document
check for eight bands and retention of all fifteen treatment code rows. The real
sample is not committed; no permission to redistribute it is assumed.

Preparation checks: three reporting/normalization tests passed. A Python mirror
of the border geometry algorithm processed the supplied PDF and found eight bands:
1x1, 2x6, 1x1, 2x2, 1x1, 2x2, 1x1, 2x5 (columns x rows). This is not Rust execution
or proof of correct positioned-text reconstruction. Cargo/rustc are unavailable
in the preparation environment; Rust compilation, library tests, the existing
suites, and actual sample CLI conversion remain unverified until run locally.

Retry the folder run after rebuilding (use a fresh report directory):

```sh
python3 scripts/test_table_corpus.py /path/to/tables1 --binary ./target/debug/omniattach
```
