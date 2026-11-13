# LFE-STEP-3: operator regressions and real-export audit

This increment adds validation, not another image decoder or layout heuristic. Six generated-PDF tests exercise actual Tj/TJ operators (including real and integer gap operands), small kerning, Tm/Td/T* line positioning, paragraph gaps, comparison with ordinary text extraction, and supported/unsupported image boundaries. This supplements rather than replaces the assembler's synthetic inspection-element tests and existing Form/raster/CLI tests.

```sh
cargo fmt --all
cargo test -p omniattach-documents --features pdf --test pdf_mixed_text
cargo test -p omniattach-documents --features pdf
cargo test -p omniattach-cli --test pdf_images
cargo clippy --workspace --all-features --all-targets -- -D warnings
```

For a real PDF, create both exports from the same source bytes (new directories):

```sh
cargo run -p omniattach-cli -- convert book.pdf --goal markdown --output plain-export
cargo run -p omniattach-cli -- convert book.pdf --goal custom:pdf-images --route pdf-images --output image-export
python3 scripts/audit_pdf_image_export.py image-export --reference plain-export --report image-audit.json
```

The Python 3.10+ stdlib utility checks contained non-symlink artifact paths, manifest file sizes, saved and JSON-embedded image bytes/hashes, image artifact metadata, typed image-block references and Markdown image occurrence counts. It checks source hash agreement and reports diagnostic-code counts. It reads no original PDF and invokes no models, renderers or external commands. An all-skipped/zero-image export can pass structural checks; diagnostic codes and counts must still be reviewed for coverage. The Markdown parser targets generated inline image links, not arbitrary Markdown syntax.

Optional reference comparison reports paragraph-only whitespace-normalized and alphanumeric sequence hashes/counts. It excludes heading/bookmark blocks and generic image labels. Prose differences do not alone fail the structural audit: canonical furniture removal and normalization can explain them. Different source bytes do fail. Exact alphanumeric equality ignores punctuation/whitespace but preserves case and sequence; it is sample-specific evidence, not proof of semantic fidelity or visual reading order. No extracted prose is printed, and failure messages are deliberately generic. Source hashes, counts and diagnostic codes remain potentially identifying metadata.

JSON/Markdown read limits and total image-read budgets are configurable. These are read bounds, not a peak-memory guarantee: JSON arrays expand in memory. The utility is for trusted local exports and is not a general hostile-input sandbox or a JPEG/JP2/PNG codec validator. Reports use exclusive creation and never overwrite an existing report file. Report files are explicitly user-requested outputs; the utility does not modify exports.

Before considering the supported STEP-3 subset ready for review, validate ordinary prose continuity and image link placement on representative mixed documents, inspect skipped-image diagnostics, and open exported images. Record the source hash and structural report rather than committing private book content. Shared signed-metric line-builder refactoring, predictors/masks and other unsupported formats remain separate follow-up scope; none is silently claimed here.

Neither the new Rust regression target nor the Python audit utility has been executed in the preparation environment. No extraction/API/schema/route/dependency/version changes, merge or publication are included.
