# Python TexTeller reference comparison

This is an end-to-end CPU comparison, not raw-token or numerical engine parity.
The Rust worker uses ONNX; the Python worker uses upstream TexTeller/PyTorch
preprocessing, greedy decoding and formatting (`keep_style=True`, one beam).
Both receive the same clean images and token budget. Upstream claims identical
checkpoint weights for the unified model repository; this helper does not
independently prove ONNX/PyTorch tensor equality.

## 1. Separate reference environment

Run from the repository root. Use Python 3.12 for this candidate dependency set,
not the existing Python 3.14 runtime environment. If `python3.12` is unavailable,
install it first; alternatively `uv venv --python 3.12` can create the environment.
Do not replace the working Rust runtime environment.

```bash
export LFE4C_HOME="$HOME/.local/share/omniattach/lfe4c"
python3.12 -m venv "$LFE4C_HOME/reference-venv"
export REFPY="$LFE4C_HOME/reference-venv/bin/python"
"$REFPY" -m pip install --upgrade pip
"$REFPY" -m pip install torch==2.6.0 torchvision==0.21.0 \
  --index-url https://download.pytorch.org/whl/cpu
"$REFPY" -m pip install texteller==1.0.2 'optimum[onnxruntime]==1.24.0' \
  transformers==4.47.0 onnxruntime==1.21.0 'huggingface-hub<1.0'
"$REFPY" -m pip check
"$REFPY" -m pip freeze --all > "$LFE4C_HOME/reference-environment.txt"
```

These are direct version pins, not a complete transitive lock. Real installation,
imports and inference remain unverified until run locally. Stop on any installation
or `pip check` error. The environment inventory is retained for troubleshooting.
The old `texteller==0.1.2` meta-package is not the API used by this worker.

## 2. Provision and generate a comparison config

```bash
"$REFPY" scripts/lfe_equation_reference.py --download
```

Defaults use the existing `$LFE4C_HOME/config.json` and `texteller3-onnx` directory,
and create `$LFE4C_HOME/comparison-config.json` exclusively. `--base`, `--model-dir`
and `--output` override these paths. Existing output is never overwritten.

Downloads happen only with explicit `--download`, from
`opticsWolf/texteller3-onnx` at
`5f817336d600ae880205362e16092994aecd01e1`. This adds the substantial safetensors
checkpoint and configuration/tokenizer files; it does not redownload ONNX files.
Without `--download`, all listed assets must already exist locally.

The helper verifies existing Rust artifact hashes before and after provisioning,
requires declared model artifacts and matching model provenance, and checks exact
`tokenizer.json` byte identity across backends. It preserves the original Rust
config, reuses its token budget, fingerprints reference files/worker/interpreter,
and records selected installed package versions. Local hashes are fingerprints,
not independent proof of Hub origin or checkpoint equivalence. No inference runs
while creating the config. The Python worker sets Hub/Transformers offline flags.

## 3. Run both backends

```bash
export ORT_DYLIB_PATH="$(cat "$LFE4C_HOME/runtime-path.txt")"
test -f "$ORT_DYLIB_PATH"
"$REFPY" scripts/lfe_equation_eval.py \
  /tmp/lfe4b-equations \
  "$LFE4C_HOME/comparison-config.json" \
  "$LFE4C_HOME/comparison-results-$(date +%Y%m%d-%H%M%S).json" \
  --repeats 3 --timeout 600
```

The Rust library path still points to its working runtime, not the reference
venv's ONNX Runtime. The timeout applies to each backend batch. Return the entire
report, including failed-backend diagnostics if either worker fails.

Compare per-backend raw and formatting-normalized accuracy, unique-formula counts,
repeat consistency, and cross-backend agreement. Keep load time and batch wall
time separate from reported per-image latency. Thread settings, preprocessing,
formatting and model implementations are not controlled to numerical parity;
latency is pipeline-specific, not an isolated language/runtime speed comparison.
No real reference inference or broader recognition claim follows from stub tests.

## Tests

```bash
python -m unittest discover -s scripts -p 'test_lfe_equation_[em]*.py' -v
```

The reference config tests use dummy bytes, no model imports and no downloads.
