# omniattach-documents

A standalone Rust library for **local document extraction into AI-friendly, structured data**.

The canonical `Document` contains typed blocks, tables, diagnostics and source locations. Markdown, plain text, JSON and CSV are projections of that model. Conversion does not write sidecars, fetch URLs, execute scripts, run Office macros or download models.

This is an implemented **0.1 baseline**, not the whole planned omniattach ecosystem. It does not yet depend on an unpublished `omniattach-core` crate. Public model types live here until that interface is stabilized.

## Formats

| Input | Cargo feature | Extraction | Qualification |
|---|---|---|---|
| Text, logs, configuration, YAML/TOML | None | Line blocks | YAML/TOML remain text, not parsed data |
| Common source extensions | None | Code block with language | No symbol graph or language-aware analysis |
| Markdown | None | Headings, paragraphs, code, lists, quotes, tables | Semantic extraction; inline styling/link destinations/list nesting are lossy |
| HTML/XHTML | None | Headings, paragraphs, code, lists, tables | No scripts/styles or resource loading; CSS layout not interpreted |
| CSV/TSV | None | Tables, quoted fields, ragged rows | Header and delimiter policy configurable |
| XML | None | Explicit record-to-table mapping | Namespace-aware restricted selectors, not full XPath |
| JSON/JSONL | None | Validated original text as code | Retains lexical numbers; no automatic flattening |
| DOCX | `office` | Main body paragraphs, direct headings, tables | No inherited style resolution or page layout |
| PPTX | `office` | Slide text, presentation relationship order | No rendering, spatial reading order or speaker notes |
| ODT/ODP | `office` | Text, headings, basic tables, slide locations | No rendering/assets; repeated spaces simplified |
| ODS | `office` | Typed/lexical values, formulas, bounded repetition | No recalculation, styles or merges |
| XLSX/XLSM/XLSB/XLS | `office` | Calamine values and cached formulas | Parser-normalized values, not original numeric lexemes |
| EPUB | `office` | Spine-ordered chapter text/tables | No DRM or image extraction |
| PDF | `pdf` | Page-located text | **Not** OCR, layout-aware Markdown or table recognition |

XLSX, ODS, DOCX, PPTX, ODT, ODP, EPUB and PDF have executable minimal fixtures in the tests. XLS/XLSB/XLSM are routed through Calamine but were **not independently validated against real exported files** in this release. These are declared adapter routes, not a broad compatibility certification.

Audio, video, OCR, legacy DOC/PPT conversion, LibreOffice invocation, image assets, live feeds, async job scheduling and embeddings are outside this crate.

## Build

```sh
cargo test --locked
cargo test --locked --all-features
cargo clippy --locked --all-features --all-targets -- -D warnings
```

Default features are empty. Enable optional document families explicitly:

```toml
[dependencies]
omniattach-documents = { path = "../omniattach-documents", features = ["office", "pdf"] }
```

Use a current stable Rust toolchain. The exact tested toolchain and feature matrix are recorded in `docs/TESTING.md`. No minimum supported Rust version is claimed. The lockfile is included; dependencies are not vendored, so a first build needs registry access or an existing Cargo cache.

## Rust API

```rust
use omniattach_documents::{extract, Options};

fn main() -> omniattach_documents::Result<()> {
    let document = extract("report.docx", &Options::default())?;
    println!("{}", document.to_markdown());
    for diagnostic in &document.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    Ok(())
}
```

Entry points:

| API | Purpose |
|---|---|
| `extract(path, &options)` | Read a local file, without creating sidecars |
| `extract_bytes(name, bytes, &options)` | Route using the supplied extension |
| `extract_as(name, bytes, Format::..., &options)` | Explicit format for extensionless or renamed input |
| `extract_xml_table(name, bytes, spec, &options)` | Explicit XML mapping |
| `Document::to_markdown()` | Semantic GFM-style rendering |
| `Document::to_text()` | Plain text with tab-delimited table rows |
| `Document::to_json()` | Canonical data and diagnostics |
| `Table::to_csv[_with]()` | One selected table per CSV |

`detect_format` uses extensions, not comprehensive MIME sniffing. The Office readers validate ZIP structure. Unsupported formats and disabled features return explicit errors; there is no silent backend downgrade.

## XML to CSV

