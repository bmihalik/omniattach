# Real-PDF table corpus runner

From the repository root:

```sh
python3 -m pip install pypdf
cargo build --locked -p omniattach-cli --bin omniattach
python3 scripts/test_table_corpus.py /path/to/table-pdfs --binary ./target/debug/omniattach
```

Use your actual executable path if Cargo uses a non-default target directory.
On Windows use the built omniattach.exe path. The runner does not build Rust or
install MinerU. Its only third-party Python dependency is pypdf.

The positional argument is a folder of PDF documents. Every page is inspected,
then every discovered candidate is converted. Shortened/excerpted PDFs are fine;
reported page numbers refer to the supplied excerpts, not the original books.
Extensions are case-insensitive. Other file types are listed as skipped, not
silently treated as tested documents. Use --recursive to include subfolders.
Input PDFs are not modified. Encrypted PDFs requiring passwords are reported as
errors. The batch continues after individual file, page and candidate failures.

By default a fresh table-corpus-* report directory is created in the current
working directory. Alternatively pass --output /path/to/new-report-directory;
it must not already exist. Each run starts fresh, with no stale result reuse.

## Optional MinerU reference

Install/provision MinerU separately, then opt in:

```sh
python3 scripts/test_table_corpus.py /path/to/table-pdfs --binary ./target/debug/omniattach --mineru
```

The default reference argv is the legacy CLI form:

```text
mineru -p SINGLE_PAGE_PDF -o OUTPUT_DIRECTORY
```

MinerU CLI/output formats vary by version. The adapter deliberately consumes
Content List V1: exactly one *_content_list.json file per reference invocation,
containing a flat list with type=table, page_idx=0, and HTML in table_body.
Missing/ambiguous outputs or a different schema are errors, not zero-table passes.
An empty valid V1 list is a valid zero-table reference result. Configure a command
that generates V1 for your installed version via a JSON argv array, for example
if that version requires an explicit content-list export flag:

```sh
python3 scripts/test_table_corpus.py /path/to/table-pdfs --binary ./target/debug/omniattach --mineru --mineru-command '["mineru","-p","{input}","-o","{output}","--dump-content-list"]'
```

Check mineru --help before choosing flags; this example is not universal.
Both placeholders are required. Substitution is literal, with no shell evaluation;
paths with spaces remain single arguments. Custom commands are trusted local code.
No reference is invoked unless --mineru is present. MinerU may download models,
use substantial CPU/GPU resources, or contact services depending on installation
and command configuration. The runner is not a network or process sandbox.

The runner writes a single-page reference PDF for each original input page. This
keeps comparison page-local and avoids cross-page reference table merging, but
adds startup cost and may change document-level context/annotations. It is intended
for the user's short excerpts, not a throughput benchmark. Default timeouts are
120 seconds per omniattach call and 1800 seconds per MinerU call; override with
--timeout and --mineru-timeout. Captured output is held in memory; only run trusted
local executables. Timeout handling targets the invoked process, not an arbitrary
descendant process tree.

## Outputs and comparison

- report.json: document hashes, page counts, skipped files, original discovery and
  conversion results, normalized tables, reference blocks/limitations, pairings,
  differences and operational errors.
- summary.csv: one row per page, plus document-level failures.
- document-N/page-N/: per-invocation argv, elapsed time, exit status, stdout/stderr,
  per-page result.json, and (when requested) reference PDFs and MinerU artifacts.

Normalization uses Unicode NFC and whitespace collapse, retaining case, punctuation,
numbers, row/column order and empty cells. HTML entities/line breaks and Markdown
escaped pipes are decoded. Rectangular HTML tables only: merged cells, nested
 tables, image/math cells and malformed reference tables are retained as unsupported
reference blocks rather than silently flattened. Normalization is not semantic
numeric equivalence and may still flag harmless formatting differences.

Within a page, matching is deterministic greedy one-to-one: exact table equality
first, then equal shape, then character similarity. Table order may differ between
extractors; row/column order within a table may not. Duplicate tables retain their
multiplicity. Fuzzy pairings are review aids, not proof of correspondence; similarity
is a character comparison of normalized JSON, not an accuracy percentage, TEDS,
or an optimal global assignment. No geometry-based correspondence is claimed.

Statuses: agree (all normalized tables exactly paired), different, no_tables,
not_requested, inconclusive (unsupported reference tables), or error. Page-level
status takes precedence over any partial comparison. no_tables does not establish
that the page truly has no tables. Without MinerU there is no quality verdict.
MinerU is a fallible reference, not ground truth; scans, borderless tables and merged
cells can exceed the current omniattach scope and should be reviewed as such.

Exit codes: 0 for completed runs (differences retained for review), 1 for any
operational/extraction error, 2 for differences/inconclusive pages when
--fail-on-difference is set. Argument errors also use argparse's exit code 2.
Errors take precedence over differences. Keep reference model/version/backend and
command settings fixed when comparing runs; the report stores command argv but
does not automatically discover all installed model or executable versions.

## Runner tests

```sh
python3 -m unittest discover -s scripts -p test_table_corpus_unit.py -v
```

Ten tests cover normalization, HTML limitations, escaped pipes, shape preservation,
duplicate/reordered matching, mismatches, reference output validation, timeout logs,
and a two-page folder integration with mocked extractors and an unreadable PDF.
These pass in the preparation environment. Real omniattach/MinerU integration has
not been run there and needs validation on your machine and representative PDFs.
