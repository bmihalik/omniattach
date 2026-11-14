# LFE-4D5A: separate crop controls and selector audits

D3/D4 and the conservative scoring policy stay unchanged. This checkpoint does
not add a trained classifier or choose a best transform from reference labels.

## Crop controls

Every original proposal receives all four fixed policies: byte-identical PNG,
16-pixel white border, 90-degree counterclockwise + border, and 90-degree clockwise
+ border. Quarter turns are lossless; no resizing or page-context recropping runs.
Global rotations are sensitivity controls, not a deployable orientation selector.
The plan and every derived pixel are validated before inference. Sources and
prepared inputs are hash-checked between workers; outputs must be new and separate.

The default is ONE repeat: for the current 71 candidates, 71 requests per policy
and 284 per backend. Use --repeats 3 at preparation for 852 per backend. Every policy
keeps the original 36-positive denominator (35 localized), multiplied by repeats.
Original proposal footprints/IoU matches remain fixed; this cannot recover D3's
localization miss. Each policy is reported separately; no oracle best-of fusion.
An identity rerun is a contemporaneous control, not guaranteed D4 reproduction.

From the repository, use the existing controller and recognizer environments:

```bash
"$CTRL" -m unittest discover -s scripts -p test_lfe_equation_controls.py -v
# Stop if tests fail. REGION_RUN must still name the original D3 directory.
export CONTROL_PLAN="$LFE4C_HOME/control-plan-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_controls.py prepare "$REGION_RUN" "$CONTROL_PLAN"
# Preparation runs no model. Inspect its request counts before continuing.
export ORT_DYLIB_PATH="$(cat "$LFE4C_HOME/runtime-path.txt")"
export CONTROL_RUN="$LFE4C_HOME/control-run-$(date +%Y%m%d-%H%M%S)"
"$CTRL" scripts/lfe_equation_controls.py run "$CONTROL_PLAN" \
  "$LFE4C_HOME/comparison-config.json" "$CONTROL_RUN" --timeout 1800
```

Timeout is per policy/backend batch; workers reload between policies. Keep model
artifacts/token limits unchanged. Retain plan.json, requests, crops and raw backend
responses; attach the run's report.json. Completion/truncation and peak memory
remain unmeasured. Local tests use inspected shared-function extracts and real fake
subprocesses; CI imports full repository modules. Nine model-free tests passed;
real model results, original-crop pixel verification and CI results are not claimed.

## Selector audit (no selector execution)

Preparation exports selector-input.json with only original IDs/paths/hashes, and
an UNCONFIGURED all-abstain template. A real, independently specified producer must
supply accept/reject/abstain for every original candidate, identify itself, and keep
the source proposal hash. The template is not a validated classifier. No rule
converts nonempty LaTeX into an acceptance decision.

```bash
"$CTRL" scripts/lfe_equation_controls.py selector-audit "$REGION_RUN" \
  /path/to/actual-selector-decisions.json /path/to/new-selector-audit.json
```

Post-hoc groups are 35 matched-reference positive proxies, 3 known-negative-page
candidates, and 33 UNLABELED positive-page candidates on the current fixture.
Unmatched does not imply non-equation: the oversized formula candidate is included
in that unlabeled group. Report accept/reject/abstain counts and proxy retention
with the original positive denominator; no semantic precision/F1 is calculated.
Decision-file hashing is not independent verification of producer behavior or
absence of label leakage. Workers/producers are trusted, not filesystem-sandboxed.
Actual selector development and independently labeled/held-out evaluation remain
future work. These proxy groups must not be presented as a training/test benchmark.
