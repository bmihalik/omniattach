# Explicit model provisioning API

Feature `backends` exposes omniattach::model_store. No conversion route, default
extraction or CLI startup calls it implicitly. Storage has no hardcoded cache root.

```rust,ignore
use omniattach::model_store::*;
let store = ModelStore::new("/srv/omniattach-models")?;
let spec = texteller3_onnx_spec()?;
let mut options = ProvisionOptions::default(); // offline, external execution denied
let status = store.inspect(&spec, &options)?; // read-only, verifies ready files
options.allow_network = true;
options.allow_external = true; // only needed for the installed-curl transport
let installed = store.provision(&spec, &options, &CurlModelFetcher::default())?;
// Existing verified installations are reused even with both permissions false.
```

The caller can supply another trusted ModelSpec/fetcher. Specs require a full
40-hex revision, flat safe filenames, HTTPS URLs, SHA-256 and bounded sizes. The
TexTeller preset uses revision/checksums from the previously evaluated profile;
remote metadata could not be retrieved during preparation. No new remote download
or model-compatibility/license certification is claimed. Review exact upstream
model/export licenses before downloading. Hashes pin bytes, not legal permissions.

Files live under ROOT/ID/REVISION. Downloads use a private temporary directory,
then type/size/hash verification. A create-new writer lock prevents cooperating
writers from publishing the same revision concurrently. Destination creation is
no-clobber; a verified receipt is atomically published last. Directory existence
alone is not readiness. Incomplete/corrupt installations are reported, never
silently replaced. An interrupted publication or crashed-process lock may require
manual inspection/cleanup. Root and parents must be trusted; this is not a hostile
filesystem sandbox or a power-loss durability guarantee. Exact existing specs are
immutable; no floating-latest update, automatic fallback or archive extraction.

Curl must already be installed. Its config-file loading is disabled; only HTTPS
redirects are permitted. The existing runner controls timeout/cancellation, output
and Unix process groups. Size checks bound accepted files, not OS disk/RAM quotas.
Custom fetchers must honor the same contracts. Non-Unix transport needs a suitable
injected fetcher. ONNX Runtime and the worker executable are NOT downloaded.

With `documents` also enabled, installed.equation_worker_config_json(worker,
runtime, 512) verifies model files and hashes the explicit local worker/runtime,
returning the JSON accepted by EquationWorkerConfig::from_file. Save that JSON with
create-new semantics, or deserialize and pass it to with_pdf_equation_worker.
Set the normal CLI worker-config environment variable to use it. No inference or
runtime installation occurs during configuration. Worker/executable changes still
require refreshed configuration hashes.

```bash
cargo test --locked -p omniattach --features workspace --test model_store
cargo check --locked -p omniattach --features backends --example provision-equation-model
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/omniattach-models --inspect
# Explicit pre-download, before conversion:
cargo run -p omniattach --features backends --example provision-equation-model -- \
  /srv/omniattach-models --download --allow-network --allow-external
```

11 Rust tests are prepared with workspace features, using local fake fetchers,
not network/models. Cargo/rustc are unavailable here: compilation, tests, formatting,
real downloads and CI remain unverified. The preset pins were checked against the
existing evaluation config; two edited base Git blobs and TOML parsing were checked.
No dependency/version/lockfile change or new main-CLI `models` command is included.
