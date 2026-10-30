# omniattach

Version 0.3.0. Explicit local conversion orchestration for the omniattach workspace: route selection, staged source identity, owned temporary bundles, diagnostics and durable exports.

## Features

| Feature | Purpose |
|---|---|
| Default: none | Portable facade, without optional sibling integrations |
| core | Re-export shared core types |
| documents | Enable Office/PDF/EPUB/MOBI/AZW3 extraction and document defaults |
| backends | Enable adapters for installed subprocess tools |
| workspace | All optional integrations |

Enabling a feature makes integration code available; library callers still register routes using the builder. The CLI registers document defaults when built with its documents feature.

```toml
[dependencies]
omniattach = { version = "0.3.0", features = ["documents"] }
```

## Document conversion

```rust,no_run
use omniattach::{ConversionRequest, FacadeContext, Goal, OmniAttach};
fn main() -> omniattach::Result<()> {
    let engine = OmniAttach::builder()
        .with_portable_defaults()?
        .with_document_defaults()?
        .build();
    let bundle = engine.convert(
        &ConversionRequest::new("book.azw3", Goal::Markdown),
        &FacadeContext::default(),
    )?;
    bundle.export("new-book-export")?;
    Ok(())
}
```

This example requires the documents feature. Without that feature, use `OmniAttach::new()` for portable text/Markdown conversion. The builder starts empty; explicitly add defaults or custom converters.

Routes declare formats, goals and execution kind. Ambiguity is an error; insertion order does not select a winner. Explicit routes must match the request and policy. No fallback is attempted after conversion failure. Capabilities describe registered contracts, not installed-tool health. Planning is advisory; conversion stages and checks actual input bytes.

## Ebook scope

The documents route accepts Format::Mobi and Format::Azw3, with the existing extract, markdown and tables goals. A tables goal exports only tables actually recovered; it does not infer tables from layout. Ebook diagnostics are forwarded to facade warnings, and the canonical document is stored as a payload and document.json artifact. Source identity refers to original staged input, not reconstructed HTML.

HUFF/CDIC and record trailers are supported with bounded decoding. KF8 FDST validation selects the primary flow and reports skipped secondary flows. Full KF8 skeleton/fragment reconstruction, navigation, layout and asset preservation are not claimed. DRM/encrypted inputs fail. For extensionless input, use an explicit format override.

## Runtime and export

External routes require both registration and explicit permission. No process starts merely because a backend is registered. An external declaration is not a hostile-code sandbox. Deadlines and cancellation are cooperative; arbitrary in-process work and filesystem operations cannot be forcibly interrupted by the facade.

Keep a bundle alive while using temporary artifact paths, or export it to a new directory. Exported manifests and declared artifact paths are the durable interface. Nested backend payload paths can refer to original staging locations; consume top-level facade artifacts for exported files. Export is not a hard transactional filesystem guarantee.

Input/export budgets are not live disk quotas or process memory limits. Use OS isolation for untrusted input. See docs/SECURITY.md, the repository BUILD.md and the CLI ebook guide.

## Versioning and validation

0.3.0 adds public format variants and document routing compared with 0.2.x. Downstream exhaustive matches may need new arms. Package versions do not automatically change serialized schema identifiers, which retain their existing values.

```sh
cargo test -p omniattach
cargo test -p omniattach --features workspace
cargo clippy -p omniattach --all-features --all-targets -- -D warnings
```

The excluded verification manifest is a standalone portable harness with its own version and lockfile; it is not a release package. It does not certify optional integrations. Earlier archive-delivery notes and fixed historical test counts are not evidence of validation for the current tree. Apache-2.0.
