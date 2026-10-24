#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --manifest-path verification/Cargo.toml -- --check
cargo test --manifest-path verification/Cargo.toml --locked
cargo clippy --manifest-path verification/Cargo.toml --locked --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path verification/Cargo.toml --locked --no-deps
