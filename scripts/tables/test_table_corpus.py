#!/usr/bin/env python3
"""Unit and mocked integration tests; no Rust build or MinerU model required."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import table_corpus as corpus

MD = "| A | B |\n| --- | --- |\n| One | 42 |\n"
HTML = "<table><tr><th>A</th><th>B</th></tr><tr><td>One</td><td>42</td></tr></table>"


class CorpusTests(unittest.TestCase):
    def test_formats_normalize_equally(self):
        self.assertEqual(corpus.markdown_rows(MD), corpus.html_rows(HTML))

    def test_normalization_is_conservative(self):
        self.assertEqual(corpus.normalize(" e\u0301\u00a0 a\n"), "é a")
        self.assertNotEqual(corpus.normalize("A"), corpus.normalize("a"))
        self.assertNotEqual(corpus.normalize("1,000"), corpus.normalize("1000"))

    def test_html_entities_and_breaks(self):
        self.assertEqual(corpus.html_rows("<table><tr><td>A&amp;B<br>C</td></tr></table>"), [["A&B C"]])

    def test_unsupported_html_is_not_flattened(self):
        for html in ["<table><tr><td colspan='2'>A</td></tr></table>", "<table><tr><td><img src='x'></td></tr></table>", "<table><tr><td>A"]:
            with self.subTest(html=html), self.assertRaises(ValueError):
                corpus.html_rows(html)

    def test_escaped_pipe_and_shape(self):
        self.assertEqual(corpus.markdown_rows("| A\\|B |\n| --- |\n"), [["A|B"]])
        with self.assertRaises(ValueError):
            corpus.markdown_rows("| A | B |\n| C |\n")

    def test_reordered_tables_and_duplicates(self):
        a, b = [["a"]], [["b"]]
        self.assertEqual(corpus.compare([a, b], [b, a])["status"], "agree")
        result = corpus.compare([a], [a, a])
        self.assertEqual(result["status"], "different")
        self.assertEqual(len(result["unmatched_reference"]), 1)
        self.assertEqual(corpus.compare([], [a])["status"], "different")
        self.assertEqual(corpus.compare([], [])["status"], "no_tables")

    def test_changed_content_is_not_agreement(self):
        result = corpus.compare([[["42"]]], [[["43"]]] )
        self.assertEqual(result["status"], "different")
        self.assertFalse(result["matches"][0]["exact"])

    def test_reference_missing_or_ambiguous_is_error(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            with self.assertRaises(ValueError):
                corpus.mineru_tables(directory)
            (directory / "a_content_list.json").write_text("[]")
            self.assertEqual(corpus.mineru_tables(directory), ([], []))
            (directory / "b_content_list.json").write_text("[]")
            with self.assertRaises(ValueError):
                corpus.mineru_tables(directory)

    def test_timeout_preserves_logs(self):
        with tempfile.TemporaryDirectory() as temp:
            prefix = Path(temp) / "command"
            with patch.object(corpus.subprocess, "run", side_effect=subprocess.TimeoutExpired(["fake"], 1, output=b"partial")):
                with self.assertRaises(ValueError):
                    corpus.run(["fake"], prefix, 1)
            self.assertEqual(prefix.with_suffix(".stdout.txt").read_text(), "partial")
            self.assertIn("error", json.loads(prefix.with_suffix(".command.json").read_text()))

    def test_folder_mocked_integration(self):
        from pypdf import PdfWriter
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "documents"
            source.mkdir()
            writer = PdfWriter()
            writer.add_blank_page(width=600, height=600)
            writer.add_blank_page(width=600, height=600)
            with (source / "table.PDF").open("wb") as stream:
                writer.write(stream)
            (source / "notes.txt").write_text("skip me")
            (source / "broken.pdf").write_bytes(b"broken")
            calls = []
            def fake_run(command, prefix, timeout, env=None):
                calls.append(command)
                if command[0] == "reference":
                    output = Path(command[-1])
                    (output / "page_content_list.json").write_text(json.dumps([{"type": "table", "page_idx": 0, "table_body": HTML}]))
                    return ""
                if command[1] == "pdf-tables":
                    return json.dumps({"candidates": [{"candidate": 1}]})
                return json.dumps({"markdown": MD})
            output = root / "results"
            with patch.object(corpus, "run", side_effect=fake_run), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                code = corpus.main([str(source), "--output", str(output), "--mineru", "--mineru-command", '["reference", "{input}", "{output}"]'])
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(code, 1)  # Broken file must not prevent the valid PDF being tested.
            self.assertEqual(report["summary"], {"agree": 2})
            self.assertEqual(report["skipped_files"], ["notes.txt"])
            self.assertEqual(len(calls), 6)
            self.assertTrue((output / "summary.csv").is_file())


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

    def test_extraction_and_comparison_statuses_are_independent(self):
        cases = [
            (False, {}, ("success", "not_requested")),
            (True, {"comparison": {"status": "agree"}}, ("success", "agree")),
            (True, {"comparison": {"status": "different"}}, ("success", "different")),
            (True, {"extraction_errors": ["bad"], "errors": ["bad"]}, ("error", "blocked")),
            (True, {"reference_errors": ["bad"], "errors": ["bad"]}, ("success", "error")),
            (True, {"unsupported_reference": [{}]}, ("success", "inconclusive")),
        ]
        for requested, extra, expected in cases:
            with self.subTest(expected=expected):
                page = {"extraction_errors": [], "reference_errors": [], "errors": [], "unsupported_reference": [], **extra}
                corpus.page_statuses(page, requested)
                self.assertEqual((page["extraction_status"], page["comparison_status"]), expected)


if __name__ == "__main__":
    unittest.main(verbosity=2)
