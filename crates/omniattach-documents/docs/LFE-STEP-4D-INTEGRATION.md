# LFE-STEP4: narrow equation-aware Rust projection

With feature pdf, pdf_equations::recognize_image_equations consumes the existing
extract_pdf_with_images result plus explicitly selected IMAGE BLOCK IDs, a reusable
EquationRecognizer callback and EquationLimits. extract_pdf_with_equations is a
convenience wrapper. Ordinary extraction, Options and canonical Document/BlockKind
are unchanged. Empty selection invokes no model and preserves Markdown exactly.

EquationDocument keeps all original blocks/assets and adds occurrence-bound outcomes.
Its to_markdown replaces only successful selected image occurrences with display
LaTeX. One outer math delimiter pair is removed for display only; returned strings
remain in JSON. Its to_json uses omniattach/pdf-equations/0.1 with the base document
and equations array; do not confuse this wrapper with canonical Document JSON.
Failures, empty math, missing/unsupported assets and input/output size limits keep
image fallback and page/path diagnostics. Duplicate/unknown/non-image selections
and selection-count limits fail before callbacks. Reused assets keep distinct
occurrence results; native asset bytes are not duplicated during Markdown rendering.

Supported inputs are explicitly selected native PNG/JPEG assets, not arbitrary
viewer-space regions. No classifier, page renderer, rotation correction, OCR,
automatic model download or semantic-equivalence claim is added. No prose/math
text is removed or reflowed beyond the existing image projection. The callback is
trusted in-process code: caller owns deadlines/process isolation; byte/result
checks are not hard RAM/CPU bounds. Model completion/truncation remains unknown.

## Real Bobine path (isolated example)

The existing worker binary/protocol stays unchanged. The isolated tool gains only
a dev-dependency on the local document crate for this example; ordinary workspace
builds gain no Bobine/ORT dependency. Model/export licensing remains the caller's
responsibility. Models must already exist locally. Use the existing ORT_DYLIB_PATH.

```bash
cargo fmt --all
cargo test --locked -p omniattach-documents --features pdf \
  --test pdf_equations --test pdf_images
cargo check --locked -p omniattach-documents --no-default-features
cargo check --manifest-path tools/lfe-formula-bobine/Cargo.toml --example pdf-equations

# List occurrence block IDs without loading a model:
cargo run --release --manifest-path tools/lfe-formula-bobine/Cargo.toml \
  --example pdf-equations -- --list INPUT.pdf

# Explicitly choose known equation-image blocks from that SAME input:
cargo run --release --manifest-path tools/lfe-formula-bobine/Cargo.toml \
  --example pdf-equations -- INPUT.pdf NEW_OUTPUT_DIRECTORY \
  ENCODER.onnx DECODER.onnx TOKENIZER.json 512 BLOCK_ID [BLOCK_ID...]
```

Bobine loads once and is reused for selected occurrences. The new output directory
contains document.md, wrapper document.json and images/ for preserved fallback.
Its parent must exist; input/output files are not overwritten. Partial NEW output
may remain on I/O failure. Use a trusted output parent, not a hostile shared folder.
The isolated tool's local Cargo.lock will need Cargo resolution for its new dev-
dependency; retain the resolved lock with run evidence. No lock regeneration or
root manifest/package version change is asserted here. CLI/facade registration,
automatic selection, rendered/vector/native-text equation recognition are deferred.

## Verification

Seven Rust integration tests are prepared for selected substitution, shared-asset
occurrence identity, JSON/source provenance, fallbacks, selection preflight, limits,
unsupported assets, unchanged ordinary extraction and no OCR. Cargo/rustc/rustfmt
are unavailable in the preparation environment: compilation, tests, formatting,
real example inference and CI are NOT claimed passed. lib.rs base Git blob was
verified before adding only the feature-gated module declaration. Run the commands
above and normal workspace checks before treating the feature as validated.
