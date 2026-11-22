# STEP-5 reusable discovery and candidate conversion

`omniattach_documents::pdf_tables::discovery` now provides:

- discover_pdf_tables(path, page, options)
- discover_pdf_tables_bytes(name, bytes, page, options)
- convert_pdf_table(path, page, candidate, options)
- convert_pdf_table_bytes(name, bytes, page, candidate, options)

Listing returns PdfTableDiscovery with source name/hash, page, warnings and a typed
candidate array. Candidates retain their previous JSON fields and page-local one-based
numbers. Listing adds top-level schema_version, source_sha256 and warnings. Candidate
numbers are not LFE inspection sequences, durable IDs or cross-document references.
Rediscover after input changes. Conversion always re-discovers a candidate and extracts
text from the same input byte slice; path conversion reads only once. Result is the
existing PdfTableConversion, including contributing text sequences and warnings.

The example is now a thin library wrapper; duplicate discovery logic was removed.
Its invocation and Markdown remain unchanged. Reconstruction tests and discovery tests
now live in the library, so example-only cargo test does not exercise them:

```sh
cargo test --locked -p omniattach-documents --features pdf --lib pdf_tables
python3 scripts/test_lfe_step5.py
cargo check --locked --workspace --all-features --all-targets
cargo fmt --all
```

Supported scope is unchanged: full-span page-content grid rulings in one S/s path,
no XObjects, clipping, non-default Rotate/UserUnit, continuation or scanned tables.
The parser is not yet shared with the full inspection interpreter. Conversion now
uses configured typed-API cell/text/output checks and rejects unpositioned text on
the selected page. No automatic replacement of prose or canonical Document mutation.

Parsing is bounded by existing byte/decompression/page/operation/path budgets and
candidate caps, not an OS resource sandbox. Inspection still traverses the document
when converting the selected page. Main CLI/facade integration is pending. No model
or dependency is added. Prior tests were reported passing by the user; tests for
this discovery refactor have not been executed in the preparation environment.
