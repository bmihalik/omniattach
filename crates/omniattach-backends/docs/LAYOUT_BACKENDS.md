# Dedicated MinerU and Marker backends

MinerU and Marker are explicit PDF-to-Markdown adapters with relocatable output packages. The facade now provides with_mineru/with_marker and preserves their packages through export; see ../../omniattach/docs/LAYOUT_BACKENDS.md. Use those dedicated registrations rather than the generic ExternalAdapter, which retains its independent asset-renaming contract. Native PDF defaults are unchanged.

## Explicit invocation profiles

MinerU::default() uses python3 with mineru.parser.parse and ParseResult.save(FileBasedDataWriter). Set tool.program to the interpreter in the installed MinerU environment. This uses the complete SDK-save package, not the budgeted mineru parse command. MinerU::new(MinerUProfile::LegacyPipeline) uses historical mineru -p INPUT -o OUT -b pipeline. Profiles are explicit contracts, not universal version detection or automatic fallback.

Marker::default() invokes marker_single INPUT --output_dir OUT --output_format markdown. Optional Fast/Balanced mode and force_ocr map to corresponding flags; no use_llm flag is enabled. Match these options to the installed version. Configure runtime/model environment with Tool.env. No registration invokes a process or downloads models; conversion invokes upstream code, which may download models or access configured services. Configure offline behavior upstream and use OS isolation as needed.

## Output contract

Exactly one Markdown file must exist under out/, unless output.markdown_path explicitly selects one. All accepted Markdown is validated. Markdown, JSON and common raster image files are copied under result/ with the relative tree intact. Unsupported/debug files (including generated PDFs, SVGs, binaries and logs) are skipped with a count diagnostic. JSON is syntax-validated and retained verbatim, not interpreted as a universal layout schema; JSON-internal references and semantics are not validated.

Markdown inline/reference images and links, HTML img src and anchor href are checked with percent decoding. Missing local assets, output-root escapes, unsafe URI schemes, remote images, srcset and unsupported resource-bearing media tags fail explicitly. Normal HTTP(S)/mailto/tel hyperlinks remain without fetching. This is not HTML/CSS sanitization or arbitrary embedded-resource extraction. Image MIME types follow extensions, not decoding. Sanitize untrusted rendered output downstream.

metadata.layout_package.markdown_asset gives the Markdown base path; sha256 records retained-file hashes. Resolve relative images from that artifact's directory. Generated text/producer JSON do not invent authoritative original page/region provenance. Original input hashes are preserved. The facade adds a separate exported-path mapping and verifies hashes during copying.

Symlinks, nonregular output and nonportable names are rejected. Depth is capped at 32; scan entries at 4 * artifact_count + 32; accepted bytes/count obey artifact limits. These are postprocess acceptance bounds, not live disk/RAM quotas, a race-proof filesystem sandbox or a whole-job CPU deadline. Cancellation is checked during scanning/Markdown processing; subprocess invocation retains existing runner limits.

Injected-runner tests cover command contracts, nested images/JSON, export lifetime, references/HTML, empty/missing/ambiguous output, path rejection, symlinks, budgets, cancellation and process failures. Facade tests cover exported relative links, source identity, permissions and explicit selection. Fixture image bytes validate copying, not image decoding. Run cargo fmt --all and relevant backend/facade tests, plus workspace checks and Clippy. Rust compilation and real MinerU/Marker inference have not been run here. Cargo must refresh Cargo.lock for parser dependencies; it is not hand-edited. No version/schema release bump, merge or publication is included.
