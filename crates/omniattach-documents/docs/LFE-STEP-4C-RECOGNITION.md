# LFE-4C: opt-in recognition comparison

Implements evaluation tooling and two real-backend adapters, NOT a completed model evaluation or a winning-backend selection. Ten local protocol/scoring tests passed using explicit stubs, not OCR. No real model inference, accuracy measurement or latency benchmark was executed. Rust adapter compilation is unverified (no Cargo installed). Python reference was syntax-checked, not imported against installed TexTeller. CI runs protocol tests only.

## Isolation and provisioning

No production crates, root Cargo manifests/locks, schemas or routes change. tools/lfe-formula-bobine is an isolated unpublished workspace; Bobine is pinned to =0.9.1, but transitive versions are not locked yet. Build separately and retain its generated Cargo.lock with evaluation evidence:

```sh
cargo build --release --manifest-path tools/lfe-formula-bobine/Cargo.toml
python -m unittest discover -s scripts -p test_lfe_equation_eval.py -v
```

Provision compatible TexTeller encoder, merged KV-cache decoder, tokenizer.json and ONNX Runtime shared library; set ORT_DYLIB_PATH to the actual library. Rust uses local load_from_paths and CPUExecutionProvider, not a Hub constructor. Bobine still brings a full ingestion dependency graph. Review code AND exact model/export licenses separately.

For Python, install/pin TexTeller and PyTorch in a separate environment and provision local checkpoint/tokenizer directories. Worker enables HuggingFace/Transformers offline flags; this is NOT a network sandbox. API inspected in OleehyO/TexTeller texteller/api/load.py and inference.py. img2latex formats output even with keep_style=True: returned LaTeX is retained, but it is NOT raw-token parity with Bobine. Verify export/weight equivalence before attributing differences to Rust.

## Config and execution

Create a local JSON file with backends: [{id, argv, artifacts}]. IDs must be unique; argv is an executable/argument array, never shell text. artifacts is a nonempty array of {path, sha256}, with actual local hashes. Declare ALL model, external tensor and tokenizer files. Optional provenance fields may document revision, precision, license and runtime versions. Use absolute paths; paths are not relative to the config file. Config is trusted executable configuration.

Rust argv: [binary, encoder, merged_decoder, tokenizer_json, "512"].
Python argv: [python_executable, absolute_path_to_lfe_texteller_worker.py, model_directory, tokenizer_directory, "512"].

```sh
python scripts/validate_lfe_equation_fixtures.py /tmp/lfe4b-equations
python scripts/lfe_equation_eval.py /tmp/lfe4b-equations \
  /tmp/lfe4c-config.json /tmp/lfe4c-results.json --repeats 3 --timeout 600
```

Output must be NEW; a configuration failure can leave an empty/partial output. Failed backend batches are retained and cause exit 1. Per-image errors and empty predictions stay in accuracy denominators; completed transport is not an accuracy gate. No automatic model download, retries or fallback. No merge/publication.

## Protocol and evidence

The baseline evaluates eight UNIQUE clean formula PNGs from LFE-4A, deduplicating repeated PDF occurrences. Advanced formulas without formula_image are excluded. No PDF-region extraction added.

One process per backend receives {schema_version:"omniattach/formula-worker/0.1", images:[{id,path}]}; no labels are sent. Both adapters load once per sequential batch. Response: {results:[{id,status,latex,elapsed_ms}], metadata:{...}}. Status is ok/error; every requested ID must occur exactly once. Errors retain error text; elapsed_ms is optional for custom workers. Stdout is JSON-only. Filesystem access is not sandboxed, so a trusted worker could still read external ground truth.

Report retains predictions, image/model hashes, config, package/load metadata, batch wall time, per-image reported times, exact/trimmed-exact accuracy and pairwise agreement. Only boundary whitespace is trimmed: internal LaTeX whitespace is preserved. Every case, including errors, stays in accuracy denominators. Repeats are timing observations, not additional unique formulas. Median includes first inference; no warm-up exclusion. Batch wall includes startup/import/loading; per-image latency does not. Hashes identify bytes, not equivalent weights or license rights. Inputs are rechecked before each backend and after execution.

No fabricated confidence. Public APIs expose no EOS status: completion/truncation is UNKNOWN. Usable means nonempty output, not verified complete mathematics. Semantic equivalence, rendering similarity, memory, handwriting, broad generalization and actual model quality are unmeasured. A real provisioned run is still required to complete the empirical LFE-4C comparison.

## Limits

One timeout per batch. Unix timeout kills the process group; other platforms kill only the direct child. Output spools to temporary files, then JSON read is capped at 8 MiB and retained stderr at 64 KiB. These are NOT hard disk/RAM quotas; untrusted programs require external resource limits/sandboxing. Local machine paths, model weights and results should remain outside Git.
