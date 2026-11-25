# Cell reading order and corpus status follow-up

Cell rendering now clusters text into lines before sorting each line left-to-right.
Fixed anchors are established in descending text-box height order. Normal fragments
must match anchor top and bottom within 1 PDF unit. Smaller fragments (height at
most 85% of anchor height) can join a nearby line if at least half their height
overlaps it vertically. Attachment must be unique; ambiguous fragments remain on
separate lines. Cached horizontal extents prevent rescanning full lines per glyph.
Limits: 16,384 runs and 1,024 lines per cell. Existing cell boundary checks, HTML
escaping, explicit word gaps, row assignment and provenance remain in force.

This is a bounded heuristic for native horizontal text, not general mathematical
layout, bidirectional text, or semantic superscript/subscript recognition. Scripts
are emitted as ordinary inline characters (CO2, not sub/sup HTML). It does not
repair hyphenation, infer headers, group section bands, or change the first-row
header convention. New line grouping can change Markdown within a cell and should
be checked against representative PDFs. Genuine separated lines retain <br>.

The sample regression now asserts all 15 normalized descriptions, exact code order
and multiplicity, and no duplicated source sequences across candidates. In
particular code 94400 must read "Breathing response to CO2 (CO2 response curve)".
Whitespace is normalized, but numbers, punctuation and existing hyphenation remain.
The sample is supplied locally and is not committed.

Corpus JSON adds extraction_status and comparison_status per page, categorized
extraction_errors/reference_errors, extraction_summary/comparison_summary (page
counts), and document_error_count. CSV appends the two status columns. Existing
status/summary fields and exit codes are preserved for compatibility. Extraction
success means discovery and all candidate conversions completed, not verified
accuracy; zero candidates can still be an extraction success. Comparison may be
not_requested, blocked by native extraction errors, error, inconclusive, agree,
different or no_tables. Raw partial comparisons are retained, but blocked status
must take precedence. Document failures are counted separately from page summaries.

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
cargo build --locked -p omniattach-cli --bin omniattach
python3 scripts/test_lfe_filled_tables.py --binary ./target/debug/omniattach --sample /path/to/01-evibaseprac.pdf
python3 -m unittest discover -s scripts -p test_table_corpus_unit.py -v
python3 scripts/test_lfe_step5.py
python3 scripts/test_lfe_step5_cli.py
cargo check --locked --workspace --all-features --all-targets
```

Preparation: four Python reporting tests passed, plus four mocked folder integration
cases (no reference, matching reference, native failure, reference failure). A Python
mirror replayed all retained sample runs: only description 94400 changed, fixing CO2;
all other normalized descriptions stayed unchanged, with 1,732 unique source runs.
Rust compilation, the four new Rust clustering tests and actual CLI sample execution
have not been run in the preparation environment. These remain local validation.
