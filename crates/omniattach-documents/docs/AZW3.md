# MOBI, EPUB and experimental AZW3 extraction

Available in the 0.3.0 document/facade/CLI release line. The main CLI now uses the same library extractor as azw3-extract; example-only descriptions from earlier development no longer apply.

## Features and usage

The document crate defaults to no optional parsers. epub aliases office, mobi enables MOBI, azw3 implies mobi, and ebooks enables all three. The facade's documents feature enables Office/PDF/MOBI/AZW3. The CLI enables documents by default.

```sh
cargo build -p omniattach-cli
./target/debug/omniattach probe book.azw3 --pretty
./target/debug/omniattach plan book.azw3 --goal markdown
./target/debug/omniattach convert book.azw3 --goal markdown --output new-book-export
./target/debug/omniattach convert book.mobi --goal markdown --output new-mobi-export
./target/debug/omniattach convert renamed.bin --format azw3 --output explicit-export
```

No external-backend flags or installed ebook converter are required. Outputs must use a new destination directory. See ../../omniattach-cli/docs/EBOOKS.md for transport details. For focused local debugging:

```sh
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
```

The example's stderr experimental warning follows successful extraction; it is not itself an error or a file-specific failure detector. Inspect exit status and the JSON diagnostics.

## Compression, trailers and flows

Uncompressed, PalmDOC and HUFF/CDIC text are supported with checked record/dictionary references and bounded output, recursion and work. Supported backwards-length trailers and the overlap suffix are removed before decompression. Encryption is rejected. UTF-8/Windows-1252 decoding does not replace invalid characters.

KF8 multiple-flow headers reference FDST using record-zero offsets 0xC0/0xC4. The reader validates signature, count, table bounds and contiguous ordered ranges. The complete flow range must cover the decoded buffer; flow zero must match the declared primary text length. Only flow zero is projected as HTML. AZW3_SECONDARY_FLOWS_SKIPPED records omitted flow counts and bytes. This is metadata-directed selection, not arbitrary truncation. Without an applicable reference, strict decoded-length equality remains in force.

## LLM-oriented scope

The goal is reliable text, useful structure and provenance rather than reader-identical rendering. Full KF8 skeleton/fragment reconstruction and hybrid boundary handling are not implemented. Reading order and completeness are not guaranteed for every book. Navigation, link targets, images, fonts and CSS are not reconstructed. Primary-flow extraction can work well for some books without proving generic compatibility. No heading is guaranteed solely because a paragraph looks prominent in a reader.

Original name, SHA-256 and byte length remain source identity. Model offsets must not be interpreted as original compressed-record positions. Resource budgets are not an OS sandbox; isolate hostile input. Converter diagnostics are forwarded by the facade and retained in document.json.

## Validation

```sh
cargo test -p omniattach-documents --features azw3
cargo test -p omniattach-documents --features ebooks
cargo test -p omniattach-cli --test ebooks
cargo check --workspace --all-features --all-targets
```

Generated tests cover compression/trailers, malformed tables, primary-flow selection and CLI exports. Real-book comparisons establish only sample-specific text evidence; further regression coverage is required. Release validation and Cargo.lock refresh remain local tasks. Package versions do not change schema versions automatically.
