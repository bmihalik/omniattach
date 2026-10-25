
This guide explains how to build the application, run the different kinds of tests, and format the sources. Run all commands from the repository root.

## 1. Prerequisites

The workspace requires Rust 1.85 or newer (edition 2024).

```bash
rustup update stable
rustup component add rustfmt clippy
```

The `sqlite3` CLI is optional but useful for inspecting the database.

## 2. Get the Latest Sources

```bash
git clone https://github.com/bmihalik/omniattach.git         # first time
cd omniattach
git pull --ff-only origin main                               # existing checkout
```

`--ff-only` avoids accidental merge commits.

## 3. Build

```bash
cargo build            # debug build
cargo build --release  # optimized build
```

Binaries are written to `target/debug/` or `target/release/`.

## 4. Testing

OMNIATTACH_SCANNED_PDF=/path/to/scan.pdf OMNIATTACH_EXPECTED_OCR_TEXT=word \
  cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored

cargo test --workspace --all-features


## 5. Formatting

```bash
cargo fmt --all                # rewrite files in place
cargo fmt --all -- --check     # verify only; non-zero exit on differences
```

Review changes with `git diff` before committing. Rustfmt reads `rustfmt.toml` or `.rustfmt.toml` from the repository (or parent directories); without one it uses defaults. Commit a repository-level `rustfmt.toml` for shared project rules.

## 6. Full Validation Before Pushing

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test
```

## 7. Cleanup

```bash
cargo clean
`

