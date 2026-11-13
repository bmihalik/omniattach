# LFE-STEP-3: explicit image projection library API

With the `pdf` feature, call `extract_pdf_with_images(name, bytes, &Options)` to obtain a typed `Document` with text-run and image blocks plus in-memory assets. The call itself is the opt-in: Options, extract, extract_bytes, extract_as and facade routes are unchanged. The API writes no files. The pdf-images example now calls the library and retains only input/export handling.

```sh
cargo test -p omniattach-documents --features pdf --test pdf_images
cargo test -p omniattach-documents --features pdf --example pdf-images
cargo run -p omniattach-documents --features pdf --example pdf-images -- book.pdf NEW_EXPORT_DIRECTORY
```

The destination must not exist and its parent must exist. The example writes document.md, document.json and images/. It validates flat image filenames and refuses overwrite. Partial new output remains on write failure; it never recursively deletes it. Use a trusted destination parent: this is not a hardened concurrent filesystem sandbox. JSON still contains asset bytes in addition to separately saved files.

The projection reuses inspection traversal/guards and STEP-2 naming. Every supported occurrence gets an image block with page and operation/Form provenance; indirect XObjects share asset bytes across occurrences/pages. Asset provenance describes its first occurrence and object ID. Equation/table hints remain overlapping hints and are not duplicated as source text. The shared non-text naming sequence is unchanged.

Only a single DCTDecode JPEG or JPXDecode JPEG-2000 filter is supported. Masks, custom Decode and DecodeParms are conservatively skipped, as are unsupported filters and missing indirect image IDs. Payloads are not codec-validated or color-converted; clipping/visibility, CMYK interpretation, inline images and vector/composite figures are not reconstructed. Input, inspection content/operation, element, asset-byte and asset-count limits apply. Asset bytes are bounded before cloning. The current implementation parses the PDF for inspection and again for asset access; this can be consolidated later without altering the public API.

Each text-show run remains a separate paragraph. This is explicit experimental behavior, not canonical PDF paragraph semantics: text fragments can gain paragraph breaks, though source text is not intentionally merged or removed. No canonical paragraph joining, furniture removal, outlines, dehyphenation or OCR is applied. Traversal order is not visual reading order. Reuse/extend the existing line-aware interpreter between image boundaries before proposing this as the default LLM-ingestion route.

Image-bearing output retains the provisional omniattach-documents/0.2 schema marker; default extraction remains 0.1. No package/dependency version is changed. Facade/CLI integration and artifact manifest registration remain follow-up work, as do basic raw-raster PNG support and real-document validation. STEP-3 is not complete.

Seven library integration tests cover real JPEG byte/hash/link preservation, explicit fragmented-text ordering, nested Forms, cross-page sharing/provenance, masks/Decode/filter rejection, resource limits and unchanged ordinary extraction/no OCR invocation. Two example writer tests retain export/overwrite and path rejection coverage. The real fixture is a generated solid-color 2x1 RGB JPEG, stored as hex for text-only repository transport. Pillow generated and decoded it during preparation; SHA-256 fda4cc9e77660582cae5623b185dcc9ace4b860ac5bb00f1c573e3bd1bc3ad7f. Rust tests check byte preservation, not independent JPEG decoding. JP2 codec validity is not tested.

Rust compilation/tests for this increment have not been run in the preparation environment. Run cargo fmt --all, the commands above, existing image-rendering/PDF tests, cargo test --workspace --all-features and cargo check --workspace --all-features --all-targets before integration. No merge or publication is included.
