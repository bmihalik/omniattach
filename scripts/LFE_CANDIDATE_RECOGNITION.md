# LFE-4D4: recognize every frozen D3 candidate

Reads an existing completed D3 directory; does not render, resize, rotate, deduplicate
or reference-filter its crops. Crop hashes/dimensions and the separate proposal
artifact are checked. Source files/config are checked between workers. The D3
matching implementation must still have the fingerprint recorded by the input run.
Requests contain only IDs and paths. Associations are replayed after all workers
finish; the existing conservative formatter is unchanged.

For the audited D3 run: 71 proposals x 3 repeats = 213 requests per backend.
35 matched references give 105 conditional cases, but localization-qualified
recognition keeps all 36 x 3 = 108 positive cases in its denominator. Unmatched
outputs remain in the report; negative-page outputs are a subset of unmatched
outputs, not extra cases. No classifier runs: neither empty nor nonempty LaTeX is
a validated classification decision. Transport/protocol/integrity failures fail
the run; per-image errors/empty outputs remain recorded with no recognition credit.
Timings over all candidates are separate from matched-subset timings. Completion,
truncation and peak memory remain unmeasured. Workers are trusted, not sandboxed.

Use the existing rendering-controller venv; do not alter recognizer environments.
REGION_RUN must name the existing D3 output, not a new directory.

```bash
git switch lopdf-floating-elements
git pull --ff-only origin lopdf-floating-elements
export LFE4C_HOME="$HOME/.local/share/omniattach/lfe4c"
export CTRL="$LFE4C_HOME/render-venv/bin/python"
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_candidate_eval.py -v
# Stop if tests fail. Output must be new and outside REGION_RUN.
export ORT_DYLIB_PATH="$(cat "$LFE4C_HOME/runtime-path.txt")"
export CANDIDATE_RUN="$LFE4C_HOME/candidates-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_candidate_eval.py "$REGION_RUN" \
  "$LFE4C_HOME/comparison-config.json" "$CANDIDATE_RUN" --repeats 3 --timeout 1800
```

Timeout is per complete backend batch. Keep existing model token limits unchanged.
Attach report.json; retain request.json and backend response artifacts. If no report
is produced, retain the terminal error. There are no new controller dependencies.

Seven model-free tests passed locally. Local tests used inspected shared-function
extracts plus real subprocess fake workers; CI imports the full repository modules.
A supplied-ledger test with placeholder PNGs exercised 213 requests per fake backend,
105 conditional cases and the 108-case denominator. These are plumbing tests, not
real model inference, original-crop verification, or verified GitHub CI results.
