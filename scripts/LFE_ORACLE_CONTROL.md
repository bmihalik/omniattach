# LFE-4D2: ground-truth-guided rendering control

This is an oracle recognition control, NOT equation detection or extraction recall.
D1's embedded-image baseline stays unchanged. Default fixtures select 24 native
mathtext/outline/full-page-scan occurrences; the other 12 remain out of this
control's scope. Advanced optional LaTeX fixtures are not selected.

Crops use allocated manifest regions, not exact ink bounds. Full pages are
rendered at 150 DPI by default; floor/ceil converts bottom-left PDF coordinates
to top-left PNG coordinates. No reference PNG is opened. Blank crops fail;
threshold ink statistics and edge contact are diagnostics, not proof of complete
or uncontaminated crops. Rotated/cropped/nonzero-origin/UserUnit!=1 pages fail
closed. Page rasters are capped at 20 million pixels. Rendering is for trusted
synthetic fixtures; --timeout bounds workers, not native renderer execution.

Use a separate controller environment so recognizer packages/artifacts stay put:

```bash
git switch lopdf-floating-elements
git pull --ff-only origin lopdf-floating-elements
export LFE4C_HOME="$HOME/.local/share/omniattach/lfe4c"
"$LFE4C_HOME/reference-venv/bin/python" -m venv "$LFE4C_HOME/render-venv"
export CTRL="$LFE4C_HOME/render-venv/bin/python"
"$CTRL" -m pip install -r scripts/requirements-lfe-render.txt
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_oracle.py -v
export ORT_DYLIB_PATH="$(cat "$LFE4C_HOME/runtime-path.txt")"
export ORACLE_RUN="$LFE4C_HOME/oracle-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_oracle.py /tmp/lfe4b-equations "$ORACLE_RUN" \
  --config "$LFE4C_HOME/comparison-config.json" --dpi 150 --repeats 3 --timeout 600
```

Omit --config for model-free preparation. Output must be new. Keep report.json
and crops; attach the report after running. With default fixtures, 24 ready crops
would produce 72 requests per backend. This is an expectation, not an inference
result established by this change.

The report marks ground_truth_guided=true and detector_evaluated=false. Conditional
recognition scores cover submitted crops; oracle_selected_match_rate additionally
keeps failed/blank selected crops in the denominator, with no credit. Neither rate
is detector accuracy or D1's whole-corpus score. The report also separates source
representations and same-crop repeat consistency; legacy formula-level consistency
can mix different rendered representations. Recognition mismatches are measured,
not treated as tool crashes. Crop/worker failures cause nonzero exit.

Six local tests cover real PDF rendering/coordinate checks, rejected transforms,
blank/failure accounting and a model-free CLI; worker dispatch/scoring is stubbed.
Dedicated CI generates the real default corpus and checks all 24 controls without
models. Full-corpus rendering, inference and CI status remain pending until run.
A later detector-proposal track must remain separate from these oracle crops.
