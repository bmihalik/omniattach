# Testing

```sh
cargo test -p omniattach-documents                  # text formats only
cargo test -p omniattach-documents --features office
cargo test -p omniattach-documents --features pdf
cargo test -p omniattach-documents --all-features
```

Tests build their fixtures in code (minimal ZIP packages, PDFs written with lopdf
or raw objects); no binary documents are committed. Builds without a feature
check that the disabled format returns an explicit error.

Opt-in `#[ignore]` suites read local data from environment variables:

| Test | Variable | Data |
|---|---|---|
| `pdf::handoff_pdfs_match_poppler_words_and_source_lines` | `OMNIATTACH_PDF_LINES_DIR` | Local PDFs compared with `pdftotext` |
| `lfe_equation_corpus` | `OMNIATTACH_EQUATION_CORPUS` | Corpus from `scripts/equations/fixtures.py` |
| `pdf_tables::samples` | `OMNIATTACH_FILLED_TABLE_SAMPLE`, `OMNIATTACH_OPEN_ROWS_SAMPLE` | Private real table PDFs |

Run them with `-- --ignored`. See [PDF-ELEMENTS.md](PDF-ELEMENTS.md) for the PDF
element tools. Synthetic fixtures are regression checks, not a measure of
real-world accuracy.
