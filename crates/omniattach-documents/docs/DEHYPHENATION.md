# Line-end de-hyphenation

The document library applies one post-extraction pass to Paragraph, ListItem and Quote text, including OCR. Options::dehyphenate_max_tokens defaults to 6; 0 disables it. Headings, code, tables, source identity, spans and IDs are unchanged. Cross-page PDF paragraphs retain their first source page. Rendering and document JSON reflect the transformed text.

The public dehyphenate::dehyphenate_text(text, max_tokens) returns (String, Stats { joined, skipped_long }) and additionally protects fenced code and pipe-table rows. Structured prose does not use those Markdown heuristics because block types already identify code/tables.

A letter followed by -, U+00AD or U+2010 at line end joins a lowercase continuation. Its first word and trailing punctuation move up; the remaining text stays on its line. Uppercase/digit continuations, dashes and blank-line boundaries are retained. The reference estimates ceil(word characters / 4) for ASCII and / 3 otherwise, not actual tokenizer output. Long candidates stay unchanged. DEHYPHENATED reports joined/skipped-long counts when either is nonzero; it is absent when disabled or nothing qualifies. Source spans remain original-input provenance, not offsets into rewritten text.

This is the library stage of development. Facade request-option parsing, the dedicated CLI flag, final changelog integration and external-backend scope documentation follow in a separate commit. No external-backend outputs are postprocessed in this stage. Plain-text extraction currently emits separate blocks per line, so this per-block pass does not join those blocks; callers can use the public text helper on whole text when appropriate.

Validation: cargo test -p omniattach-documents --all-features and cargo fmt --all. Existing PDF line-fidelity tests explicitly disable the pass; separate tests exercise default-on page joins and OCR. These tests have been prepared but not executed here: Rust/Cargo/rustfmt are unavailable. No package/schema version or publication change is included.
