# Dedicated MinerU and Marker backends

This commit adds backend-level adapters and relocatable packages. Facade registration and path-preserving facade copying are a separate following change; the old generic ExternalAdapter still renames assets independently and must not yet be used to promise working Markdown image links. Native PDF defaults are unchanged.

## Explicit invocation profiles

- MinerU::default() uses python3 with the documented mineru.parser.parse / ParseResult.save(FileBasedDataWriter) API. Set tool.program to the Python interpreter in the installed MinerU environment. This profile exports complete Markdown, JSON and images; it does not use the budgeted mineru parse command, which currently defaults to the first 10 PDF pages.
- MinerU::new(MinerUProfile::LegacyPipeline) uses the historical mineru -p INPUT -o OUT -b pipeline CLI. It is not a compatibility alias for modern MinerU commands.
- Marker::default() invokes marker_single INPUT --output_dir OUT --output_format markdown. Optional mode Fast/Balanced and force_ocr map to the corresponding flags. No use_llm flag is enabled.

Profiles are explicit contracts, not universal version detection. No registration invokes a tool, probes a GPU, installs models or triggers inference. Conversion starts an external process; upstream tools may download models, start inference servers or access configured network services. Tool.env supplies the runtime/model settings retained by the process runner; configure local/offline behavior upstream and use OS isolation if needed. Input support is PDF-only in this initial integration.

## Output and assets

Exactly one Markdown file must exist under out/, unless output.markdown_path explicitly selects one relative path. Every accepted Markdown file is validated. The packager retains Markdown, JSON and common raster image files under result/, preserving the entire relative output tree; unsupported/debug files are skipped with a count diagnostic. It does not copy generated PDFs, SVGs, arbitrary binaries or logs.

Markdown inline/reference images and links, plus HTML img src and anchor href, are checked after percent decoding. Local references must resolve to retained files inside the package; missing files, escape paths, unsafe URI schemes, remote images and unsupported srcset/media tags fail explicitly. Normal HTTP(S)/mailto/tel hyperlinks remain without network fetching. HTML/CSS sanitization and arbitrary embedded-resource interpretation are not provided; do not directly render untrusted Markdown/JSON without frontend sanitization. Image MIME types follow file extensions, not an image decoder.

metadata.layout_package.markdown_asset identifies the base Markdown artifact. Relative image references are meaningful relative to that file, not the working directory or the root of concatenated text. metadata.layout_package.sha256 records retained-file hashes. Existing source SHA-256 remains the copied input identity. Producer JSON is syntax-validated and retained verbatim, not mapped into fabricated universal page/region types; JSON-internal references and producer schema semantics are not independently validated.

Symlink/nonregular outputs and nonportable names are rejected. Scan depth is capped at 32; scan entries are bounded by 4 * artifact_count + 32. Accepted bytes/count obey existing artifact limits. These are postprocess acceptance limits, not live disk/RAM quotas, a race-proof filesystem sandbox or a whole-job CPU deadline. Cancellation is checked while scanning and processing Markdown.

## Validation

Injected Runner tests cover command profiles, nested images, JSON preservation, export ownership, reference/HTML validation, missing/ambiguous/empty outputs, unsafe paths, symlinks, budget failures, cancellation and process-error propagation. The fake image bytes test byte preservation, not image decoding.

Run cargo fmt --all; cargo test -p omniattach-backends --test layout_backends; cargo clippy -p omniattach-backends --all-targets -- -D warnings. Let Cargo refresh Cargo.lock for the Markdown/HTML/percent-decoding parser dependencies and review its diff. No lockfile is hand-edited. Rust compilation and real MinerU/Marker inference have not been run in the preparation environment; fixture tests do not certify installed-tool compatibility. No package/schema release version is bumped and nothing is published.
