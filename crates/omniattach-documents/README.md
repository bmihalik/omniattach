# omniattach-documents

Version 0.3.0. Local document extraction into typed blocks, tables, diagnostics and source metadata for LLM ingestion. Markdown, text, JSON and CSV are projections of the canonical Document; they are not visual renderings of the original.

## Formats and features

| Input | Feature | Scope |
|---|---|---|
| Text/source/Markdown/HTML/CSV/TSV/JSON | None | Text, code, semantic blocks and supported tables |
| XML | None | Explicit namespace-aware record/column mapping, not automatic schema inference |
| DOCX/PPTX/ODF/EPUB/workbooks | office | Package/document parsing; format-specific diagnostics describe losses |
| PDF | pdf | Page text, bookmark headings, supported embedded images; no general layout/table reconstruction |
| EPUB | epub or office | Existing spine-ordered text/table extraction; epub aliases office |
| MOBI | mobi | DRM-free uncompressed, PalmDOC and HUFF/CDIC text projection |
| AZW3 | azw3 | Experimental KF8 primary-flow HTML projection; implies mobi |
| All ebooks | ebooks | epub, mobi and azw3 together |

Default features are empty. The epub alias currently also enables Office spreadsheet dependencies. The facade's documents feature enables Office/PDF/MOBI/AZW3; the main CLI enables documents by default.

```toml
[dependencies]
omniattach-documents = { version = "0.3.0", features = ["office", "pdf", "ebooks"] }
```

## API

```rust,no_run
use omniattach_documents::{extract, Options};
fn main() -> omniattach_documents::Result<()> {
    let doc = extract("notes.md", &Options::default())?;
    println!("{}", doc.to_markdown());
    for diagnostic in &doc.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    Ok(())
}
```

Use extract_bytes for named byte buffers, extract_as with an explicit Format for renamed input, and extract_xml_table for a declared XML mapping. detect_format uses filename extensions; individual adapters validate content. Unsupported formats or disabled features return errors, without silently switching to another backend.

Document::to_json retains the canonical model, Document::to_text renders text and table rows, and Table::to_csv exports one table. The library does not write sidecars; callers or the CLI choose exports.

## Ebooks

MOBI/AZW3 readers validate record tables, reject encryption, strip supported extra-data/overlap trailers and bound decompression. HUFF/CDIC decoding checks dictionary indices, recursion and work. KF8 uses a referenced FDST table to validate the whole decoded stream and select the primary flow; secondary-flow omission is diagnosed.

This is not full KF8 reconstruction: skeletons/fragments, hybrid boundaries, navigation, images, fonts, CSS and link targets are not reconstructed. Successful extraction does not guarantee completeness or reading order for every book. See [docs/AZW3.md](docs/AZW3.md) and the [main CLI ebook guide](../omniattach-cli/docs/EBOOKS.md). The raw mobi dependency remains optional; bounded text decoding is implemented locally rather than invoking its unbounded decompression path.

## PDF

A targeted compatibility shim normalizes real-valued TJ word gaps using lopdf's existing integer-spacing heuristic. It avoids inserting literal font-encoded space bytes and preserves small kerning adjustments. It is not full position-aware layout reconstruction. Original input bytes and SHA-256 remain unchanged.

Bookmarks are emitted as headings before page text; named destinations are not fully resolved. A PdfOcr callback may recognize otherwise empty pages; the document crate does not itself launch an OCR binary. The facade can supply its supported external OCR path. Supported embedded JPEG/JP2 assets are retained; other encodings may be skipped with diagnostics.

## XML, provenance and limits

XML requires explicit record/column selectors with namespaces and an explicit repeated-value policy. It is not full XPath. Missing versus empty values, significant numeric strings and CSV null markers need caller policy; consult the XML API and examples/xml-mapping.json.

Source identity includes original name, byte length and SHA-256. IDs are extraction-local identifiers, not stable cache keys. Page/slide/row/column locations are one-based where available; decoded byte spans are zero-based and half-open. Do not interpret decoded spans as original compressed offsets or invent page positions for ebooks. Heading semantics are preserved only where the extractor recovers them; visual styling alone is not a guarantee of heading classification.

Limits bound input, declared expansion and selected model sizes, not hard RSS/CPU/disk use. Third-party parser internals may allocate before model checks. Use resource-limited isolated workers for hostile inputs. See docs/SECURITY.md.

## Build and examples

```sh
cargo test -p omniattach-documents
cargo test -p omniattach-documents --all-features
cargo run -p omniattach-documents --features ebooks --example documents-convert -- book.azw3 markdown
cargo run -p omniattach-documents --features azw3 --example azw3-extract -- book.azw3 json
```

Use a current stable toolchain and repository BUILD.md. Regenerate stale lockfiles with Cargo before --locked checks. Fixture tests and limited real-book comparisons are not universal compatibility certification. Version 0.3.0 adds public Format variants; downstream exhaustive matches may require updates. Serialized schema identifiers retain their existing versions.

Apache-2.0; dependencies retain their licenses. No models, converter binaries, automatic downloads, DRM removal, embeddings or async scheduler are included.