```rust
use omniattach_documents::{extract_xml_table, Options, Repeated, XmlColumn, XmlTableSpec};

fn main() -> omniattach_documents::Result<()> {
    let spec = XmlTableSpec::new("/m:measurements/m:sample", vec![
        XmlColumn::new("id", "@id"),
        XmlColumn::new("value", "m:value"),
        XmlColumn::new("unit", "m:value/@unit"),
        XmlColumn::new("tags", "m:tag").repeated(Repeated::Json),
    ]).namespace("m", "urn:measurement");
    let xml = br#"<measurements xmlns="urn:measurement"><sample id="001"><value unit="ppm">1.2300</value><tag>control</tag></sample></measurements>"#;
    let doc = extract_xml_table("measurements.xml", xml, spec, &Options::default())?;
    println!("{}", doc.tables[0].to_csv()?);
    Ok(())
}
```

Selectors support `/prefix:root/prefix:record`, relative child paths, terminal `@attr` and `.` for record text. Unprefixed names match **no namespace**, including attributes. Register a prefix to address a default XML namespace. No predicates, wildcards, descendant axes or automatic row explosion.

Repeated selections default to an error. Select `Repeated::Join(separator)` or `Repeated::Json` explicitly. Missing values are `raw: None`; an empty element is `raw: Some("")`. XML numeric strings remain strings, including significant zeros. CSV's default empty null marker collapses missing/empty; set `CsvOptions::null` to a reserved marker when needed. Marker collisions must be managed by the caller.

The mapping example is also available as `examples/xml-mapping.json`.

## Command-line examples

A compact example program is included, not a separately installed CLI:

```sh
cargo run --locked --example convert -- report.md markdown
cargo run --locked --features office --example convert -- report.odt json
cargo run --locked --features pdf --example convert -- report.pdf markdown
cargo run --locked --example convert -- tests/data/measurements.xml csv examples/xml-mapping.json
cargo run --locked --example xml_table
```

The example prints conversion output to stdout and diagnostics to stderr. For multi-sheet files, use the library to export every selected table; the example's `csv` mode exports the first table only.

## Provenance and fidelity

- Source identity: SHA-256 of original bytes, original source name/length and decoded text encoding where applicable.
- Block/table IDs: content-hash prefix plus extraction-local ordinal. They are not stable across schema/parser/option changes and do not constitute cache keys.
- Page/slide/row/column and line positions are one-based. Decoded UTF-8 byte ranges are zero-based, half-open.
- Decoded offsets refer to the decoded original input or named XML package member, **not** original encoded byte offsets or rendered Markdown offsets.
- XML table cell paths identify the mapped record/selector; the byte span covers that record, not necessarily the individual selected child.
- CSV physical line start is retained, but line end is not synthesized. `row` counts CSV records, not physical lines.
- PDF text has page locations, not inferred headings/bounding boxes. HTML has member-level provenance only; no fabricated line/byte locations.
- Workbook numeric display may be normalized by Calamine, including precision loss from its floating-point parsing. Do not use this route for forensic lexical numeric fidelity; ODS native extraction keeps the available original lexical value separately.
- Formula text and cached values are separate. Formula cells without cached values remain present. No formula engine is invoked.
- Diagnostics record the main quality losses; this release does not preserve every format-specific annotation.

## Operational boundary

The API is synchronous and materializes a document in memory. `Limits` bound input bytes, declared archive expansion, archive entries, XML node count and output block/cell counts; ODS repetitions are checked before expanding nonempty ranges. They are **not a hard process memory/CPU sandbox**. Workbook/PDF/HTML parser internals may allocate before post-parse checks. Use an isolated resource-limited worker for hostile files. See `docs/SECURITY.md`.

UTF-8, UTF BOMs supported by encoding_rs, and explicitly configured encoding_rs labels are accepted. Invalid sequences fail rather than silently replacing characters. UTF-32 is explicitly unsupported. XML without a BOM must use UTF-8; XML encoding declarations are not used to guess an encoding.

## Layout

```text
src/               public API, canonical model, format adapters, renderers
examples/          conversion program, XML Rust example and JSON mapping
tests/            parser integration tests and generated package fixtures
docs/             architecture, security, testing and RagDBMan integration
.github/workflows/ feature-matrix CI
```

## License

Apache-2.0 for this crate. Dependencies retain their own licenses. No RagDBMan Python source is embedded; its public document/extraction boundaries informed the adapter design. This release neither changes RagDBMan nor publishes anything to crates.io.
