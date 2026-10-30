# MOBI and AZW3 in the main CLI

MOBI and experimental AZW3 use the in-process documents route. The facade documents feature enables both ebook library features, and the CLI enables documents by default. No LibreOffice, Calibre, external-backend registration or external permission is required.

```sh
cargo build -p omniattach-cli
./target/debug/omniattach probe book.azw3 --pretty
./target/debug/omniattach plan book.azw3 --goal markdown --pretty
./target/debug/omniattach convert book.azw3 --goal markdown --output book-export
./target/debug/omniattach convert book.mobi --goal markdown --output mobi-export
```

Output directories must be new; their parent must exist. Markdown export includes document.md, document.json and manifest.json. Default stdout is the existing CLI JSON result envelope, not raw Markdown. To put extracted plain text on stdout while retaining a durable export:

```sh
./target/debug/omniattach convert book.azw3 --output text-export --stdout text
```

For renamed or extensionless input, explicitly select the format:

```sh
./target/debug/omniattach convert renamed.bin --format azw3 --goal markdown --output renamed-export
```

MOBI/AZW3 detection is extension-hinted, not complete Palm database identification or validation. Probe does not check installed routes or parse the ebook. Plan is advisory; conversion performs actual validation and extraction. Builds without documents do not register the ebook route. The facade's documents feature now enables ebook parsing as well as its existing Office/PDF parsing.

## Fidelity and errors

The main CLI calls the same document extraction API as azw3-extract. Existing HUFF/CDIC decoding, trailer handling, FDST validation, primary-flow selection, source identity and limits are retained. The facade forwards document diagnostics into its warnings and stores the document JSON unchanged as an artifact/payload.

AZW3 remains experimental raw-text/HTML projection, not full KF8 skeleton/fragment reconstruction. Completeness and reading order are not universally guaranteed. Secondary flows, styling and assets are not reconstructed. DRM/encrypted files and malformed containers fail; enabling external execution does not bypass those checks. Converter errors use the existing CONVERSION_FAILED mapping. This change adds no new schema or exit code, and does not publish or bump versions.

## Validate

```sh
cargo fmt --all
cargo test -p omniattach --features documents
cargo test -p omniattach-cli --test ebooks
cargo test -p omniattach-cli --features workspace
cargo check --workspace --all-features --all-targets
```

The new CLI tests use generated uncompressed MOBI/AZW3 records and exercise probing, planning, exports, explicit overrides and rejection paths. HUFF/CDIC and FDST regression tests remain in omniattach-documents. The assistant has not compiled or run these changes; test real books locally. Earlier ebook documentation describing MOBI/AZW3 as example-only is superseded by this main-CLI integration guide.
