# Publishing to crates.io

The root is a virtual workspace, not a package. Release package versions are independent:

| Package | Target version |
|---|---|
| omniattach-core | 0.1.0 (unchanged) |
| omniattach-documents | 0.3.0 |
| omniattach-backends | 0.1.0 (unchanged) |
| omniattach | 0.3.0 |
| omniattach-cli | 0.3.0 |

0.3.0 introduces ebook formats/routes and public enum variants; downstream exhaustive matches may need migration. Keep each internal path dependency's registry requirement aligned with the intended release. The facade and CLI require the 0.3.0 dependency line, not the older 0.2.x packages lacking these features. Schema identifiers are versioned separately and are not automatically bumped.

## Prepare locally

Use current stable Rust. Let Cargo update the root Cargo.lock through normal resolution; review and commit that diff before locked release checks. This manifest/documentation change does not regenerate the lockfile. Excluded verification harnesses have their own package versions and lockfiles and are not published as workspace releases.

```sh
cargo check --workspace --all-features --all-targets
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

After the root lockfile is current, repeat checks using --locked. Review package file lists, README/license inclusion and accidental fixtures or secrets:

```sh
cargo package -p omniattach-documents --list
cargo package -p omniattach --list
cargo package -p omniattach-cli --list
```

Check registry ownership, name availability and already-published versions. Never attempt to republish an existing version. Keep tokens out of source and logs; authenticate with cargo login or a securely supplied registry credential.

## Dry run and upload

A workspace-wide cargo publish --dry-run worked with the maintainer's Cargo and used a temporary registry for interdependent packages. Older Cargo versions or per-package dry runs may require dependencies to exist in crates.io first. Follow actual Cargo errors; do not bypass verification with --no-verify.

```sh
cargo publish --dry-run
```

Only after explicit local review, publish in dependency order:

```sh
cargo publish -p omniattach-documents
cargo publish -p omniattach
cargo publish -p omniattach-cli
```

The unchanged core/backend versions must already be available to the publishing account/registry. If they have never been published, verify and publish omniattach-core 0.1.0 and omniattach-backends 0.1.0 first; otherwise skip them. Wait for registry availability before dependent uploads. A dry run does not reserve names or guarantee permission to publish.

## Verify installed CLI

After publication, outside the checkout:

```sh
cargo install omniattach-cli --version 0.3.0
omniattach --version
omniattach capabilities --pretty
```

Add --features workspace when external backend adapters are wanted. The package is omniattach-cli; the binary is omniattach. Ebook routes need no external registration or execution permission. Confirm diagnostics and output on authorized representative files; no full KF8 fidelity claim is made.

No command in this guide is evidence it has run. Version/documentation commits do not create tags, GitHub releases or crates.io uploads. Coordinate release tags only after validation.
