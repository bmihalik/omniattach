# Line-end de-hyphenation

The library applies one pass at the end of extract_as to Paragraph, ListItem and Quote text, including OCR. Options::dehyphenate_max_tokens defaults to 6; 0 disables. Headings, code, tables, source identity, spans and IDs are unchanged. Cross-page PDF paragraphs retain their first source page. Text, Markdown, canonical payloads and document.json reflect the same transformed blocks.

Public API: omniattach_documents::dehyphenate::dehyphenate_text(text, max_tokens) returns (String, Stats { joined, skipped_long }). This helper protects fenced code and pipe-table rows; structured prose uses block kinds instead.

A letter followed by -, U+00AD or U+2010 at LF line end joins a lowercase continuation. Its first word and trailing punctuation move up; the rest stays on its line. Dashes, digit/uppercase continuations and blank-line boundaries remain. Chained joins are supported. Token estimates are ceil(word characters / 4) for ASCII and / 3 otherwise, matching the supplied reference rather than a model tokenizer. Over-budget candidates stay unchanged. DEHYPHENATED reports joined/skipped-long counts only when nonzero and enabled. Original spans are provenance, not rewritten-text offsets.

## Facade and CLI

With the facade documents feature and registered document defaults:

```rust,ignore
ConversionRequest::new("book.pdf", Goal::Markdown)
    .with_options(serde_json::json!({"dehyphenate_max_tokens": 6}));
```

Document options accept only integer JSON values 0..=64; null options or omission select the default. A null/string/bool/floating-point field is invalid. The setting is independent of OCR enablement. Existing OCR language/DPI constraints remain.

```sh
omniattach convert book.pdf --goal markdown --dehyphenate-max-tokens 6 --output new-export
omniattach convert book.pdf --goal markdown --dehyphenate-max-tokens 0 --output raw-export
omniattach convert book.pdf --options-json '{"dehyphenate_max_tokens":6}' --output json-export
```

The CLI flag is available on plan/convert, merges with other JSON/file options and rejects a duplicate dehyphenate_max_tokens key even if the values agree. Omission does not inject a new option into unrelated routes.

## Scope and validation

Automatic processing is limited to the structured documents route. External-backend outputs (including MinerU, transcription and custom converters) and portable-text routes are not postprocessed here; adapters/callers can explicitly use the text helper. Plain-text library extraction emits separate blocks per source line, so the per-block pass cannot join those blocks. OCR processing needs line breaks in the supplied callback text; this change does not reconstruct lines from the facade's existing flattened Tesseract word list. No second pass runs on rendered Markdown.

Existing PDF line-fidelity tests explicitly disable the pass; separate tests cover default-on cross-page joining and OCR. Reference, structured-block, facade-parser and CLI-transport tests are included. Rust/Cargo/rustfmt are unavailable in the preparation environment: no compilation or test success is claimed. Run cargo fmt --all, cargo test --workspace --all-features and cargo check --workspace --all-features --all-targets before merging. No version/schema bump or publication is included. Thinkerizer integration/removal remains a later consumer change after release.
