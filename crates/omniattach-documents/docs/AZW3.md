# Experimental AZW3 text projection

This first implementation is an opt-in example executable, not full AZW3/KF8 support and not a registered main-CLI route. Existing extract/extract_bytes APIs do not yet route .azw3 files. The example uses the MIT-licensed mobi 0.8 Rust crate and the existing HTML document extractor; no Calibre, Python, shell command, network access or DRM removal is involved.

## Run

From the repository root:

```sh
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 markdown
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 text
```

JSON includes projection diagnostics. Markdown and text output also print an experimental warning on stderr. Output is written to stdout, not beside the source.

## Scope

- Parses BOOKMOBI Palm containers with raw or PalmDOC-compressed text.
- Uses strict text decoding provided by mobi and the existing HTML-to-block/table projection.
- Retains the original input name, byte length and SHA-256 rather than misattributing decoded HTML as the original source.
- Rejects encrypted files, HUFF/CDIC compression, record trailers, malformed record offsets and unsupported text-record ranges.
- Checks input size, record count, declared text length and a conservative expansion bound before text decoding. Existing block/cell limits still apply. Dependency parser allocations are not a hard process memory/CPU sandbox; isolate untrusted files.

This is raw-text recovery, not a reconstructed ebook. KF8 skeleton/fragment indices, chapter order, navigation, images, fonts, links, layout and hybrid MOBI/KF8 boundaries are not reconstructed. Some valid AZW3 files will be rejected; others may produce incomplete or misordered text. Successful extraction is not proof of complete or faithful book conversion. Use only DRM-free books you are authorized to process.

## Validation

```sh
cargo test -p omniattach-documents --features azw3 --example azw3-extract
cargo check -p omniattach-documents --all-features --all-targets
cargo fmt --all -- --check
```

The supplied tests cover basic invalid input and size rejection, not compatibility with real Kindle exports. Real DRM-free AZW3 fixtures, success-path tests, decompression edge cases and a dependency security review remain necessary before treating this as production support. Cargo.lock must be updated by Cargo locally. This change does not bump or publish a package version; select a new version before publishing an already-released package.
