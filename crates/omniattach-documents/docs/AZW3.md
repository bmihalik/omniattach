# MOBI, EPUB and experimental AZW3 extraction

## Features and entry points

- `epub` aliases the existing `office` feature and its package reader.
- `mobi` enables the DRM-free MOBI text projection.
- `azw3` enables experimental KF8 raw-text projection.
- `ebooks` enables all three.

The library extract/extract_bytes/extract_as APIs recognize MOBI, EPUB and AZW3. Main-CLI MOBI/AZW3 routes are not included; EPUB already uses the main CLI's documents route.

```sh
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.mobi markdown
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.epub json
```

## Compression and trailers

The shared ebook reader handles compression 1 (uncompressed), 2 (PalmDOC) and 17480 (HUFF/CDIC). HUFF/CDIC references are read from record zero, and dictionary headers, offsets, phrase counts and indices are checked. Dictionary recursion is cycle-checked and depth-limited; decoded output and decoding work are capped. Recursive expansions are not cached without bounds. Backwards-length extra-data records and the bit-zero overlap suffix are removed before decompression, including flag combinations 1 and 3 reported by the two header summaries.

Decoded byte length must match the declared text length. A mismatch fails explicitly rather than silently returning truncated text. UTF-8 and Windows-1252 are supported without replacement decoding. Encryption remains rejected. The original source SHA-256, byte length and name are retained. No input file is rewritten and no external converter or DRM removal is used.

## Important limitations

HUFF/CDIC decompression is not KF8 reconstruction. Skeleton/fragment indices, hybrid boundaries, chapter order, navigation, images, fonts, layout and links are not reconstructed. Output remains a best-effort HTML projection and may be incomplete or misordered. Merely completing decompression does not certify faithful conversion. Some legitimate containers may still be rejected by strict validation or resource budgets.

Budgets are not an OS memory/CPU sandbox. Use resource-limited workers for untrusted inputs. The existing mobi dependency remains in the feature configuration, but the new bounded text decoding path does not call its unbounded decompression routines.

## Validation

```sh
cargo fmt --all
cargo test -p omniattach-documents --features azw3
cargo test -p omniattach-documents --features ebooks
cargo check -p omniattach-documents --all-features --all-targets
```

Generated tests cover literal and recursive HUFF phrases, cycles, malformed table offsets, work/output limits, PalmDOC back-references, invalid trailers, encrypted input and complete extraction of generated HUFF AZW3 containers with flags 1 and 3. They do not cover every valid HUFF code-table shape or establish compatibility with the user's actual books. Those books were not attached, only their header JSON summaries; local real-book regression tests remain required. The assistant has not compiled or run the Rust tests.

If extraction fails, retain the exact error. Useful errors identify invalid HUFF/CDIC references, code ranges, dictionary indices, cycles, trailers, decoding budgets or text-length mismatch. Do not bypass these checks to force apparently successful output. Updating Cargo.lock and selecting an unpublished package version are local release tasks; this change does not publish a package.
