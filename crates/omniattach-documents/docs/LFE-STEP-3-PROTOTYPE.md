# LFE-STEP-3: isolated image export prototype

This increment adds the `pdf-images` example, not a new library API. Run:

```sh
cargo run -p omniattach-documents --features pdf --example pdf-images -- book.pdf NEW_EXPORT_DIRECTORY
cargo test -p omniattach-documents --features pdf --example pdf-images
```

The destination must not exist. Its parent must exist. It receives `document.md`, `document.json` and `images/`. Partial output remains if writing fails; the writer never recursively deletes an export directory. Use a trusted destination parent; this is not a hardened concurrent filesystem sandbox.

The example reuses inspection anchors, STEP-2 names and the existing typed `BlockKind::Image` renderer. Each image occurrence gets a block; repeated references to the same indirect XObject share one asset. Equation/table hints are not emitted as duplicate source text. STEP-2's shared non-text numbering is preserved, including reserved names for equation/table candidates. No changes are made to Options, lib.rs, normal extraction or facade exports.

Only standalone DCTDecode JPEG or JPXDecode JPEG-2000 streams are copied. Masks and custom Decode are conservatively skipped, as are unsupported filters and images without indirect object IDs. Payloads are not codec-validated or color-converted. Byte and asset-count limits are checked before cloning image bytes; inspection retains its own input/content/operation limits.

Text is deliberately represented as separate raw text-show paragraphs. Traversal order is preserved, not visual reading order or canonical paragraph semantics. This prototype does not apply canonical paragraph joining, outlines, furniture removal, dehyphenation or OCR. Text fragmented into multiple show operations can therefore gain paragraph breaks. Clipping/visibility, inline images, raw-pixel PNG conversion, vector/composite figures, captions and image interpretation remain unsupported.

The example uses the provisional `omniattach-documents/0.2` schema marker for typed image-bearing output only. Package versions are not changed. JSON currently includes asset bytes as well as the separately saved files. No manifest/facade registration is added.

Four structural tests cover traversal and shared assets, unsupported-filter diagnostics, file/link correspondence and overwrite refusal, and path rejection before creating output. The PDF image payload is a deliberately non-decodable framing fixture, so these tests do not establish image-codec validity. Rust compilation/tests have not been executed in the preparation environment.

Before integration, run formatting, the example tests, existing image-rendering/PDF tests and workspace checks. Remaining STEP-3 work includes library/facade/CLI integration, basic raw raster support with strict bounds, mask/color-space handling or explicit diagnostics, and real-image/real-book validation. This prototype does not complete STEP-3.
