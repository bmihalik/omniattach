#!/usr/bin/env python3
"""Open-sided row-span CLI regressions. User sample stays local."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import unicodedata
from test_lfe_step5 import pdf_bytes
from test_table_corpus import markdown_rows

BINARY = None
SAMPLE = None


def content():
    s = "0 0 600 600 re W n\n"
    for y in (100, 130, 190, 220):
        s += f"50 {y} 400 0.5 re f\n"
    s += "130 160 320 0.5 re f\n"
    for x in (130, 230, 300):
        s += f"{x} 100 0.5 120.5 re f\n"
    s += "BT /F1 10 Tf "
    for x, y, text in [(55,205,"Kind"),(135,205,"App"),(235,205,"Country"),(305,205,"Function"),(55,175,"Group"),(55,145,"continued"),(135,175,"A"),(235,175,"UK"),(305,175,"First"),(135,145,"B"),(235,145,"USA"),(305,145,"Second"),(55,115,"Other"),(135,115,"C"),(235,115,"France"),(305,115,"Third")]:
        s += f"1 0 0 1 {x} {y} Tm ({text}) Tj "
    return s + "ET\n"


class OpenRowsCLI(unittest.TestCase):
    def setUp(self):
        if not BINARY:
            self.skipTest("pass --binary to run actual Rust CLI tests")
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.path = Path(temp.name) / "open.pdf"
        self.path.write_bytes(pdf_bytes(content()))

    def invoke(self, command, page=1, candidate=None):
        args = [BINARY, command, str(self.path), "--page", str(page)]
        if candidate is not None:
            args += ["--candidate", str(candidate)]
        return subprocess.run(args, capture_output=True, text=True, timeout=60)

    def get(self, command, page=1, candidate=None):
        result = self.invoke(command, page, candidate)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_synthetic_extracts_spanning_label_once(self):
        listing = self.get("pdf-tables")
        self.assertEqual(listing["schema_version"], "omniattach/pdf-table-discovery/0.2")
        self.assertEqual(len(listing["candidates"]), 1)
        spans = listing["candidates"][0]["cell_spans"]
        self.assertIn({"row": 1, "column": 0, "row_span": 2, "column_span": 1}, spans)
        result = self.get("pdf-table", candidate=1)
        self.assertEqual(markdown_rows(result["markdown"]), [["Kind", "App", "Country", "Function"], ["Group continued", "A", "UK", "First"], ["", "B", "USA", "Second"], ["Other", "C", "France", "Third"]])
        self.assertEqual(len(result["source_sequences"]), len(set(result["source_sequences"])))
        self.assertEqual(result["markdown"].count("continued"), 1)

    def test_partial_divider_does_not_invent_columns(self):
        self.path.write_bytes(pdf_bytes(content().replace("230 100 0.5 120.5", "230 100 0.5 60")))
        self.assertEqual(self.get("pdf-tables")["candidates"], [])

    def test_cutting_clip_remains_rejected(self):
        self.path.write_bytes(pdf_bytes(content().replace("0 0 600 600", "100 0 500 600")))
        result = self.invoke("pdf-tables")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("clipped", result.stderr)

    def test_real_sample_when_supplied(self):
        if not SAMPLE:
            self.skipTest("pass --sample /path/to/02-healthbig.pdf")
        self.path = Path(SAMPLE).resolve()
        names = []
        for page, expected_rows, expected_spans in [(2,21,[1,5,5,10]), (3,14,[1,7,6])]:
            with self.subTest(page=page):
                listing = self.get("pdf-tables", page)
                self.assertEqual(len(listing["candidates"]), 1)
                candidate = listing["candidates"][0]
                self.assertEqual(len(candidate["x_boundaries"]), 5)
                self.assertEqual(len(candidate["y_boundaries"])-1, expected_rows)
                self.assertEqual([s["row_span"] for s in candidate["cell_spans"] if s["column"]==0], expected_spans)
                result = self.get("pdf-table", page, 1)
                rows = markdown_rows(result["markdown"])
                self.assertEqual(len(rows), expected_rows)
                self.assertEqual(rows[0], ["Apps type", "Apps name", "Country", "Introduction and function"])
                self.assertEqual(len(result["source_sequences"]), len(set(result["source_sequences"])))
                names.extend(unicodedata.normalize("NFKC", row[1]) for row in rows[1:] if row[1])
                if page == 2:
                    self.assertEqual(rows[1][1], "iTinnitus [40]")
                    self.assertEqual(rows[-1][1:], ["HealthPROMISE [58]", "", ""])
                else:
                    self.assertEqual(rows[1][:2], ["", ""])
                    self.assertEqual(rows[1][2], "United States")
                    self.assertTrue(rows[1][3].startswith("Patient-engaged care"))
                    self.assertEqual(rows[-1][1], "eCAALYX [37]")
                    self.assertIn("multiple chronic conditions", rows[-1][3])
        self.assertEqual(len(names), 32)
        self.assertEqual(len(names), len(set(names)))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary")
    parser.add_argument("--sample")
    args, remaining = parser.parse_known_args()
    BINARY, SAMPLE = args.binary, args.sample
    unittest.main(argv=[__file__, *remaining], verbosity=2)
