# Scripts

Development tools around PDF element extraction. Python 3.10+; each folder's
tests run without models or a Rust build:

```sh
for d in tables equations images; do python3 -m unittest discover -s scripts/$d -p 'test_*.py'; done
```

Rust behavior is tested with `cargo test`; see
[PDF-ELEMENTS.md](../crates/omniattach-documents/docs/PDF-ELEMENTS.md).

## tables/

`table_corpus.py` runs table discovery and conversion on every page of every PDF
in a folder and writes a fresh report directory (`report.json`, `summary.csv`,
per-document logs). Use it to check a new detector against real documents.

```sh
python3 -m pip install pypdf
cargo build -p omniattach-cli
python3 scripts/tables/table_corpus.py /path/to/pdfs --binary target/debug/omniattach [--recursive]
# Optional reference comparison with a separately installed MinerU:
python3 scripts/tables/table_corpus.py /path/to/pdfs --binary target/debug/omniattach --mineru
```

Extraction status and comparison status are reported separately; a page can
extract successfully and still differ from the reference.

## equations/

- `fixtures.py OUT_DIR` generates a synthetic equation corpus (native text,
  outlines, rasters, reused/rotated images, scans and negatives) with a ground
  truth manifest. `--latex` adds pdflatex formulas if TeX is installed.
- `validate_fixtures.py OUT_DIR` checks the corpus independently (pypdf, PDFium).
- `recognition_eval.py CORPUS CONFIG.json NEW_REPORT.json` runs one or more
  formula workers (protocol `omniattach/formula-worker/0.1`) on the clean formula
  images and scores exact matches, agreement and timing.
- `recognition_metrics.py IN.json NEW.json` re-scores a report offline with
  formatting-tolerant LaTeX comparison.

```sh
python3 -m pip install -r scripts/equations/requirements.txt
python3 scripts/equations/fixtures.py /tmp/eq
python3 scripts/equations/validate_fixtures.py /tmp/eq
OMNIATTACH_EQUATION_CORPUS=/tmp/eq cargo test -p omniattach-documents --features pdf \
  --test lfe_equation_corpus -- --ignored
```

Worker config for `recognition_eval.py`:
`{"backends":[{"id":"bobine","argv":[...],"artifacts":[{"path":"...","sha256":"..."}]}]}`.

## images/

For a real PDF, export both ordinary and image-aware output, then audit:

```sh
omniattach convert book.pdf --goal markdown --output plain
omniattach convert book.pdf --goal custom:pdf-images --output images
python3 scripts/images/audit_export.py images --reference plain --report audit.json
python3 scripts/images/compare_export_text.py images --reference plain
```

`audit_export.py` checks that every image file, JSON asset and Markdown link
agree (paths, sizes, hashes) and summarizes diagnostics. `compare_export_text.py`
aligns paragraph text between the two exports to show where prose differs.

## Other

- `smoke-net.sh` downloads a few public samples and converts them (network).
- `test_pdf_ocr.sh` checks the OCR route; needs pdftoppm, tesseract, ImageMagick.
