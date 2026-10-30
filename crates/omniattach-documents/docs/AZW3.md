# MOBI, EPUB and experimental AZW3 extraction

## Feature choices

- `epub`: alias for the existing `office` feature, including its ZIP and spreadsheet dependencies. EPUB support is not a new parser; it retains the existing EPUB implementation.
- `mobi`: enables DRM-free MOBI text extraction using the mobi 0.8 Rust crate.
- `azw3`: enables the same parser with explicit experimental KF8 text projection.
- `ebooks`: enables all three.

The normal library APIs now recognize .mobi, .epub and .azw3 extensions, case-insensitively. Explicit overrides use Format::Mobi, Format::Epub or Format::Azw3.

```rust,ignore
use omniattach_documents::{extract, Options};
let doc = extract("book.mobi", &Options::default())?;
println!("{}", doc.to_markdown());
```

Run the existing conversion example with ebook features:

```sh
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.mobi markdown
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.epub json
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
```

## Boundaries

EPUB already has a route in the main omniattach CLI when documents support is compiled in. This change does not add MOBI or AZW3 format enums/routes to the facade or main CLI, or enable ebook features transitively there. Use the document library or its examples for MOBI/AZW3.

MOBI supports uncompressed and PalmDOC-compressed text records, strict decoding and HTML projection into the existing document model. It rejects encrypted/DRM files, HUFF/CDIC compression, record trailers and unsupported text ranges. KF8 under Format::Mobi is rejected rather than silently advertised as legacy MOBI support. Explicit AZW3 projection remains experimental: skeletons/fragments and hybrid boundaries are not reconstructed, so completeness and reading order are not guaranteed.

Images, fonts, navigation, page layout and link destinations are not preserved by this MOBI projection. Diagnostics describe the losses. Original input SHA-256, name and byte length remain the document source identity; no compressed-record offsets or rendered page positions are invented. No DRM removal, external converter or model download is included.

Input/record/decoded-byte/block/cell checks are not an OS memory or CPU sandbox. Dependency parser internals may allocate before checks; isolate hostile inputs.

## Validation

```sh
cargo test -p omniattach-documents --no-default-features --test ebooks
cargo test -p omniattach-documents --features mobi --test ebooks
cargo test -p omniattach-documents --features ebooks
cargo check -p omniattach-documents --all-features --all-targets
cargo fmt --all
```

New tests cover routing, disabled features, malformed input and input limits. They do not establish successful real-book compatibility. Add authorized DRM-free fixtures and success-path/DRM/compression tests before production certification. This change has not been compiled by the assistant. Update Cargo.lock locally with Cargo and select a new package version before publishing an already-released version.
