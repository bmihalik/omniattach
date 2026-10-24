# Workspace integration checklist

1. Extract at the repository root; archive writes only `crates/omniattach/`.
2. Keep existing `omniattach-core`, `omniattach-documents`, `omniattach-backends` and root Cargo.toml intact.
3. Ensure the root workspace includes `crates/omniattach`; existing `crates/*` is sufficient.
4. Run `cargo test -p omniattach` to compile portable support against your actual workspace manifests.
5. Enable `core`/`documents` features for namespaced re-exports; connect the actual extraction API with DocumentAdapter or Converter.
6. Run `cargo test -p omniattach --features backends` to compile the optional backend bridge and its three actual-API regression tests.
7. Resolve any real sibling API differences explicitly in `src/backend_adapter.rs`; do not replace sibling crates with mocks to make the build pass.
8. Run `cargo test -p omniattach --features workspace` and warning-denying lint/doc checks on your real workspace.
9. Only then enable those features by default in the application/CLI crate. Register actual tools and allow external execution explicitly.

## Root manifest example

Merge the member entry into your existing manifest, rather than copying over it:

```toml
[workspace]
resolver = "3"
members = [
    "crates/omniattach-core",
    "crates/omniattach-documents",
    "crates/omniattach-backends",
    "crates/omniattach",
]
```

Existing workspace metadata and shared dependencies remain unchanged. This release's facade uses direct sibling `path` dependencies; it does not require their dependency aliases to exist in the root table. The root owns dependency resolution and its Cargo.lock.

## Optional backend API assumptions

The bridge references these names from the preceding backend delivery:

- `Backend::convert(&Path, &BackendContext) -> Result<ArtifactBundle>` and concrete Poppler/Tesseract/LibreOffice/FFmpeg/FFprobe/WhisperCpp types.
- `BackendContext::new(&dyn Runner)`, run_options cancellation/timeout, limits and scratch_parent.
- ArtifactBundle's public manifest and `asset_path(index)`; Manifest text/diagnostics/assets fields.
- Backend CancellationToken Clone/cancel/is_cancelled.

These signatures are described in the conversation but were not re-obtained as source attachments in this turn, so their integration compilation is explicitly unverified. `tests/backend_bridge.rs` runs only with the main manifest and `backends` feature; the portable harness neither compiles it nor fabricates replacements.

## Core consolidation

The attached layout lists core conversion models, XML specs, progress traits, backend traits and a planner. Their exact signatures were not supplied. Therefore this release deliberately does not `pub use core::*`, implement an uninspected core trait, or pretend FacadeBundle is core::ConversionBundle. The namespace `omniattach::core` exposes the actual crate unchanged once enabled.

After actual source review, replace facade-owned envelope/routing/progress internals with the real core equivalents where appropriate, preserving explicit routes, provenance and owned result lifetimes. This is outstanding integration work; it is not a feature already tested in this archive.
