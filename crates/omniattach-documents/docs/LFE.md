# LFE: lightweight PDF inspection and naming

STEP-1 reuses the existing lopdf 0.46 content interpreter for ordinary extraction and opt-in inspection. Default extract/extract_as use no candidate collector; text, Markdown, OCR, dehyphenation and existing assets are intended to remain unchanged. The candidate report is separate from Document and does not change its canonical schema. STEP-2 adds optional deunicode-based naming under the pdf feature; see LFE-NAMING.md. No renderer, model or image-codec dependency is added.

With feature pdf, inspect_pdf(path, &Options) and inspect_pdf_bytes(original_name, bytes, &Options) return PdfInspection. to_json serializes the inventory. naming_plan() returns a separate deterministic naming plan without mutating the report or creating files. The facade's documents namespace re-exports these APIs. Stock CLI integration is not added.

```sh
cargo run -p omniattach-documents --features pdf --example pdf-inspect -- book.pdf
cargo run -p omniattach-documents --features pdf --example pdf-inspect -- --names book.pdf
```

Inspection records original source name/hash/length, page count, document-wide one-based sequences and diagnostics. Elements have one-based page/operation-visit anchors, referenced Form paths, optional image object/resource identity, pixel dimensions and bounds. Reused images are separate occurrences. Operation visits are expanded traversal counters, not byte offsets. Direct Forms receive no invented object IDs.

Text elements are raw decoded runs, not paragraphs or final reading order. Furniture remains; reflow, dehyphenation, OCR and column sorting are not applied. TJ word_gap_before is only a hint. Do not concatenate these records as canonical Markdown. Use ordinary extraction for prose output.

Images are unclassified XObject occurrences; inspection does not save/decode pixels. Equation hints use short equality/math-symbol text or direct Formula tags. Table hints use direct Table tags or a grid in one stroked path with at least three spanning rulings on each axis. Derived hints reference text sequences without copying the source text. Hints can overlap and are not recognized structures or calibrated probabilities. Full structure trees, image-only classification, borderless/separately painted tables are not implemented.

Bounds are internal transformed PDF user space, unclipped and not normalized for CropBox/Rotate/UserUnit. Text bounds are estimated, not glyph outlines. Image bounds are transformed unit-square extents, not necessarily visible cropped appearances. This metadata does not enter ordinary LLM-facing Markdown. Inline-image inspection is not guaranteed: observed BI operators get a diagnostic, and unsupported content can still fail parsing. Type3 glyph programs, clipping/visibility semantics and composite/vector figures remain outside STEP-1.

Existing limits bound input, decompression/text, total elements/pages, path segments, Form calls/depth and operation work. Per-page element budgets decrease as the report fills. These are not hard RAM/CPU sandbox bounds.

Run cargo fmt --all; cargo test -p omniattach-documents --features pdf --test pdf_inspection; cargo test -p omniattach-documents --features pdf --test lfe_names; cargo test -p omniattach-documents --features pdf --test pdf; cargo test -p omniattach-documents --features pdf --test pdf_spacing; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets; cargo clippy --workspace --all-features --all-targets -- -D warnings. The user reported STEP-1 tests passing; STEP-2 tests are prepared but not executed here. Fixtures do not certify universal PDF compatibility or real image decoding.

STEP-3 will save native assets to images/ with Markdown links. STEP-4 adds optional equation recognition. STEP-5 handles table conversion/continuations. Naming alone enables none of these operations. No package-version bump, merge or publication is included.
