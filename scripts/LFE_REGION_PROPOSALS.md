# LFE-4D3: independent region-proposal baseline

This is an unclassified ink-band proposer, not a semantic equation detector.
No OCR runs. D1/D2 and their recognition/scoring policies are unchanged.

All PDF pages, including negatives, are rendered. propose(image) receives pixels
only, not identities, labels or reference boxes. It groups occupied rows, merges
up to 12 blank rows, adds 4 pixels of padding and rejects bands below 4 ink pixels.
Settings were fixed before the default-corpus experiment. Pixel parameters mean
changing DPI changes the experiment. No reference-based candidate filtering occurs;
all proposed crops are retained, including prose and negative-page candidates.
This is data-flow separation, not a sandbox against malicious proposer code.

After each page's proposals are fixed, scoring converts allocated manifest slots
through PDFium's page/bitmap mapping and derives grayscale<245 ink envelopes inside
them. These are fixture-reference proxies, not exact semantic ink annotations;
they assume slots contain the intended target without contamination. Matching is
deterministic maximum-cardinality one-to-one at IoU>=0.5, not greedy matching.
Unmatched proposals and negative-page candidates remain visible. Missing references
stay in the all-positive denominator and cause failure status. A completed baseline
does not imply good proposal precision, detection accuracy or recognition accuracy.

The baseline merges some rotated/multicolumn layouts and does not distinguish prose
from math. There is no learned classifier or third-party detector adapter yet.
Evaluation is for trusted generated fixtures, with a 20-million-pixel page cap;
there is no native-renderer hard timeout or untrusted-PDF security guarantee.

Run from the repository using the existing separate rendering-controller venv:

```bash
git switch lopdf-floating-elements
git pull --ff-only origin lopdf-floating-elements
export LFE4C_HOME="$HOME/.local/share/omniattach/lfe4c"
export CTRL="$LFE4C_HOME/render-venv/bin/python"
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_regions.py -v
# Stop if tests fail; the output directory below must be new.
export REGION_RUN="$LFE4C_HOME/regions-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_regions.py /tmp/lfe4b-equations "$REGION_RUN" --dpi 150 --iou 0.5
```

Attach report.json and retain proposals.json, full pages and candidate crops.
There are no new dependencies for the existing rendering-controller environment.

Seven local tests passed, including real rendering, CropBox/rotation reference
mapping, non-greedy matching, negative candidates, and proposal invariance when
labels/boxes change or all positive annotations are removed. A locally regenerated
default corpus yielded 71 proposals, 35/36 proxy matches, 36 unmatched proposals
and 3 negative-page candidates; the missed occurrence was page-transform.pdf.
Those are local observations, not results on the user's PDF bytes, general detector
accuracy, model inference or verified GitHub CI. CI checks execution/accounting,
not a tuned accuracy target. The next stage needs semantic selection/refinement
and recognition of independently proposed crops, without oracle prefiltering.
