# Verification

Verified in the Linux x86_64 sandbox on 24 October 2026 with rustc 1.99.1 and Cargo 1.99.1.

| Configuration | Integration tests | Documentation tests | Result |
|---|---:|---:|---|
| Empty default feature set | 24 | 1 | Passed |
| `office` | 33 | 1 | Passed |
| `pdf` | 25 | 1 | Passed |
| All features | 34 | 1 | Passed |

Two tests in text-only builds explicitly verify disabled feature errors; counts differ accordingly. No tests are ignored. Source formatting and warning-denying Clippy checks pass for the default, PDF-only and all-feature configurations. The CI definition repeats the whole feature matrix with formatting/lint checks on future runs.

Package tests create minimal XML ZIP packages in memory and execute the actual Rust adapters. PDF tests construct a minimal two-page PDF with lopdf and execute real text extraction. They are integration/synthetic-fixture tests, **not** claims of real-world corpus accuracy. XML measurement examples are invented sample data for exercising the API, not laboratory measurements.

Coverage includes Hungarian Unicode and CRLF byte ranges; strict decoding/BOM/fallback behavior; Markdown structure/tables; CSV quoting/multiline/ragged rows; XML namespaces/attributes/mixed text/missing-vs-empty/repetition/invalid selectors; script omission; canonical JSON roundtrip; side-effect-free extraction; DOCX table provenance; presentation relationship order; ODT/ODP text; ODS repetition budgets and formulas; XLSX cached formulas and actual addresses; EPUB spine order; PDF page spans and empty-page diagnostics; ZIP traversal and expansion checks; padded-cell limits.

The included XML Rust example and conversion example were executed. The source package can be inspected with `cargo package --list`; the archive intentionally excludes `target`, dependency caches and compiled binaries.

Not established: exhaustive Office/PDF compliance, OCR/media quality, complete lexical fidelity through Calamine, older-toolchain support, macOS/Windows builds, dependency vulnerability status, fuzzing, concurrency scalability or peak memory/throughput. No crate was published and no upstream repository was modified.
