# LFE equation scoring

Existing raw/trimmed scores and predictions are unchanged. Additive metrics use
policy `omniattach/formula-formatting/0.1`; they do not prove semantic equivalence.

The delimiter metric strips one complete outer `\[...\]` or `\(...\)` pair.
Restricted token normalization ignores ASCII math whitespace and explicit spacing,
normalizes fraction style, paired left/right parentheses or brackets, and
single-atom script braces. Unknown commands/text/environments disable rewriting
for the entire expression; only delimiter-stripped literal equality remains.
Multi-token scripts, operators, control-word boundaries and text are preserved.
Audit tokens and supported-pair counts are included in the report.

Repeated-case denominators include errors. Unique-formula metrics require all
repeats to match. Consistency requires at least two usable, identical outputs;
singletons are not evidence. Timing includes first-call samples from one loaded
worker; p95 uses nearest rank. This is not an independent-cold-start benchmark.

Fresh runs add evaluator/module/executable hashes and normalized backend agreement.
An interpreter hash does not identify its worker script: declare scripts as config
artifacts. Hashes do not prove a source revision. Failed backends have null metrics.

Offline rescoring (no models or artifact paths required):

```bash
python scripts/lfe_equation_metrics.py INPUT_REPORT.json NEW_REPORT.json
```

The output must not exist. Original raw metrics, predictions, timings and comparisons
are preserved. The derived report records offline mode, no new inference, and
source-report/scorer hashes. It enriches completed-backend scores only, not existing
cross-backend comparisons. PDF detection, truncation and memory remain unmeasured.

Tests (no models):

```bash
python -m unittest discover -s scripts -p 'test_lfe_equation_[em]*.py' -v
```
