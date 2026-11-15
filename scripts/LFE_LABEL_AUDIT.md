# LFE-4D5C: mapped annotation QA (not selector training)

Joins the original D3 report/proposals, the full D5B review kit (including private
mapping and blind PNGs), and original reviewed-labels/0.1. It checks fingerprints,
complete ID coverage, source/review pixel equality, exact-pixel groups, provisional
split assignments, duplicate-label consistency and orientation coverage. Fixture
reference associations are used only for post-review diagnostics, not label changes.

The supplied upright-amendment.json records the user's explicit 35-upright
clarification. It is bound to the exact original labels.json and pack hashes. It
changes only its listed unknown equation orientations to 0; existing 90/270 labels,
categories, original reviews and files are preserved. Without an amendment, null
continues to mean unknown. This is user clarification, not fresh independent review.
Use the ORIGINAL labels export, not the previously derived amended-labels file.

```bash
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_label_audit.py -v
# Stop if tests fail. Save the supplied amendment under REVIEW_KIT first.
export LABEL_AUDIT="$LFE4C_HOME/label-audit-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_label_audit.py \
  "$REGION_RUN" "$REVIEW_KIT" .results/new-comparison/labels.json "$LABEL_AUDIT" \
  --orientation-amendment "$REVIEW_KIT/upright-amendment.json"
```

All input files are read-only; output must be new and outside the source and kit.
Retain report.json and mapped-labels.csv. Output is internal/post-review: never
send the source mapping or proxy diagnostics to independent blind reviewers.

The report identifies equation labels on unmatched positive-page crops, helping
investigate 37 reviewed equation candidates versus 36 fixture occurrences. Those
are different units; no equality is assumed. Matched-proxy/non-equation and known-
negative/equation disagreements are flags for inspection, never auto-corrections.
Exact-pixel label conflicts and unknown equation orientations are also flagged.
Status checked means only that these checks passed, not validated annotation truth.

No selector or orientation model executes; no accuracy, held-out generalization or
reviewer independence is claimed. The fixture has already been inspected. Pixel
splits do not protect against related formula families or near duplicates. Resolve
findings and expand to genuinely independent data before benchmark/model claims.
Nine tests passed against the full new module with synthetic PNGs; real user-kit
results and CI remain unverified. Earlier code/scoring/dependencies stay unchanged.
