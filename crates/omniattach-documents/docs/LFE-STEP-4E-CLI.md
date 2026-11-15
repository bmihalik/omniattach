# STEP4 main CLI / facade integration

This is the existing native-image explicit-selection feature, not an automatic
equation detector. Build CLI with `--features workspace` (documents + backends).
Facade registration is explicit:

```rust,ignore
let engine = OmniAttach::builder()
    .with_pdf_equation_worker(config)?
    .build();
// Goal::Custom("pdf-equations"), options {"block_ids":[...]}; allow_external=true.
```

The main CLI reads a trusted startup configuration path from
OMNIATTACH_PDF_EQUATION_WORKER_CONFIG ONLY with --external-backends. This uses the
existing custom-goal/options interface; it adds no Goal variant or request-based
executable configuration. Registration/capabilities do not execute/probe the worker.
Ordinary extract/markdown route selection is unchanged. Unset the environment
variable when not using that registration. The configured worker route always
requires --allow-external, including an empty-selection listing/export.

Worker config is a separate JSON object (not the multi-backend comparison config):

```json
{"argv":["/absolute/lfe-formula-bobine","/absolute/encoder.onnx","/absolute/decoder.onnx","/absolute/tokenizer.json","512"],"env":{"ORT_DYLIB_PATH":"/absolute/libonnxruntime.so"},"timeout_ms":600000,"artifacts":[{"path":"/absolute/lfe-formula-bobine","sha256":"ACTUAL_SHA256"}]}
```

Declare the executable AND all model/tokenizer/runtime files in artifacts, each
with its real SHA-256. The example shows only the executable entry; add the other
entries before use. Hashes are checked at conversion, not registration. Rebuild
the native worker and refresh its executable hash: it now accepts --request-file
PATH in addition to its unchanged stdin protocol. The facade appends that flag;
custom workers must implement this file-input contract. argv is never shell text.
Paths must be absolute; config is trusted executable configuration. The runner
clears most inherited environment: declare ORT_DYLIB_PATH explicitly in env.

```bash
cargo fmt --all
cargo test --locked -p omniattach --features workspace --test pdf_equations
cargo test --locked -p omniattach-cli --features workspace --test pdf_equations
cargo test --locked -p omniattach-documents --features pdf --test pdf_equations
cargo build --release --manifest-path tools/lfe-formula-bobine/Cargo.toml
cargo build -p omniattach-cli --features workspace
export OMNIATTACH_PDF_EQUATION_WORKER_CONFIG=/absolute/worker.json
./target/debug/omniattach --external-backends convert INPUT.pdf \
  --goal custom:pdf-equations --allow-external \
  --options-json '{"block_ids":["ACTUAL_DOCUMENT_HASH:b2"]}' --output NEW_OUTPUT
```

To discover block IDs without inference, use block_ids:[] and inspect exported
wrapper document.json -> document.blocks. Never select every image automatically.

One worker batch handles eligible selections, so the model loads once per document.
The existing ProcessRunner enforces cancellation, timeout, bounded stdout/stderr
and Unix process-group cleanup; no stdin support change is made to that runner.
Non-Unix users need an injected compatible runner. Per-image errors/empty outputs
keep image fallback. Batch failures/malformed IDs fail conversion rather than
silently succeeding; timeout/cancel/limit errors retain their facade mappings.

The route reuses the native-image exporter, replaces only job-owned document files,
and registers document.md, wrapper document.json, preserved images and
 equation-worker.json in the normal durable facade manifest. Payloads identify the
equation wrapper, not canonical Document. --stdout text emits its Markdown/LaTeX
projection. Final artifact count/bytes remain policy-bounded; source/image hashes
are checked after worker execution. Workers and output parents remain trusted;
these limits are not an OS sandbox. Completion/truncation remains unknown.

No Bobine/ORT model dependency is added to the main workspace. No automatic
selection, rendering, orientation correction, review work, merge or release.
Rust tests are prepared (5 facade + 1 CLI); Cargo/rustc are unavailable in the
preparation environment. Compilation, tests, real CLI inference, formatting and CI
are NOT claimed passed. Four edited base-file Git blobs were checked exactly.
