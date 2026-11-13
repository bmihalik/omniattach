# LFE-STEP-3: opt-in facade and CLI image export

The CLI registers an in-process `pdf-images` route when its `documents` feature is enabled (including default builds). Select it with the dedicated custom goal, not the ordinary Markdown goal:

```sh
cargo run -p omniattach-cli -- plan book.pdf --goal custom:pdf-images --route pdf-images --pretty
cargo run -p omniattach-cli -- convert book.pdf --goal custom:pdf-images --route pdf-images --output NEW_EXPORT_DIRECTORY
```

No external-backend registration or execution permission is required. `--route pdf-images` is optional with this dedicated goal because only one route matches. `--goal markdown --route pdf-images` is intentionally a route mismatch. Adding another route for ordinary PDF Markdown would otherwise make default planning ambiguous. No registry selection logic, goal enum, default text extraction or automatic fallback changes.

Facade callers explicitly register `OmniAttach::builder().with_pdf_image_projection()?` and request `Goal::Custom("pdf-images".into())`, optionally `.with_route("pdf-images")`. `with_document_defaults()` alone continues to register only its existing routes. The CLI registers both.

The converter calls the tested documents `extract_pdf_with_images` API. It writes `document.md`, `document.json`, and supported JPEG/JP2 assets at the exact `images/...` paths referenced by Markdown. Each file is a declared facade artifact and appears in manifest.json; image artifact locations retain asset ID, SHA-256 and first-occurrence provenance. The original request source name feeds naming, not the staged input.pdf filename. The image-bearing document payload is namespaced by its actual schema, currently omniattach-documents/0.2. Diagnostics are forwarded to facade warnings. Plain-text stdout remains document text with image markers, not Markdown.

The existing facade bundle validates paths/files and performs durable export with its normal new-directory policy. The adapter separately validates generated flat image names, unique IDs, byte lengths, hashes and image-block asset references. File/byte budgets include document.json and document.md as well as image files; embedded bytes inside JSON count again toward the budget. Existing facade manifest/payload accounting remains unchanged. Policy input, content and asset limits are applied before extraction; the aggregate artifact budget is checked before writing output artifacts. Cancellation/deadline checks bracket extraction and artifact operations, not each internal PDF operator.

Request options must be empty or null. OCR, custom dehyphenation options and `--dehyphenate-max-tokens` are rejected rather than silently ignored. The underlying experimental raw-run scope is unchanged: one paragraph per text-show run, traversal rather than visual order, no canonical text joining, no color conversion/codec validation, masks/custom Decode/DecodeParms skipped, no inline images, raw-pixel PNG encoding, composite/vector figures, captions or OCR. This is not the default LLM-ingestion path.

Three CLI integration tests exercise default planning, in-process selection, manifest/artifact registration, image reuse and relocatability, option/budget rejection, unsupported filters and overwrite refusal. Their ASCII image payload is structural, not decodable JPEG; the library's real-JPEG fixture tests remain the separate byte-preservation evidence.

Validate with cargo fmt --all; cargo test -p omniattach-cli --test pdf_images; cargo test -p omniattach-documents --features pdf --test pdf_images; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets; cargo clippy --workspace --all-features --all-targets -- -D warnings. This increment has not been compiled or tested in the preparation environment. No dependencies/versions, merge or publication are included. Raw-raster PNG support, line-aware mixed text assembly and real-document validation remain before broader STEP-3 completion.
