# Verification results

Environment: Linux x86_64, Rust/Cargo 1.99.1. The isolated verification manifest compiles the actual ../src files and ../tests/facade.rs with real crates.io dependencies and no sibling stand-ins.

| Command/check | Observed |
|---|---|
| Isolated `cargo test --locked` | 39 integration tests passed, one compiled rustdoc example passed |
| Isolated `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Isolated `cargo fmt -- --check` | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps` | Passed |
| Portable convert example with Markdown export | Executed, manifest/artifact checked |
| Custom DocumentAdapter example with text input | Executed, payload JSON checked |
| NDJSON observer output | Staging/converting/completed order checked |
| Extracted tar archive portable harness | Rebuilt/tested using supplied verification lockfile |

Coverage includes explicit/ambiguous/unknown route selection, policy gating, no fallback, source hashes, private staging, concurrent jobs, UTF-8/UTF-16/BOM handling, strict encoding failures, text/Markdown preservation, size/count limits, traversal/symlink rejection, declared-only export, cleanup, cancellation/deadline, progress, document payload preservation and explicit two-step pipelines.

Not executed: main workspace manifest build (actual siblings unavailable), `--features core/documents/backends/workspace`, three optional backend bridge tests, external process execution through the bridge, real document parsing, XML/table conversion, model inference or Windows/macOS runs. No native-tool verification claim from the preceding separate backend release is transferred to this facade release.

The independent Cargo.lock is in verification/. Cargo's main facade manifest still requires the real sibling manifests to resolve optional path dependencies. Use only the main manifest for all-features compatibility testing. Do not enable sibling features on the independent harness; it intentionally has no replacement dependencies.
