#!/usr/bin/env python3
"""Batch PDF table inspection and optional MinerU differential comparison."""
import argparse
import csv
import difflib
import hashlib
from html import unescape
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unicodedata


def normalize(value):
    return " ".join(unicodedata.normalize("NFC", value).split())


def markdown_rows(text):
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        if not (line.startswith("|") and line.endswith("|")):
            raise ValueError("unsupported Markdown table line")
        cells = re.split(r"(?<!\\)\|", line[1:-1])
        if all(re.fullmatch(r"\s*:?-{3,}:?\s*", c) for c in cells):
            continue
        rows.append([normalize(unescape(c.replace(r"\|", "|").replace("<br />", " ").replace("<br/>", " ").replace("<br>", " "))) for c in cells])
    if not rows or not rows[0] or any(len(r) != len(rows[0]) for r in rows):
        raise ValueError("empty or nonrectangular Markdown table")
    return rows


class HTMLTable(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.rows, self.row, self.cell = [], None, None
        self.depth, self.tables = 0, 0

    def handle_starttag(self, tag, attrs):
        if tag == "table":
            self.depth += 1
            self.tables += 1
            if self.depth > 1 or self.tables > 1:
                raise ValueError("nested/multiple HTML tables unsupported")
        if tag == "tr":
            if self.row is not None:
                raise ValueError("unclosed HTML row")
            self.row = []
        if tag in ("td", "th"):
            if self.cell is not None or self.row is None:
                raise ValueError("malformed HTML cell")
            if any(k in ("rowspan", "colspan") and v != "1" for k, v in attrs):
                raise ValueError("merged reference cells unsupported")
            self.cell = []
        if tag == "br" and self.cell is not None:
            self.cell.append(" ")
        if tag in ("img", "math"):
            raise ValueError("image/math reference cells unsupported")

    def handle_endtag(self, tag):
        if tag in ("td", "th"):
            if self.cell is None or self.row is None:
                raise ValueError("malformed HTML cell ending")
            self.row.append(normalize("".join(self.cell)))
            self.cell = None
        if tag == "tr":
            if self.row is None or self.cell is not None:
                raise ValueError("malformed HTML row ending")
            self.rows.append(self.row)
            self.row = None
        if tag == "table":
            self.depth -= 1

    def handle_data(self, data):
        if self.cell is not None:
            self.cell.append(data)


def html_rows(text):
    parser = HTMLTable()
    parser.feed(text)
    parser.close()
    rows = parser.rows
    if parser.tables != 1 or parser.depth or parser.cell is not None or parser.row is not None:
        raise ValueError("incomplete HTML table")
    if not rows or not rows[0] or any(len(r) != len(rows[0]) for r in rows):
        raise ValueError("empty/nonrectangular reference table")
    return rows


def compare(actual, reference):
    # Deterministic greedy one-to-one matching, exact matches before fuzzy matches.
    pairs = []
    for i, a in enumerate(actual):
        for j, b in enumerate(reference):
            exact = a == b
            same_shape = [len(r) for r in a] == [len(r) for r in b]
            similarity = difflib.SequenceMatcher(None, json.dumps(a, ensure_ascii=False), json.dumps(b, ensure_ascii=False), autojunk=False).ratio()
            pairs.append((exact, same_shape, similarity, i, j))
    used_a, used_b, matches = set(), set(), []
    for exact, shape, score, i, j in sorted(pairs, key=lambda p: (-p[0], -p[1], -p[2], p[3], p[4])):
        if i not in used_a and j not in used_b:
            used_a.add(i)
            used_b.add(j)
            matches.append({"actual_index": i, "reference_index": j, "exact": exact, "same_shape": shape, "similarity": round(score, 6)})
    missing = [j for j in range(len(reference)) if j not in used_b]
    extra = [i for i in range(len(actual)) if i not in used_a]
    status = "no_tables" if not actual and not reference else "agree"
    if missing or extra or any(not m["exact"] for m in matches):
        status = "different"
    return {"status": status, "matches": matches, "unmatched_actual": extra, "unmatched_reference": missing}


def page_statuses(record, reference_requested):
    record["extraction_status"] = "error" if record["extraction_errors"] else "success"
    if not reference_requested:
        comparison = "not_requested"
    elif record["extraction_errors"]:
        comparison = "blocked"
    elif record["reference_errors"]:
        comparison = "error"
    elif record["unsupported_reference"]:
        comparison = "inconclusive"
    else:
        comparison = record["comparison"]["status"]
    record["comparison_status"] = comparison
    legacy = record.get("comparison", {}).get("status", "not_requested")
    record["status"] = "error" if record["errors"] else "inconclusive" if record["unsupported_reference"] else legacy


def run(command, prefix, timeout, env=None):
    started = time.monotonic()
    meta = {"argv": command}
    stdout = stderr = ""
    try:
        result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout, env=env)
        stdout, stderr = result.stdout, result.stderr
        meta["returncode"] = result.returncode
    except (OSError, subprocess.TimeoutExpired) as error:
        meta["error"] = str(error)
        stdout, stderr = getattr(error, "stdout", "") or "", getattr(error, "stderr", "") or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", "replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", "replace")
    meta["seconds"] = round(time.monotonic() - started, 3)
    prefix.with_suffix(".stdout.txt").write_text(stdout, encoding="utf-8")
    prefix.with_suffix(".stderr.txt").write_text(stderr, encoding="utf-8")
    prefix.with_suffix(".command.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    if meta.get("returncode") != 0:
        detail = " ".join((stderr.strip() or meta.get("error", "no stderr")).split())
        if len(detail) > 2000:
            detail = detail[:2000] + " [truncated; see logs]"
        raise ValueError(f"command failed (exit={meta.get('returncode', 'not available')}): {detail}; see {prefix}.command.json")
    return stdout


def mineru_tables(directory):
    paths = sorted(directory.rglob("*_content_list.json"))
    if len(paths) != 1:
        raise ValueError(f"expected one MinerU V1 *_content_list.json, found {len(paths)}; configure --mineru-command for your version")
    items = json.loads(paths[0].read_text(encoding="utf-8"))
    if not isinstance(items, list) or any(not isinstance(x, dict) for x in items):
        raise ValueError("unsupported MinerU content-list schema; expected V1 flat list")
    tables, unsupported = [], []
    for index, item in enumerate(items):
        if item.get("type") != "table":
            continue
        if item.get("page_idx") != 0 or isinstance(item.get("page_idx"), bool):
            raise ValueError("expected page_idx=0 for a single-page MinerU input")
        try:
            rows = html_rows(item["table_body"])
        except (KeyError, TypeError, ValueError) as error:
            unsupported.append({"block_index": index, "reason": str(error), "block": item})
        else:
            tables.append({"block_index": index, "rows": rows})
    return tables, unsupported


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--binary", default="omniattach", help="Built executable path or name on PATH")
    parser.add_argument("--recursive", action="store_true")
    parser.add_argument("--output", type=Path, help="New, non-existing report directory")
    parser.add_argument("--mineru", action="store_true", help="Run the optional reference extractor (may download models)")
    parser.add_argument("--mineru-command", default='["mineru", "-p", "{input}", "-o", "{output}"]', help="JSON argv template; legacy CLI default; both placeholders required")
    parser.add_argument("--timeout", type=float, default=120, help="Seconds per omniattach invocation")
    parser.add_argument("--mineru-timeout", type=float, default=1800)
    parser.add_argument("--fail-on-difference", action="store_true")
    args = parser.parse_args(argv)
    if not args.folder.is_dir() or args.timeout <= 0 or args.mineru_timeout <= 0:
        parser.error("folder must exist and timeouts must be positive")
    try:
        from pypdf import PdfReader, PdfWriter
    except ImportError:
        parser.error("install the runner dependency: python3 -m pip install pypdf")
    try:
        template = json.loads(args.mineru_command)
        if not isinstance(template, list) or not template or not all(isinstance(x, str) for x in template):
            raise ValueError("expected nonempty JSON array of strings")
        if not any("{input}" in x for x in template) or not any("{output}" in x for x in template):
            raise ValueError("command needs {input} and {output}")
    except (ValueError, TypeError) as error:
        parser.error(str(error))
    folder = args.folder.resolve()
    files = sorted(p for p in (folder.rglob("*") if args.recursive else folder.iterdir()) if p.is_file())
    pdfs = [p for p in files if p.suffix.lower() == ".pdf"]
    if not pdfs:
        parser.error("no PDF files found")
    if args.output:
        args.output.mkdir(parents=True, exist_ok=False)
        output = args.output.resolve()
    else:
        output = Path(tempfile.mkdtemp(prefix="table-corpus-", dir=Path.cwd())).resolve()
    report = {"schema": "omniattach/table-corpus/0.1", "folder": str(folder), "reference_requested": args.mineru,
              "skipped_files": [str(p.relative_to(folder)) for p in files if p.suffix.lower() != ".pdf"], "documents": []}
    for index, path in enumerate(pdfs, 1):
        relative = str(path.relative_to(folder))
        print(f"[{index}/{len(pdfs)}] {relative}", flush=True)
        directory = output / f"document-{index:04d}"
        directory.mkdir()
        doc = {"file": relative, "pages": [], "errors": []}
        report["documents"].append(doc)
        try:
            doc["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
            reader = PdfReader(str(path))
            if reader.is_encrypted and not reader.decrypt(""):
                raise ValueError("encrypted PDF requires a password; not supported")
            doc["page_count"] = len(reader.pages)
            for page in range(1, len(reader.pages) + 1):
                work = directory / f"page-{page:04d}"
                work.mkdir()
                record = {"page": page, "actual": [], "reference": [], "unsupported_reference": [], "errors": [], "extraction_errors": [], "reference_errors": []}
                doc["pages"].append(record)
                base = [args.binary, "pdf-tables", str(path), "--page", str(page)]
                try:
                    listing = json.loads(run(base, work / "discovery", args.timeout))
                    candidates = listing["candidates"]
                    record["discovery"] = listing
                    if not isinstance(candidates, list):
                        raise ValueError("invalid candidate list")
                    for candidate in candidates:
                        number = candidate["candidate"]
                        if type(number) is not int or number < 1:
                            raise ValueError("invalid candidate number")
                        try:
                            value = json.loads(run([args.binary, "pdf-table", str(path), "--page", str(page), "--candidate", str(number)], work / f"candidate-{number}", args.timeout))
                            record["actual"].append({"candidate": number, "rows": markdown_rows(value["markdown"]), "conversion": value})
                        except (ValueError, KeyError, TypeError) as error:
                            message = f"candidate {number}: {error}"
                            record["errors"].append(message)
                            record["extraction_errors"].append(message)
                except (ValueError, KeyError, TypeError) as error:
                    message = f"discovery: {error}"
                    record["errors"].append(message)
                    record["extraction_errors"].append(message)
                if args.mineru:
                    try:
                        single = work / "reference-input.pdf"
                        writer = PdfWriter()
                        writer.add_page(reader.pages[page - 1])
                        with single.open("wb") as stream:
                            writer.write(stream)
                        reference_dir = work / "mineru"
                        reference_dir.mkdir()
                        command = [x.replace("{input}", str(single)).replace("{output}", str(reference_dir)) for x in template]
                        run(command, work / "reference", args.mineru_timeout)
                        record["reference"], record["unsupported_reference"] = mineru_tables(reference_dir)
                        record["comparison"] = compare([t["rows"] for t in record["actual"]], [t["rows"] for t in record["reference"]])
                    except Exception as error:
                        message = f"reference: {type(error).__name__}: {error}"
                        record["errors"].append(message)
                        record["reference_errors"].append(message)
                page_statuses(record, args.mineru)
                (work / "result.json").write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding="utf-8")
        except Exception as error:
            doc["errors"].append(f"{type(error).__name__}: {error}")
    statuses = [p["status"] for d in report["documents"] for p in d["pages"] if "status" in p]
    report["summary"] = {s: statuses.count(s) for s in sorted(set(statuses))}
    for field in ("extraction", "comparison"):
        values = [p[field + "_status"] for d in report["documents"] for p in d["pages"] if field + "_status" in p]
        report[field + "_summary"] = {s: values.count(s) for s in sorted(set(values))}
    report["document_error_count"] = sum(bool(d["errors"]) for d in report["documents"])
    errors = any(d["errors"] or any(p["errors"] for p in d["pages"]) for d in report["documents"])
    report["operational_errors"] = errors
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    with (output / "summary.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["file", "page", "status", "actual_tables", "reference_tables", "unsupported_reference", "errors", "extraction_status", "comparison_status"])
        for doc in report["documents"]:
            for page in doc["pages"]:
                writer.writerow([doc["file"], page["page"], page.get("status", "error"), len(page["actual"]), len(page["reference"]), len(page["unsupported_reference"]), " ; ".join(page["errors"]), page.get("extraction_status", "error"), page.get("comparison_status", "blocked" if args.mineru else "not_requested")])
            if doc["errors"]:
                writer.writerow([doc["file"], "", "error", "", "", "", " ; ".join(doc["errors"]), "error", "blocked" if args.mineru else "not_requested"])
    print(f"Report: {output}\nExtraction (pages): {json.dumps(report['extraction_summary'])}\nComparison (pages): {json.dumps(report['comparison_summary'])}\nDocument errors: {report['document_error_count']}")
    return 1 if errors else 2 if args.fail_on_difference and any(s in ("different", "inconclusive") for s in statuses) else 0


if __name__ == "__main__":
    sys.exit(main())
