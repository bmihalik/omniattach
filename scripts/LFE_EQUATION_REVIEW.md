# LFE-4D5B: blind candidate review, not classifier inference

Prepare from the original D3 run, never from transformed controls. This tool reads
only proposals.json and crop PNGs; it does not read fixture references, IoU matches,
recognizer predictions or scoring rules. It retains every occurrence, randomizes
order, uses opaque IDs and re-encodes unchanged RGB pixels without PNG text metadata.

```bash
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_review.py -v
# Stop if tests fail; use a new output outside REGION_RUN.
export REVIEW_KIT="$LFE4C_HOME/review-kit-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_review.py prepare "$REGION_RUN" "$REVIEW_KIT"
```

Send ONLY the blind/ subdirectory to reviewers. Keep private-map.json and original
PDFs, fixture labels, model outputs and prior analysis inaccessible to them.
Open blind/index.html to view pixels. Reviewers edit separate copies of review-a.json
and review-b.json, replace UNSET with distinct identities, and label EVERY item.
The current conversation contains expected answers and is not an independent blind
review context. Identity/independence declarations are not technically verified.

## Annotation rules

- equation: a standalone mathematical expression, whether or not it contains '='.
- non_equation: prose, code or headings without a standalone mathematical expression.
- mixed: substantial mathematical and nonmathematical content in one crop.
- ambiguous: cannot confidently decide candidate type.
- unreadable: image quality prevents a reliable content judgment.

For equation only, clockwise_degrees is the correction needed to make it upright:
0, 90, 180, 270, or null for uncertain. Other categories require null. Category null
means unfinished and is rejected; ambiguous is an explicit completed judgment.
Do not force mixed/ambiguous/unreadable into positive or negative classes. Preserve
notes and original review files. No LaTeX transcription or model-guided label choice.

```bash
"$CTRL" scripts/lfe_equation_review.py compare "$REVIEW_KIT/blind" \
  /path/to/completed-a.json /path/to/completed-b.json /path/to/new-comparison
```

Disagreements produce pending_adjudication and a blank adjudication template;
labels.json is NOT written. A named adjudicator resolves every conflicting ID after
reviewing both votes and pixels, without model/reference answers. Then rerun into
a NEW directory with --adjudication /path/to/completed-adjudication.json. The file
is bound to that exact review pair. Full consensus/adjudication emits all labels,
including uncertainty, not selector predictions. Notes differences alone do not
count as label disagreements; originals remain in the comparison report.

## Dataset limitations

Private mapping retains original proposal IDs and exact-RGB-pixel groups. All
identical pixels receive the same provisional development/holdout assignment;
there is no guaranteed split size. Near duplicates and formula families can still
cross groups. This fixture has already been inspected: these are NOT independent
held-out performance data. Inspect duplicate-label consistency and expand to an
independently sampled corpus before training/freezing a real benchmark.

No classifier/orientation gate, automatic decisions, accuracy scores, inference or
changes to earlier checkpoints are included. Nine tests passed locally against the
full new module with synthetic PNGs. Actual user crops/reviewer judgments and CI
results remain unverified. The D5A plan/pixel provenance audit remains separate.
