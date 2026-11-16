# Hugging Face Hub-only provisioning (breaking API replacement)

Only provisioning changes. Manual local model paths, EquationRecognizer callbacks,
Bobine workers and main conversion routes remain supported and unchanged. Conversion
never silently downloads. There is no generic ModelFile URL, ModelSpec, ModelFetcher,
FetchContext, CurlModelFetcher, custom installation receipt or legacy provider path.
Old model files are NOT deleted/imported; they remain usable through local inference.

ModelStore now uses the official hf-hub 0.4.3 async client with a private Tokio runtime
on a dedicated worker thread. No curl or other download executable is needed.
Network authorization is allow_network; allow_external is removed from provisioning
(inference subprocess execution still has its separate existing permission).

```rust,ignore
use omniattach::model_store::*;
let store = ModelStore::new("/srv/huggingface/hub")?;
let spec = HubModelSpec {
    repo_id: "owner/model".into(), revision: "main".into(),
    files: vec![HubFile { path: "nested/model.safetensors".into(), sha256: None }],
};
let options = ProvisionOptions { allow_network: true, ..Default::default() };
let model = store.provision(&spec, &options)?;
// Or provision_with_progress(&spec, &options, &observer).
```

Repo IDs and selected files are required; nested relative file paths are supported.
Branches/tags resolve once to a commit before fetching the selected files, avoiding
mixed revisions. InstalledModel exposes that resolved revision and local paths.
Download support does NOT imply inference compatibility with every Hub architecture.
TexTeller's worker-config helper still requires its three known ONNX/tokenizer names.
Select all needed shards and external tensor files explicitly. No remote code runs.

Storage is the standard Hub models--OWNER--NAME/{refs,blobs,snapshots} cache, not the
old ROOT/ID/REVISION installation layout. ModelStore::from_env honors HF_HUB_CACHE,
then SDK HF_HOME/default cache behavior. Offline inspect/verify never build an HTTP
client. Direct commit snapshots work without a commit-named refs entry. Shared-cache
symlinks are allowed when they resolve inside the configured cache root. Cache and
parents must be trusted. Files are always read/hash-counted; trusted SHA-256 pins
are checked when supplied. Without pins, cache contents rely on normal Hub/cache
trust, not an independent cryptographic attestation of every remote file.

Default provisioning reuses complete cached selections, including offline. Explicit
refresh=true re-resolves a branch and requires network permission. Corrupt pinned
files fail; no cache wipe/legacy migration or fallback to another revision/provider.
The client uses the fixed official Hub endpoint; HF_ENDPOINT is not honored. Normal
Hub CDN redirects are handled by the SDK. Authentication accepts options.token,
then HF_TOKEN, then SDK cached credentials. Tokens are not serialized and our
options Debug implementation redacts the token. Do not log upstream client internals.

Progress is still callback-based, schema omniattach/model-progress/0.2. Model ID is
the Hub repo ID; revision becomes the resolved commit. SDK totals now supply a real
per-file total when known. SDK counters can include resumed prefixes, not just wire
traffic; no whole-model percentage is inferred. Cached reuse reports CacheHit.
Completed is emitted only after selected-file verification. Callbacks run on the
provisioning worker thread; send events to a UI channel. The synchronous API blocks
its caller, so invoke it in an application background task. Cancellation/deadline
checks race the SDK future; shutting down the private runtime cancels outstanding
SDK async tasks. Shared cache partial files may remain resumable. This is not an OS
disk/RAM sandbox; byte budgets govern accepted files, not a hard temporary quota.

## Example and validation

```bash
# Native SDK dependencies were added; regenerate and retain the workspace lock.
cargo generate-lockfile
cargo fmt --all
cargo test --locked -p omniattach --features workspace --test model_store --test model_progress
cargo test --locked -p omniattach --features workspace --lib model_progress
cargo check --locked --workspace --all-features --all-targets
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/huggingface/hub --inspect
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/huggingface/hub --download --allow-network --progress text
# Generic Hub file selection; replace the identity/path with a real selection:
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/huggingface/hub --download --allow-network --repo owner/model \
  --revision main --file nested/model.safetensors --progress ndjson
```

The default remains the evaluated pinned TexTeller profile, now expressed only as
Hub repo/revision/files/checksums. Progress/errors go to stderr; stdout is final
JSON. No new main-CLI models subcommand is introduced. Offline fixtures and SDK
progress-adapter tests are prepared. Cargo/rustc are unavailable in preparation:
compilation, tests, formatting, live Hub download and CI are unverified. Cargo.lock
cannot be regenerated here; regenerate/commit it before locked CI or merge. Existing
locked workflows intentionally remain gated on that regenerated lockfile.
