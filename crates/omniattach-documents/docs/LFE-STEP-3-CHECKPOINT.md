# LFE-STEP-3: supported image export validation checkpoint

## Scope

This checkpoint adds generated-PDF public-API regressions and records existing validation. It does not alter the decoder, ordinary extraction, schemas, dependencies, lockfile, version or route. It does not merge or publish the feature branch.

`tests/pdf_predictor_images.rs` builds tiny synthetic PDFs with one indirect image painted twice between three text segments. Three tests cover Gray/RGB Predictor-15 Flate-to-PNG pixels (including different row filters), omitted/default Gray parameters and explicit RGB parameters, shared asset IDs, typed block placement in stream traversal order, Markdown reference counts, JSON round-trip, rejected predictor data/parameters/masks preserving text boundaries, and ordinary extraction remaining non-opt-in. A test-only stored-DEFLATE writer avoids adding a compression dependency. No real PDF or extracted book text is included.

## Validation evidence and limits

The user reported tests passing after decoder commit `11f9ebd` and supplied an audit with matching source hashes, `ok: true`, no issues, 74 assets, 74 image blocks, 74 Markdown links and 1,690,960 saved image bytes. No PDF_IMAGE_SKIPPED entries appeared; the only diagnostic codes were LFE_INSPECTION_SCOPE and LFE_IMAGE_EXPORT_EXPERIMENTAL. The earlier audit had 27 assets and 47 skip diagnostics. Independent attachment metadata identified 27 JPEG and 47 Predictor-15 Flate image objects. This is sample-specific evidence, not a general completeness claim.

The image-aware paragraph hashes were unchanged by Predictor-15 support: 109,580 alphanumeric characters and 31,904 whitespace tokens. Examination of both prior export JSON files established that removing exactly 96 numeric lines corresponding to printed pages 5 through 100 yielded the ordinary export's exact alphanumeric sequence. Those lines contained 188 digits; 67 were whole paragraphs and 29 were lines within larger paragraphs. The ordinary export separately reported 768 dehyphenation joins. Punctuation, whitespace, semantic fidelity and visual order were not certified by the alphanumeric comparison.

An independent Python reference reconstruction of all 47 Predictor-15 objects matched pypdf and Pillow on 2,034,650 pixel bytes. The sample used Sub on all 8,279 rows; separate independent Gray/RGB vectors exercised None/Sub/Up/Average/Paeth. This did not execute Rust. Pixel agreement for newly exported Rust PNG files has not been independently checked from the JSON audit alone.

The NEW Rust integration target in this checkpoint has not been compiled/executed in the preparation environment. Its two stored-zlib payloads were checked with Python zlib, and Pillow decoded both to the specified expected pixels. That validates fixture construction, not Rust integration. Prior user test success must not be reported as success for this newly added target.

## Run before review

```sh
cargo fmt --all
cargo test -p omniattach-documents --features pdf --test pdf_predictor_images
cargo test -p omniattach-documents --features pdf
cargo test -p omniattach-cli --test pdf_images
cargo clippy --workspace --all-features --all-targets -- -D warnings
python3 scripts/test_audit_pdf_image_export.py
python3 scripts/test_triage_pdf_image_export.py
python3 scripts/test_compare_pdf_export_text.py
```

Keep local real-document exports and reports out of commits. Open representative newly exported PNGs as an additional visual check. The audit validates references/bytes/metadata, not pixel semantics or coverage. Use `compare_pdf_export_text.py` rather than page-bucket triage for text comparison: ordinary paragraphs can span pages, and source.page is only their starting page. The triage utility still provides skipped-image reason summaries.

## Remaining boundaries

The image route remains experimental and follows content-stream traversal, not guaranteed visual reading order. No OCR, caption inference, composite/vector figure reconstruction or complete layout reconstruction is provided. Predictor support remains limited to direct FlateDecode with direct Predictor-15 parameters matching 8-bit DeviceGray/DeviceRGB geometry; TIFF/fixed PNG predictor values, array/indirect parameters, masks, custom Decode, unsupported bit depths/color spaces/filter chains and inline images remain outside this subset. Predictor budgeting includes filtered row bytes and reconstructed pixels; it is not a complete process peak-memory guarantee. Furniture removal and dehyphenation differences between ordinary and image-aware extraction remain intentional, not silently normalized here.
