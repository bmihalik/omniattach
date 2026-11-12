# Changelog

## Unreleased

- Move line-end de-hyphenation into omniattach-documents as a single structured-document post-extraction pass for paragraphs, list items and quotes, including OCR and cross-page PDF paragraphs. Code, headings and tables remain unchanged.
- Add public dehyphenate_text and Stats { joined, skipped_long }; protect fenced code/table rows for direct text/Markdown helper calls.
- Add Options::dehyphenate_max_tokens (default 6; 0 disables), facade JSON option validation in 0..=64 and CLI --dehyphenate-max-tokens. Duplicate CLI/JSON settings are errors.
- Estimate joined words at four ASCII or three non-ASCII characters per token; preserve uppercase continuations, dashes, blank-line boundaries and over-budget candidates. Emit DEHYPHENATED when joined/skipped-long counts are nonzero.
- Update emitted PDF_TEXT_ONLY scope information and separate raw-line PDF tests from default de-hyphenation tests. External-backend/portable-text postprocessing is deferred; see crates/omniattach-documents/docs/DEHYPHENATION.md.
- No package/schema version bump, publication or completed validation is asserted.

## 0.3.0 - release preparation

Documents, facade and CLI target 0.3.0. Core and backends retain 0.1.0. This entry does not assert publication or completed release validation.

### Added

- MOBI and experimental AZW3 extraction in the document library and main CLI documents route.
- Uncompressed/PalmDOC/HUFF-CDIC text decoding with output, recursion and work limits and supported text-record trailer stripping.
- Referenced KF8 FDST flow validation and primary-flow projection, with explicit secondary-flow omission diagnostics.
- Ebook CLI probing/planning, explicit format overrides, canonical JSON/Markdown export and generated rejection/export tests.
- epub and ebooks feature aliases in the document crate; facade documents builds enable MOBI/AZW3 parsing.

### Fixed

- PDF real-valued TJ word adjustments no longer disappear solely because lopdf's spacing branch handles integer adjustments. The extraction-only shim uses the existing threshold and leaves small kerning adjustments unchanged.
- KF8 primary-text length is no longer compared against the combined decoded multi-flow stream when a validated referenced FDST table establishes separate ranges.
- CLI --version test follows CARGO_PKG_VERSION rather than a hardcoded release string.

### Compatibility and scope

- Public Format enums gained Mobi/Azw3 variants; downstream exhaustive matches may need updates.
- Internal release requirements for documents and facade move to 0.3.0. Do not resolve against earlier 0.2.x registry packages for the new ebook integration.
- Existing serialized schemas and exit codes are not bumped by package version changes.
- AZW3 remains a raw-text/HTML projection, not full skeleton/fragment, hybrid, navigation, styling or image reconstruction. Text/ordering evidence from two local books is not universal certification.
- PDF spacing compatibility is not a full layout engine. Source files are not rewritten.
- Root Cargo.lock must be refreshed with Cargo and all release checks run locally before publishing.
