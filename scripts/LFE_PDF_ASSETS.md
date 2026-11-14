# LFE-4D1: PDF-exported formula assets

Uses the existing Rust `pdf-images` and `pdf-inspect` examples, not a new detector
or rendered-page cropper. Current fixtures are expected to yield 12 eligible of
36 positive occurrences; real extraction/inference remains pending until run.
Native text, outlines and full-page scans stay explicit unsupported occurrences.

Association is fixture-only: same page/traversal ordinal with exact image-count
agreement. Inspection anchors join projected blocks to exported files. Reused
placements remain distinct. Models receive exported files, never reference PNGs.
References only check RGB dimensions/pixels. Placement checks allocated regions
within 0.01 PDF units, not ink bounds or display-normalized CropBox/Rotate/clipping.

From repository root, after pulling this branch, use the provisioned environment:

```bash
cargo build --locked -p omniattach-documents --features pdf \
  --example pdf-images --example pdf-inspect
export LFE4C_HOME="$HOME/.local/share/omniattach/lfe4c"
export ORT_DYLIB_PATH="$(cat "$LFE4C_HOME/runtime-path.txt")"
"$LFE4C_HOME/reference-venv/bin/python" scripts/lfe_equation_pdf.py \
  /tmp/lfe4b-equations "$LFE4C_HOME/pdf-assets-$(date +%Y%m%d-%H%M%S)" \
  --pdf-images "${CARGO_TARGET_DIR:-target}/debug/examples/pdf-images" \
  --pdf-inspect "${CARGO_TARGET_DIR:-target}/debug/examples/pdf-inspect" \
  --config "$LFE4C_HOME/comparison-config.json" --repeats 3 --timeout 600
```

Omit `--config` for extraction/fidelity only. Output must be new. Return `report.json`
and retain partial exports/stderr if export fails before report creation.

Coverage, pixel fidelity, placement and conditional recognition are separate.
All-positive rates include unsupported/missing occurrences with no credit; these
are not fabricated model errors. Strict credit also requires pixel and placement
checks. Conditional unique-formula scores cover submitted occurrences only.
Hints on negatives are inventoried, not detector false positives. `completed_scoped`
is not full PDF support. Supported extraction/fidelity, negative-image or worker
failures produce nonzero exit; invalid config/hashes fail explicitly.

Tests require Pillow (in the fixture requirements):

```bash
python -m unittest discover -s scripts -p test_lfe_equation_pdf.py -v
```

Local tests use stub exports/dispatch, no real PDFs or models. Dedicated extraction
CI additionally builds the existing examples and tests generated PDFs without
models. No CI, real extraction, recognition or speed claim follows from local tests.
