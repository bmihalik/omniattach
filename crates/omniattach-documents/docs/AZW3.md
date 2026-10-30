# MOBI, EPUB and experimental AZW3 extraction

## Features and entry points

- `epub` aliases the existing `office` feature and its package reader.
- `mobi` enables DRM-free MOBI text projection.
- `azw3` enables experimental KF8 raw-text projection.
- `ebooks` enables all three.

The library extract/extract_bytes/extract_as APIs recognize MOBI, EPUB and AZW3. Main-CLI MOBI/AZW3 routes are not included; EPUB already uses the documents route.

```sh
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.mobi markdown
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.epub json
```

## Compression and trailers

The reader handles uncompressed, PalmDOC and HUFF/CDIC text. HUFF/CDIC references, dictionary offsets, counts and indices are checked. Recursion is cycle-checked and depth-limited; output and decoding work are bounded. Extra-data and overlap trailers are removed before decompression. Encryption is rejected; UTF-8 and Windows-1252 decoding do not replace invalid characters. Original source name, size and SHA-256 are retained.

## KF8 flow-aware length validation

KF8 can store multiple flows in the decompressed text-record stream. For the reported books, the header text length matches flow zero, not the entire stream. The FDST record explicitly accounts for the remaining decoded bytes.

For version 8 with multiple flows, the reader resolves the FDST index and flow count from record-zero offsets 0xC0 and 0xC4. It does not scan for an arbitrary FDST signature. It validates the referenced record signature, table bounds, flow-count budget and agreement with the header. Ranges must start at zero, be contiguous and ordered, remain within decoded bytes, and end exactly at the combined decoded length. The first endpoint must match the declared primary text length.

Only flow zero is decoded as HTML for the experimental text projection. Secondary flows are not rendered or exported; AZW3_SECONDARY_FLOWS_SKIPPED records their count and combined size. This is metadata-directed flow selection, not unconditional truncation to bypass an error. Without an applicable multiple-flow header, the original strict decoded-length equality remains in force. Single-flow headers do not activate potentially meaningless FDST indices.

## Limitations

Flow selection and decompression do not reconstruct KF8 skeletons/fragments or hybrid boundaries. Reading order and completeness remain unverified. Navigation, images, fonts, styling and link targets are not reconstructed. Some legitimate files may still be rejected by strict checks. Budgets are not an OS sandbox; isolate hostile input. No DRM removal, network fetches or external converter are invoked.

## Validation

```sh
cargo fmt --all
cargo test -p omniattach-documents --features azw3
cargo test -p omniattach-documents --features ebooks
cargo check -p omniattach-documents --all-features --all-targets
```

Generated tests cover decompression and trailers, malformed flow tables, both reported primary/total length pairs, primary-only text/Markdown projection, invalid FDST references and strict rejection without an applicable reference. The length-pair tests model the reported endpoints; they do not embed the actual books or certify full reconstruction. The assistant has not compiled or run Rust tests. Re-test both original files locally using azw3-extract and inspect diagnostics and resulting text before relying on it. No version bump or package publication is included.
