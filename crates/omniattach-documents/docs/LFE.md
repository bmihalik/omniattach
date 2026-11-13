# LFE-STEP-1: lightweight PDF inspection

The existing lopdf 0.46 content interpreter is shared by ordinary extraction and opt-in inspection. No second parser, renderer, model, transliteration or image-codec dependency is added. Ordinary extract/extract_as still call the interpreter without a collector; text, Markdown, OCR, dehyphenation and existing asset behavior are intended to remain unchanged. Candidate inspection is a separate report, not a new Document field or changed canonical schema.

With feature pdf, use inspect_pdf(path, &Options) or inspect_pdf_bytes(original_name, bytes, &Options). Both return PdfInspection; to_json serializes the inventory. The facade's documents namespace re-exports the library when enabled. Stock CLI integration is not added.

```sh
cargo run -p omniattach-documents --features pdf --example pdf-inspect -- book.pdf
```

The report records original source name/SHA-256/length, page count, document-wide one-based sequences and diagnostics. Each element carries a one-based page and operation-visit anchor, referenced Form-object path, optional image object/resource identity, optional pixel dimensions and optional bounds. Reused images remain separate occurrences. Operation visits are expanded traversal counters, not byte offsets or original content-array indices. Form paths identify referenced objects; direct embedded Forms have no invented object ID.

Text elements are raw decoded text-show runs, not paragraphs or a final reading order. Furniture remains present; whitespace, dehyphenation, OCR and multi-column sorting are not applied. A TJ-derived word_gap_before hint is retained, but it is not a complete word/line reconstruction policy. Do not concatenate raw runs as if they were canonical Markdown. Use ordinary extraction for the existing prose output.

Image XObjects are unclassified occurrences; image bytes are not decoded or saved by inspection. Equations use short equality/math-symbol hints and direct Formula marked-content tags; tables use direct Table tags or a simple grid contained in one stroked path with three spanning rulings per axis. Derived hints reference original text sequence IDs without duplicating its text. These are overlapping hypotheses, not recognized math/cells or calibrated probabilities. Image-only equation/table classification, borderless or separately painted grid assembly and full tagged-PDF structure-tree resolution are not implemented.

Bounds are internal PDF user-space extents after content transforms. They are unclipped and not normalized for page CropBox/Rotate/UserUnit. Text bounds are explicitly estimated, not glyph outlines. Image bounds describe a transformed unit-square extent, not necessarily its visible cropped appearance. This metadata is not inserted into normal LLM-facing Markdown.

Inline-image parsing is not promised: if the underlying decoder exposes BI, an unresolved-inline-image diagnostic is recorded; unsupported content may still fail parsing. Type3 glyph programs, clipping/visibility semantics, vector-figure assembly and general layout reconstruction remain outside this step.

Limits reuse input_bytes, archive_bytes for decompression/text, blocks for total candidates/pages, archive_entries for per-painted-path segments and per-page Form calls, and xml_nodes for per-page operation/text-item work. Form depth/graphics stack guards remain. Additional inspection text is bounded across pages and element budgets are reduced as pages are processed. These are not hard process RAM/CPU bounds.

Validation commands: cargo fmt --all; cargo test -p omniattach-documents --features pdf --test pdf_inspection; cargo test -p omniattach-documents --features pdf --test pdf; cargo test -p omniattach-documents --features pdf --test pdf_spacing; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets; cargo clippy --workspace --all-features --all-targets -- -D warnings. Generated fixtures test inventory behavior, not real image decoding or universal PDF compatibility. Rust/Cargo tests have not been run in the preparation environment.

LFE-STEP-2 will add deunicode-based names; STEP-3 will save native assets to images/ with Markdown links; STEP-4 adds optional equation recognition; STEP-5 handles table conversion/continuations. None is silently enabled here. No package version bump, merge or publication is included.
