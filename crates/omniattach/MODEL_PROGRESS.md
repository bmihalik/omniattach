# Provisioning progress: API and executable example

Progress is opt-in through ModelStore::provision_with_progress. Existing provision,
ProvisionOptions and ModelFetcher interfaces remain unchanged. No conversion route
starts downloading, and all existing network/external permissions still apply.
The progress-aware method requires a Sync fetcher. CurlModelFetcher supports it.

```rust,ignore
use omniattach::model_store::*;
use omniattach::model_progress::*;
let store = ModelStore::new("/srv/models")?;
let spec = texteller3_onnx_spec()?;
let options = ProvisionOptions { allow_network: true, allow_external: true, ..Default::default() };
let observer = |event: &ModelProgressEvent| {
    // Update application state or clone/send the event to your GUI/event channel.
    // Keep this callback fast and non-panicking.
};
let model = store.provision_with_progress(&spec, &options,
    &CurlModelFetcher::default(), &observer)?;
```

This is a synchronous API. Observers run on the provisioning caller's thread;
applications should run provisioning in their own background task for a responsive
UI. They may retain/cancel the existing options.cancellation token. A scoped helper
thread runs the blocking transport while the caller observes staging-file length
approximately every 200 ms. Custom fetchers must honor deadline/cancellation: an
uncooperative transport cannot be forcibly interrupted by this wrapper. Observer
blocking also delays delivery. No new generic ProcessRunner behavior is introduced.

Events use schema omniattach/model-progress/0.1 and include model/revision, stage,
optional file/index, file count, current file bytes, aggregate observed bytes and
elapsed milliseconds. Stages include cache checking/hit, file start/downloading,
file downloaded, verification pending, completion, cancellation and failure.

Byte counts are observed staging-file lengths, not network wire-byte counters.
Manifest max_bytes is a safety limit, NEVER a percentage denominator. total_bytes
is null because this transport supplies no reliable Content-Length. Do not invent
percentages/ETA. FileDownloaded does not mean checksum verified. Global Completed
occurs only after the existing store finishes verification/publication. A cache
hit reports no transferred bytes. Existing verification can take time without
periodic hashing events. Cache, publication and checksum safeguards are unchanged.

## CLI feedback

The existing provisioning example gains --progress text|ndjson|none (default text).
Text download updates are throttled to approximately one per second; phase changes
are printed separately. NDJSON emits the API events. Progress/errors go to stderr;
stdout remains the final installed-model/status JSON. CLI errors are a separate
omniattach/provision-error/0.1 JSON record and nonzero exit. Arbitrary custom
fetcher logging/panic hooks are not controlled by the library observer.

```bash
cargo test --locked -p omniattach --features workspace --test model_progress
cargo test --locked -p omniattach --features backends --example provision-equation-model
cargo test --locked -p omniattach --features workspace --test model_store
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/models --download --allow-network --allow-external --progress text
# Machine feedback: add --progress ndjson and capture stderr separately.
# Disable progress: --progress none (failures still report errors).
```

This updates the already supplied provisioning CLI example, not a new main-CLI
models subcommand. Conversion progress and model inference are unrelated.
Eight API tests plus two example tests are prepared using local fake transports,
including an intermediate-byte handshake before transport completion. Cargo/rustc
are unavailable during preparation: no compilation, tests, formatting, real network
transfer or CI success is claimed. Two edited base Git blobs were verified.
