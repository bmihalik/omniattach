# LFE-4B: generated-corpus extraction checks

Adds five opt-in Rust integration tests against LFE-4A PDFs, plus a dedicated CI workflow. No production Rust code, dependency, schema, model, version, merge or publication changes. The user reported LFE-4A tests passing; that is not evidence that this new target passed.

## Run from the repository root

Install scripts/requirements-lfe-equations.txt in a Python environment first. Choose a NEW output directory; generation never overwrites an existing one.

```sh
python scripts/lfe_equation_fixtures.py /tmp/lfe4b-equations
python scripts/validate_lfe_equation_fixtures.py /tmp/lfe4b-equations
OMNIATTACH_EQUATION_CORPUS=/tmp/lfe4b-equations \
  cargo test --locked -p omniattach-documents --features pdf \
  --test lfe_equation_corpus -- --ignored --nocapture
```

Tests are deliberately ignored in ordinary cargo test: without the Python-generated corpus, they are NOT reported as successful executions. The explicit command and dedicated workflow execute them. Missing environment/corpus fails rather than silently returning. Python validation must run first; Rust validates source PDF hashes but is not a complete manifest validator. The optional --latex corpus is accepted by inventory checks, but dedicated advanced-formula extraction assertions are not implemented.

## Coverage

| Test | Contract |
|---|---|
| Inventory | Every manifest PDF opens through Rust inspection; source hashes, pages, contiguous sequences, backward text references, unique provisional names and JSON round-trip |
| Raster equations | Occurrence bounds match placement truth; exported PNG pixels equal independently generated RGB PNGs; page/prose/image ordering and Markdown links |
| Reuse/transforms | Two paints retain one shared asset and distinct occurrence anchors; scaled/rotated placement and CropBox/Rotate fixture preserve raw user-space bounds and original asset pixels |
| Scans/outlines | Scans remain page images, not recognized equations; outline-only math does not fabricate image assets or equation recognition |
| Isolation | Ordinary extraction before/after inspection and image projection is identical and remains image-free; source PDF bytes remain unchanged |

Inspection/projection use a panic-on-call OCR callback to enforce no OCR invocation. Ordinary extraction uses default options. Reported equation-hint counts are observations, NOT exact formula recall targets. The negative code assignment may trigger a heuristic hint; zero false positives is not claimed. No region renderer, crop transform adapter, LaTeX recognizer or equation replacement is added.

Ground truth bounds are allocated regions, not ink bounds. Exact bounds assertions apply only to controlled image placements. CropBox/Rotate assertions document current unnormalized user-space inspection, not correct viewer-space cropping. Exported asset pixels intentionally remain unrotated. Scan tests check occurrence/asset structure, not an independent scan-pixel reference. Native text decoding fidelity, precise equation grouping and whole-page reading order remain unmeasured. PDF-byte preservation is not a complete filesystem side-effect audit.

## Verification status

Source APIs and decoder constraints were inspected on lopdf-floating-elements at 7a28f20. Cargo, rustc and rustfmt are unavailable in the preparation environment. These new Rust tests have NOT been compiled or executed here, and CI success is not claimed. Run the command above and cargo fmt --all before relying on the checkpoint. The workflow retains generated fixtures and the Rust test log for review. A failing assertion is a compatibility finding to investigate, not permission to weaken the expectation silently.
