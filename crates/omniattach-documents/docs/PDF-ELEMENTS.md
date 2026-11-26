# PDF elements: tables, images and equations

LFE ("lopdf floating elements") adds opt-in extraction of non-prose elements from
native PDFs. Ordinary text extraction (`extract`, `--goal markdown`) is not
affected; every feature below is requested explicitly. Nothing runs a model or
OCR unless you ask for it (`--ocr`, an equation worker).

| Element | Library entry point | CLI | Status |
|---|---|---|---|
| Inspection | `inspect_pdf`, `inspect_pdf_bytes` | `pdf-inspect` example | Stable base |
| Images | `extract_pdf_with_images` | `convert --goal custom:pdf-images` | JPEG, JPEG 2000, 8-bit Gray/RGB rasters |
| Equations | `pdf_equations::recognize_image_equations` | `convert --goal custom:pdf-equations` | Selected image blocks + external worker |
| Tables | `pdf_tables::{discover_pdf_tables, convert_pdf_table}`, `pdf_tables::ocr` | `pdf-tables`, `pdf-table` (`--ocr`) | Ruled grids, rule-only/open/rotated layouts, scanned pages via OCR |

All features need the `pdf` Cargo feature (enabled by default in the CLI).

## How it fits together

```text
PDF bytes
  └─ pdf/lines.rs        content-stream interpreter (text runs, fonts, Forms)
       ├─ ordinary extraction     → Document (paragraphs)
       └─ inspection              → PdfInspection (elements + estimated bounds)
            ├─ naming plan        → stable file names for elements
            ├─ pdf_images         → Document with image blocks + assets
            │    └─ pdf_equations → image blocks replaced by LaTeX
            └─ pdf_tables         → Markdown tables from ruled grids
```

`PdfInspection` is the shared base: a list of elements (text runs, image
occurrences, equation/table hints) with one-based page, an operation anchor and
bounds. Bounds are in unnormalized PDF user space (origin bottom-left, CropBox,
Rotate and UserUnit not applied). Text bounds are estimates from font metrics.

## CLI quick reference

```sh
# Tables: list candidates on a page, then convert one
omniattach pdf-tables input.pdf --page 2 --pretty
omniattach pdf-table  input.pdf --page 2 --candidate 1 --output-format markdown
# Scanned page: render + Tesseract (runs pdftoppm and tesseract from PATH)
omniattach pdf-table  scan.pdf --page 1 --candidate 1 --ocr --ocr-language eng --output-format markdown

# Images: Markdown with image links plus an images/ folder
omniattach convert input.pdf --goal custom:pdf-images --output out-images

# Equations: needs a worker (see below) and an external-backends build
export OMNIATTACH_PDF_EQUATION_WORKER_CONFIG=/abs/worker.json
omniattach --external-backends convert input.pdf --goal custom:pdf-equations \
  --allow-external --options-json '{"block_ids":[]}' --output out-list
# document.json lists image block IDs; select the equation images:
omniattach --external-backends convert input.pdf --goal custom:pdf-equations \
  --allow-external --options-json '{"block_ids":["<doc-hash>:b2"]}' --output out-eq
```

Developer examples in this crate:

```sh
cargo run -p omniattach-documents --features pdf --example pdf-inspect -- [--names] input.pdf
cargo run -p omniattach-documents --features pdf --example pdf-images  -- input.pdf NEW_DIR
cargo run -p omniattach-documents --features pdf --example pdf-table   -- input.pdf PAGE 50,150,250 100,130,160
```

`pdf-table` converts a hand-written grid (X and Y boundaries) and is the quickest
way to test text assignment on a new layout before writing a detector for it.

## Tables

### Discovery

`discover_pdf_tables(path, page, options)` returns `PdfTableDiscovery` with
one-based, page-local candidates. Candidate numbers are only valid for the same
input bytes. Strict detectors run first; they only accept drawn grids they can
fully prove:

| Detector | Recognizes | Merged cells |
|---|---|---|
| `stroked_grid` | One stroked path (`S`/`s`) whose segments are all full-span horizontal or vertical rulings, at least three per axis | No |
| `filled_grid` | Thin filled rectangles (`re f`, ≤ 2 units thick); collinear pieces are merged; consecutive rows with the same columns form one band; a full-width heading row becomes its own candidate | No |
| `open_rows` | Fallback when `filled_grid` finds nothing: filled rules with no outer vertical borders and full-height interior dividers | Row spans (missing rule inside a column) |

They decline pages with XObjects (`Do`), clips they cannot prove harmless, mixed
stroked and filled rulings or non-default page Rotate/UserUnit. When they decline
or find nothing, the `layout` detector runs (the reason is kept as a warning):

1. **Rules** (`rules.rs`): every visible axis-aligned rule, whatever the painting
   style: stroked lines, thin filled rectangles, edges of shaded cells and rows
   (also multi-cell fill paths), cut to the clip and the MediaBox. White paint and
   images are ignored; Form XObject content is not read.
