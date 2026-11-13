# MinerU and Marker facade integration

Enable omniattach's backends feature. Register configured adapters with OmniAttach::builder().with_mineru(backends::MinerU::default())? or .with_marker(backends::Marker::default())?. Runner-injecting variants are provided for tests/services. Registration does not probe, install or start tools. with_external_defaults and native PDF defaults do not include these model-based routes.

The mineru and marker routes accept PDF with Extract, Markdown, PdfLayout or PdfScientific goals. Goal names do not certify model accuracy. They do not produce canonical Table objects. Select .with_route("mineru") or .with_route("marker") and set FacadeContext::default().with_external(true). Multiple permitted routes remain an ambiguity error; no automatic fallback or region-level hybrid dispatch is added. Configure all model/tool options on the backend instance. Nonempty request.options, including dehyphenation, are rejected.

Unlike generic ExternalAdapter, the dedicated adapter copies each result/path to layout/result/path. It retains Markdown/JSON/image bytes and relative paths, validates transferred lengths and SHA-256 hashes, and keeps files alive after the backend workspace drops. It leaves the existing generic adapter and legacy Office exports unchanged.

The omniattach/layout-package/1 payload provides markdown_artifact and backend-to-export artifact mappings with hashes. Generated Markdown TextFragment.location also identifies its base artifact. Resolve image links relative to that Markdown file, not the export root or concatenated plain_text(). The nested omniattach-backends/0.1 payload remains backend-relative; the new mapping is the facade file interface. Source identity at the facade level refers to the original input.

Use the example without a custom wrapper:

```sh
cargo run -p omniattach --features backends --example layout_pdf -- mineru-sdk book.pdf new-export /opt/mineru/bin/python
cargo run -p omniattach --features backends --example layout_pdf -- mineru-legacy book.pdf legacy-export /opt/mineru/bin/mineru
cargo run -p omniattach --features backends --example layout_pdf -- marker book.pdf marker-export /opt/marker/bin/marker_single
```

This phase exposes library registration and a runnable example, not new stock omniattach-cli registration flags. Install the matching upstream package/models yourself. The current SDK and historical CLI contracts are distinct; see the backend guide. Upstream processes may download models or use configured network services. No real-tool/GPU compatibility or layout quality is certified. Producer JSON is opaque, not a normalized universal region schema.

Validate with cargo fmt --all; cargo test -p omniattach-backends --test layout_backends; cargo test -p omniattach --features backends --test layout_backends; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets. Let Cargo refresh the lockfile and review it. Rust tests/inference have not been run here; no merge, version bump or publication is included.
