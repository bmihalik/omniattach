# LFE-STEP-3: conservative line-aware mixed text

This increment supersedes the earlier one-paragraph-per-text-show description for the opt-in extract_pdf_with_images/custom:pdf-images path. Adjacent decoded text runs are assembled using estimated inspection bounds, TJ word-gap hints and the established canonical line builder's thresholds: horizontal gaps over 0.18 of estimated text height imply a space; baseline changes over 0.5 of height imply a new line; larger/common-leading gaps separate paragraphs. Normalization matches the established whitespace/control handling. No column sorting, caption/heading inference, OCR, running-header removal, outlines or dehyphenation is introduced.

The assembly code mirrors pdf_spacing::build_lines but is not yet the same shared function. The canonical interpreter retains signed origins/advance metrics; inspection exposes only normalized estimated bounds. Keeping canonical extraction untouched avoids changing its behavior merely to support the experimental projection. Factoring a genuinely shared metric-preserving builder is a follow-up refactor, not claimed by this increment. Bounds-based joining is most appropriate for ordinary horizontal positive-advance text; rotated/reversed/vertical writing remains outside the fidelity guarantee. Missing or unusable bounds produce a separately emitted run with LFE_TEXT_GEOMETRY_FALLBACK.

Pending text is flushed before every image occurrence, before testing image eligibility. Unsupported masks/filters/malformed payloads therefore do not allow prose to join across omitted image content. Page and observed Form-path changes, plus undecodable runs, also stop joining. Form changes without an intervening observed text/image event are not independently represented by inspection and cannot be used as boundaries. Inline BI images are not inventory occurrences and remain unsupported.

Paragraph source.page describes its page; source.path records the containing text segment's inclusive operation range and Form path. If a segment becomes several paragraphs, those paragraphs share the segment range; this is conservative coverage, not exact per-paragraph run attribution. Image occurrence and asset provenance, naming, encoded/decoded budgets and facade artifact paths are unchanged. Assembled paragraph bytes and output block counts are bounded. Source-run bytes remain independently bounded by inspection.

Five assembler unit tests cover adjacent fragments, geometry/TJ gap hints, line/paragraph gaps, page/Form/undecodable boundaries, missing geometry, provenance and limits. Existing generated-PDF integration tests are updated to expect before/image/after/image instead of be/fore/image/after/image, and explicitly verify skipped-image boundaries and a fixed ordinary-extraction text result. Existing nested-Form, image-sharing, raster and CLI tests remain required. Unit gap-hint tests operate on inspection elements; they do not establish arbitrary real-PDF typography.

```sh
cargo fmt --all
cargo test -p omniattach-documents --features pdf
cargo test -p omniattach-cli --test pdf_images
cargo test --workspace --all-features
cargo check --workspace --all-features --all-targets
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

Rust compilation/tests have not been run in the preparation environment. No dependency, lockfile, version, schema, route or default-extraction change is included. After validation, compare a real mixed-text/image PDF's prose continuity and image references against ordinary extraction before considering STEP-3 complete. No merge or publication is included.