2. **Frames** (`frame.rs`): the page is tried upright and, if at least 20% of its
   text runs up or down the page, rotated by 90 or 270 degrees. Inspection now
   gives rotated runs correct bounds and a `text_rotation` angle.
3. **Tables** (`layout.rs`): horizontal rules with the same extent form a table
   region; bands between rules that hold prose split it; a one-line caption
   crossing the columns is dropped. Columns come from interior vertical rules, or
   else from whitespace shared by the lines. Rows come from the rules; inside a
   tall band a new row starts at a vertical gap, when the first column starts
   again, or on every line for dense data. The header band stays one row. Column
   rules running past the last rule (a table cut by the page edge) close the
   table where they end. Empty outer columns are merged away. A region needs two
   rows with text in two columns (one with drawn column rules).

All thresholds are relative to the median text height. Layout candidates carry
`rotation` (0, 90 or 270; boundaries are in the rotated frame: 90 maps `(x, y)`
to `(y, -x)`, 270 to `(-y, x)`) and convert with `text_assignment: centre`.

### Scanned pages (OCR)

`pdf_tables::ocr` runs the same layout detector on a scanned page. The crate
starts no processes; the caller supplies a `PdfTableOcrPage`: the rendered page as
a `GrayRaster` (PGM/PPM parser included), its DPI and OCR words with pixel boxes.
Rules are long thin lines darker than both sides (so shading is not a rule);
`GrayRaster::for_ocr` flattens light shading and erases rules before OCR, which
keeps Tesseract from dropping shaded rows. Words below confidence 0 are dropped
and ruling fragments (`| _ [ ]`) are trimmed. `discover_ocr_tables` and
`convert_ocr_table` mirror the native functions; candidates use detector
`ocr_layout` and `source_sequences` index the OCR word list.

The CLI `--ocr` flag (feature `table-ocr`, on by default) does the whole round
trip with `pdftoppm -gray` and `tesseract ... tsv` (`--ocr-language`,
`--ocr-dpi`, default 300). Native discovery on a page without text warns that the
OCR path is needed.

### Conversion

`convert_pdf_table(path, page, candidate, options)` re-discovers the candidate,
inspects the same bytes and calls `convert_inspection`. You can also call
`convert_inspection(&report, &grid, &options)` with any `PdfTableGrid`.

`text_assignment: contained` (default, used for drawn grids):

- Horizontally the run must lie fully inside one column; otherwise the
  conversion fails with "text crosses grid boundary".
- Vertically the run's centre selects the row. Estimated bounds may spill over
  the row by at most min(1.5 units, 15% of text height) and are clamped. A small
  incursion from outside the table is ignored. Larger overlaps fail.
- For merged cells a centre exactly on a missing rule belongs to the merged cell.

`text_assignment: centre` (inferred grids): each run goes to the cell holding its
centre; runs centred outside the grid are skipped. Both modes then:

- Inside a cell, group runs into lines (subscripts and superscripts stay
  inline), order them top-down and left-to-right, and join lines with `<br>`.
- Markdown/HTML-sensitive characters are written as entities. The first row is
  always the Markdown header.

The result `PdfTableConversion` (schema `omniattach/pdf-table/0.1`) holds the
source name and SHA-256, the grid (including `cell_spans`, `rotation` and
`text_assignment` when not default), the sequences of the placed text runs, the
Markdown and warnings.

### Adding a detector

1. Reproduce the layout as a generated PDF in `tests/pdf_tables.rs`, using the
   `pdf()` helper and a raw content string.
2. Check the text assignment first with the `pdf-table` example and a hand-made
   grid. If that already fails, the problem is in conversion, not discovery.
3. Decide where it belongs. A new drawn-grid style is a strict detector: add a
   module next to `stroked.rs` / `filled.rs`, use `paths::scan` for transformed
   painted paths and return `PdfTableGrid` values. Anything inferred from text
   (new row/column heuristics, other rule sources) extends `layout.rs`, which
   works on plain rules and boxes and so serves native and OCR pages alike.
4. Add a `PdfTableDetector` variant if needed and route to it in `discovery.rs`.
5. Run the real-PDF corpus (see Testing) to check nothing else changed.

## Images

`extract_pdf_with_images(name, bytes, &options)` returns a `Document` whose
blocks interleave paragraphs and image blocks in content order, plus image
assets named from the naming plan (`<slug>-<hash8>-fe-000001.png`). Supported:
`DCTDecode` (JPEG) and `JPXDecode` (JPEG 2000) streams are copied unchanged;
unfiltered or `FlateDecode` 8-bit DeviceGray/DeviceRGB rasters (including PNG
predictors) are written as PNG. Masks, custom `Decode`, other color spaces and
inline images are skipped with a `PDF_IMAGE_SKIPPED` diagnostic. Text between
images uses the same line builder as ordinary extraction but no furniture
removal or dehyphenation. Output schema: `omniattach-documents/0.2`.

