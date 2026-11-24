#!/usr/bin/env python3
"""Filled-border regressions; --binary opts into actual Rust CLI checks."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from test_lfe_step5 import pdf_bytes
import test_table_corpus as corpus

BINARY = None
SAMPLE = None


def content():
    geometry = "0 0 600 600 re W n\n"
    for y in (100, 130, 160, 190):
        geometry += f"50 {y} 200 0.5 re f\n"
    for x in (50, 250):
        geometry += f"{x} 100 0.5 90.5 re f\n"
    geometry += "150 100 0.5 60 re f\n"
    return geometry + "BT /F1 10 Tf 1 0 0 1 60 170 Tm (Section) Tj 1 0 0 1 60 140 Tm (Name) Tj 1 0 0 1 160 140 Tm (Value) Tj 1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj ET\n"


class Reporting(unittest.TestCase):
    def test_exit_and_stderr_are_inline(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "discovery"
            result = subprocess.CompletedProcess(["fake"], 3, "", '{"error":"clipped border"}')
            with patch.object(corpus.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(ValueError, "exit=3.*clipped border"):
                    corpus.run(["fake"], prefix, 1)
            self.assertEqual(json.loads(prefix.with_suffix(".command.json").read_text())["returncode"], 3)

    def test_error_excerpt_bounded_but_log_complete(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "discovery"
            with patch.object(corpus.subprocess, "run", return_value=subprocess.CompletedProcess(["fake"], 3, "", "x" * 10000)):
                with self.assertRaises(ValueError) as caught:
                    corpus.run(["fake"], prefix, 1)
            self.assertLess(len(str(caught.exception)), 2500)
            self.assertEqual(len(prefix.with_suffix(".stderr.txt").read_text()), 10000)

    def test_numeric_entities_match_html(self):
        self.assertEqual(corpus.markdown_rows("| A&#124;B &amp; C |\n| --- |\n"), [["A|B & C"]])


class FilledCLI(unittest.TestCase):
    def setUp(self):
        if BINARY is None:
            self.skipTest("pass --binary to execute Rust regressions")
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.path = Path(temp.name) / "filled.pdf"
        self.path.write_bytes(pdf_bytes(content()))

    def invoke(self, command, *extra):
        return subprocess.run([BINARY, command, str(self.path), "--page", "1", *extra], capture_output=True, text=True, timeout=30)

    def test_heading_and_data_are_separate_candidates(self):
        listing = self.invoke("pdf-tables")
        self.assertEqual(listing.returncode, 0, listing.stderr)
        candidates = json.loads(listing.stdout)["candidates"]
        self.assertEqual([len(c["x_boundaries"]) - 1 for c in candidates], [1, 2])
        for number, expected in [(1, "| Section |\n| --- |\n"), (2, "| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n")]:
            result = self.invoke("pdf-table", "--candidate", str(number))
            self.assertEqual(result.returncode, 0, result.stderr)
            value = json.loads(result.stdout)
            self.assertEqual(value["markdown"], expected)
            self.assertTrue(any("section spans" in warning for warning in value["warnings"]))

    def test_real_sample_when_supplied(self):
        if SAMPLE is None:
            self.skipTest("pass --sample for the user PDF regression")
        self.path = Path(SAMPLE).resolve()
        listing = self.invoke("pdf-tables")
        self.assertEqual(listing.returncode, 0, listing.stderr)
        candidates = json.loads(listing.stdout)["candidates"]
        self.assertEqual(len(candidates), 8)
        codes = set()
        for candidate in candidates:
            result = self.invoke("pdf-table", "--candidate", str(candidate["candidate"]))
            self.assertEqual(result.returncode, 0, result.stderr)
            rows = corpus.markdown_rows(json.loads(result.stdout)["markdown"])
            codes.update(row[0] for row in rows if row[0].isdigit())
        self.assertEqual(codes, set("96150 96151 96152 96153 96154 96155 90901 90911 90875 90876 94010 94400 96002 95957 90806".split()))

    def test_cutting_clip_fails_without_partial_output(self):
        self.path.write_bytes(pdf_bytes(content().replace("0 0 600 600", "100 0 500 600")))
        result = self.invoke("pdf-tables")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("clipped", result.stderr)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary")
    parser.add_argument("--sample")
    args, remaining = parser.parse_known_args()
    BINARY = args.binary
    SAMPLE = args.sample
    unittest.main(argv=[__file__, *remaining], verbosity=2)
