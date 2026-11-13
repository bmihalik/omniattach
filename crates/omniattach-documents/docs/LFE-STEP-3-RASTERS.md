# LFE-STEP-3: bounded basic raster PNG export

This increment supersedes the JPEG/JP2-only support statements in the earlier prototype/CLI notes. The opt-in library API and custom:pdf-images CLI route now also convert unfiltered or single-FlateDecode 8-bit DeviceGray/DeviceRGB image XObjects to standalone PNG assets. The CLI goal, paths, asset sharing, manifest export, schema and default extraction remain unchanged. The facade accepts image/png alongside image/jpeg and image/jp2.

The png 0.17 crate is optional and activated only by the existing pdf feature. Cargo.lock must be resolved and committed during local validation; it was not regenerated here. No crate version is bumped. No handcrafted PNG encoder or external process is introduced.

Raster eligibility is intentionally narrow: direct positive Width/Height, direct BitsPerComponent 8, direct DeviceGray or DeviceRGB, no masks, Decode, DecodeParms/predictors or filter chains. Indirect color-space/filter/dimension metadata, indexed/ICC/CMYK color spaces, other bit depths and inline image payloads remain unsupported and receive PDF_IMAGE_SKIPPED. Even explicit identity Decode or predictor-free DecodeParms dictionaries are conservatively skipped for now. JPEG/JP2 passthrough behavior remains unchanged.

Checked arithmetic calculates width*height*channels before allocation. Each unique raster reserves that decoded-byte count from a document-wide archive_bytes budget; repeated supported XObjects reuse their encoded asset without decoding again. Unfiltered streams must have exactly that length. Flate decoding uses lopdf's bounded content API and must produce exactly the expected length. Unsupported or malformed raster data is skipped with a diagnostic, never mislabeled as PNG. Oversized dimensions/budgets fail with LimitExceeded. Invalid attempts consume their reserved raw budget; this is conservative.

PNG encoding uses a capped writer that refuses bytes beyond the remaining encoded-asset budget. All encoded assets combined still obey archive_bytes, and the facade additionally counts Markdown and JSON toward max_export_bytes. Decoded and encoded budgets are independent bounds, not a single peak-memory guarantee; source buffers, library codec state and JSON serialization also consume memory. This is not page rendering: transforms, clipping, visibility, soft-mask compositing, captions and vector/composite figures are not reconstructed. Raw text-show paragraph behavior is unchanged.

Three new integration tests cover four Gray/RGB unfiltered/Flate pixel round trips through the PNG decoder, repeated-asset reuse and links; malformed/unsupported raster diagnostics; and dimension/cumulative decoded budgets. A unit test exercises capped output writes. Existing JPEG/JP2-path and CLI regression tests must still pass. The compressed sample byte arrays were generated with Python zlib; Rust compilation and PNG round-trip tests have not been executed in the preparation environment.

```sh
cargo fmt --all
cargo test -p omniattach-documents --features pdf --test pdf_raster_images
cargo test -p omniattach-documents --features pdf
cargo test -p omniattach-cli --test pdf_images
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

Commit the resulting Cargo.lock update as well as formatting after validation. Line-aware mixed text assembly and real-document validation remain STEP-3 follow-up work. No merge or publication is included.