## Equations

Equations are recognized from selected image blocks of the image projection.
Native-text and vector-outline formulas are not recognized yet.

- Library: implement `EquationRecognizer` (or pass a closure) and call
  `recognize_image_equations(&doc, &block_ids, &mut recognizer, &limits)`. The
  returned `EquationDocument` renders successful selections as display LaTeX
  and keeps the image as fallback otherwise (`omniattach/pdf-equations/0.1`).
- CLI: a worker process is configured by JSON and registered only with
  `--external-backends`. Every file is hash-checked before running:

```json
{
  "argv": ["/abs/lfe-formula-bobine", "/abs/encoder.onnx", "/abs/decoder.onnx", "/abs/tokenizer.json", "512"],
  "env": {"ORT_DYLIB_PATH": "/abs/libonnxruntime.so"},
  "timeout_ms": 600000,
  "artifacts": [{"path": "/abs/lfe-formula-bobine", "sha256": "..."}, {"path": "/abs/encoder.onnx", "sha256": "..."}]
}
```

The reference worker is `tools/lfe-formula-bobine` (TexTeller ONNX via Bobine),
built separately with `cargo build --release --manifest-path tools/lfe-formula-bobine/Cargo.toml`.
Models can be fetched with the facade's `provision-equation-model` example (see
`crates/omniattach/MODEL_PROVISIONING.md`). The worker protocol is
`omniattach/formula-worker/0.1`: a JSON request `{images:[{id,path}]}` in, one
`{id,status,latex}` result per image out.

## Limits

Existing `Options::limits` bound input bytes, decompression, elements, pages and
operations. Table code reuses them (`cells`, `blocks`, `archive_bytes` for text,
`archive_entries` for path segments, `xml_nodes` for content operations) and adds
fixed caps: 257 boundaries per axis, 16,384 cells, 128 candidates per page, 8 MiB
table text. These are work bounds, not an OS sandbox; use isolated workers for
untrusted input.

## Source map

| File | Purpose |
|---|---|
| `src/pdf.rs` | Ordinary extraction, outlines, OCR hook, inspection entry points |
| `src/pdf/lines.rs` | Content interpreter, line builder, furniture removal |
| `src/pdf/lines/candidates.rs` | Inspection element collector and equation/table hints |
| `src/lfe_names.rs` | Deterministic element names |
| `src/pdf_images.rs`, `src/pdf_images/{raster,text}.rs` | Image projection, raster to PNG, text between images |
| `src/pdf_equations.rs` | Equation substitution over image blocks |
| `src/pdf_tables.rs` | Table types and Markdown rendering |
| `src/pdf_tables/{discovery,paths,stroked,filled,open_rows}.rs` | Strict candidate discovery |
| `src/pdf_tables/{rules,frame,layout}.rs` | Layout detector: rules, rotated frames, table inference |
| `src/pdf_tables/ocr.rs` | Scanned pages: raster rules, OCR words, PGM parsing |
| `src/pdf_tables/{convert,cell_lines}.rs` | Text-to-cell assignment and in-cell line order |

## Testing

```sh
cargo test -p omniattach-documents --features pdf                 # unit + generated-PDF tests
cargo test -p omniattach-cli --test pdf_table_commands           # CLI subprocess tests
```

Opt-in suites (ignored by default) need local data:

```sh
# Equation corpus generated by Python (see scripts/README.md)
python3 scripts/equations/fixtures.py /tmp/eq && python3 scripts/equations/validate_fixtures.py /tmp/eq
OMNIATTACH_EQUATION_CORPUS=/tmp/eq cargo test -p omniattach-documents --features pdf --test lfe_equation_corpus -- --ignored

# Private real-PDF table regressions (TABLE_SAMPLES: folder with 03-...-12-... samples)
OMNIATTACH_FILLED_TABLE_SAMPLE=/path/01-evibaseprac.pdf OMNIATTACH_OPEN_ROWS_SAMPLE=/path/02-healthbig.pdf \
OMNIATTACH_TABLE_SAMPLES=/path/samples \
  cargo test -p omniattach-documents --features pdf --test pdf_tables -- --ignored
# OCR round trip (needs pdftoppm and tesseract)
OMNIATTACH_TABLE_SAMPLES=/path/samples cargo test -p omniattach-cli --test pdf_table_commands -- --ignored
```

For a folder of real PDFs, `scripts/tables/table_corpus.py` runs discovery and
conversion on every page and writes a report (optionally compared with MinerU).

## Known gaps

Tables: column spans, merged cells in layout tables (a spanning or perpendicular
label lands in one row), cross-page continuation, Form XObject content, borderless
tables without any horizontal rule, skewed scans (no deskewing), forms that are
ruled like tables (they produce layout candidates), small OCR digits. Equations:
recognition of native-text or vector formulas.
