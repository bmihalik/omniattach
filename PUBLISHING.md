# Publishing to crates.io

The repository root is a virtual Cargo workspace, not a package. Publish its five member packages individually. Package versions are independent:

| Package | Version |
|---|---|
| omniattach-core | 0.1.0 |
| omniattach-documents | 0.2.0 |
| omniattach-backends | 0.1.0 |
| omniattach | 0.2.0 |
| omniattach-cli | 0.2.0 |

Internal dependencies specify both a local path and a registry version. Keep these requirements consistent with the member package versions when releasing. The isolated verification harnesses are not release packages.

## Validate locally

Use a current stable Rust toolchain. Run from the repository root:

```sh
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Review and commit any Cargo.lock changes produced by Cargo. Do not regenerate the lockfile by hand. Inspect each release archive, including README, licensing material, source files and test fixtures:

```sh
cargo package -p omniattach-core --list
cargo package -p omniattach-documents --list
cargo package -p omniattach-backends --list
cargo package -p omniattach --list
cargo package -p omniattach-cli --list
```

Before upload, confirm package names are available to your crates.io account and review public documentation for outdated claims. Passing workspace tests is not a substitute for verifying the packaged crates. Runtime external tools and model files are not installed by Cargo; test their integrations separately where applicable.

## Authenticate

Authenticate with crates.io using cargo login or a securely supplied registry token. Never commit a token to the repository.

## Publish in dependency order

For each package below, run the dry run first, inspect its result, then deliberately execute the upload command. The first three packages are independent; the facade depends on them, and the CLI depends on the facade.

```sh
cargo publish -p omniattach-core --dry-run
cargo publish -p omniattach-core

cargo publish -p omniattach-documents --dry-run
cargo publish -p omniattach-documents

cargo publish -p omniattach-backends --dry-run
cargo publish -p omniattach-backends

cargo publish -p omniattach --dry-run
cargo publish -p omniattach

cargo publish -p omniattach-cli --dry-run
cargo publish -p omniattach-cli
```

Wait for each dependency version to be available in the registry before checking or publishing its dependents. On the first release, downstream dry runs may fail until their dependencies are published. Do not bypass package verification with --no-verify merely to force an upload. Published versions cannot be overwritten; fix problems before uploading.

## Verify the installed CLI

After publication, test installation outside this workspace:

```sh
cargo install omniattach-cli --version 0.2.0 --features workspace
omniattach --version
omniattach capabilities --pretty
```

The package name is omniattach-cli; the executable name is omniattach. External routes still require runtime registration and explicit execution permission.

This guide does not certify that release validation has run, and adding it does not publish any package.
