#!/usr/bin/env python3
"""Exercise the real omniattach binary with synthetic PDFs; no third-party Python packages."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_lfe_step5 import ROOT, GRID, TEXT, EXPECTED, pdf_bytes


class MainTableCLI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run(
            ["cargo", "build", "--locked", "-p", "omniattach-cli", "--bin", "omniattach", "--message-format=json"],
            cwd=ROOT, capture_output=True, text=True, timeout=1800,
        )
        if result.returncode:
            raise RuntimeError(result.stdout + "\n" + result.stderr)
        executables = []
        for line in result.stdout.splitlines():
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            target = message.get("target", {})
            if message.get("reason") == "compiler-artifact" and target.get("name") == "omniattach" and "bin" in target.get("kind", []) and message.get("executable"):
                executables.append(Path(message["executable"]))
        if len(set(executables)) != 1 or not executables[0].is_file():
            raise RuntimeError("Cargo did not report exactly one omniattach executable")
        cls.binary = executables[0]

    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="lfe-step5-cli-")
        self.addCleanup(temp.cleanup)
        self.directory = Path(temp.name)
        self.path = self.directory / "table with spaces.pdf"
        self.cache = self.directory / "hf-cache"
        self.cache.mkdir()
        self.env = os.environ.copy()
        self.env.update({
            "HF_HOME": str(self.cache / "home"),
            "HF_HUB_CACHE": str(self.cache / "hub"),
            "HUGGINGFACE_HUB_CACHE": str(self.cache / "hub"),
            "HF_HUB_OFFLINE": "1",
            "TRANSFORMERS_OFFLINE": "1",
        })
        self.fixture()

    def fixture(self, content=GRID + TEXT, page_extra=""):
        self.data = pdf_bytes(content, page_extra)
        self.path.write_bytes(self.data)

    def invoke(self, command="pdf-tables", *extra):
        return subprocess.run(
            [str(self.binary), command, str(self.path), "--page", "1", *extra],
            cwd=self.directory, env=self.env, capture_output=True, text=True, timeout=30,
        )

    def success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "", "successful main CLI output should not mix diagnostics into stderr")
        return result.stdout

    def failure(self, result, code, exit_code):
        self.assertEqual(result.returncode, exit_code, result.stderr)
        self.assertEqual(result.stdout, "", "rejected operations must not emit a partial result")
        value = json.loads(result.stderr)
        self.assertEqual(value["schema"], "omniattach-cli/error/0.1")
        self.assertEqual(value["error"]["code"], code)
        self.assertEqual(value["exit_code"], exit_code)
        return value

    def test_listing_provenance_and_grid(self):
        value = json.loads(self.success(self.invoke()))
        self.assertEqual(value["schema_version"], "omniattach/pdf-table-discovery/0.1")
        self.assertEqual(value["source"], str(self.path))
        self.assertEqual(value["source_sha256"], hashlib.sha256(self.data).hexdigest())
        self.assertEqual(value["page"], 1)
        self.assertEqual(value["candidates"], [{"candidate": 1, "page": 1, "x_boundaries": [50.0, 150.0, 250.0], "y_boundaries": [100.0, 130.0, 160.0]}])
        self.assertTrue(value["warnings"])

    def test_conversion_json_and_markdown_agree(self):
        raw = self.success(self.invoke("pdf-table", "--candidate", "1"))
        value = json.loads(raw)
        self.assertEqual(value["schema_version"], "omniattach/pdf-table/0.1")
        self.assertEqual(value["source_name"], str(self.path))
        self.assertEqual(value["source_sha256"], hashlib.sha256(self.data).hexdigest())
        self.assertEqual(value["grid"]["page"], 1)
        self.assertTrue(value["first_row_is_header"])
        self.assertEqual(len(value["source_sequences"]), 4)
        self.assertEqual(len(set(value["source_sequences"])), 4)
        self.assertTrue(all(n > 0 for n in value["source_sequences"]))
        self.assertTrue(value["warnings"])
        self.assertEqual(value["markdown"], EXPECTED)
        markdown = self.success(self.invoke("pdf-table", "--candidate", "1", "--output-format", "markdown"))
        self.assertEqual(markdown, value["markdown"])

    def test_pretty_changes_format_not_result(self):
        compact = self.success(self.invoke("pdf-table", "--candidate", "1"))
        pretty = self.success(self.invoke("pdf-table", "--candidate", "1", "--pretty"))
        self.assertEqual(json.loads(compact), json.loads(pretty))
        self.assertIn('\n  "', pretty)

    def test_output_caps_prevent_partial_results(self):
        for command, extra in [
            ("pdf-tables", []),
            ("pdf-table", ["--candidate", "1"]),
            ("pdf-table", ["--candidate", "1", "--output-format", "markdown"]),
        ]:
            with self.subTest(command=command, extra=extra):
                self.failure(self.invoke(command, *extra, "--max-output-bytes", "1"), "LIMIT_EXCEEDED", 6)

    def test_markdown_output_cap_exact_boundary(self):
        length = len(EXPECTED.encode("utf-8"))
        self.assertEqual(self.success(self.invoke("pdf-table", "--candidate", "1", "--output-format", "markdown", "--max-output-bytes", str(length))), EXPECTED)
        self.failure(self.invoke("pdf-table", "--candidate", "1", "--output-format", "markdown", "--max-output-bytes", str(length - 1)), "LIMIT_EXCEEDED", 6)

    def test_input_cap(self):
        self.failure(self.invoke("pdf-tables", "--max-input-bytes", "1"), "LIMIT_EXCEEDED", 6)

    def test_zero_and_missing_candidates(self):
        self.failure(self.invoke("pdf-table", "--candidate", "0"), "INVALID_ARGUMENT", 2)
        self.failure(self.invoke("pdf-table", "--candidate", "99"), "INVALID_INPUT", 3)
        self.failure(self.invoke("pdf-table"), "INVALID_ARGUMENT", 2)

    def test_empty_discovery_and_empty_grid_conversion(self):
        self.fixture(TEXT)
        value = json.loads(self.success(self.invoke()))
        self.assertEqual(value["candidates"], [])
        self.failure(self.invoke("pdf-table", "--candidate", "1"), "INVALID_INPUT", 3)
        self.fixture(GRID)
        self.failure(self.invoke("pdf-table", "--candidate", "1"), "INVALID_INPUT", 3)

    def test_crossing_text_and_clipping_are_structured_errors(self):
        self.fixture(GRID + TEXT.replace("60 110", "145 110"))
        self.failure(self.invoke("pdf-table", "--candidate", "1"), "INVALID_INPUT", 3)
        self.fixture("0 0 600 600 re W n\n" + GRID + TEXT)
        self.failure(self.invoke(), "INVALID_INPUT", 3)

    def test_transform_and_multiple_candidate_selection(self):
        self.fixture("q 2 0 0 2 5 7 cm\n" + GRID + TEXT + "Q\n")
        self.assertEqual(json.loads(self.success(self.invoke("pdf-table", "--candidate", "1")))["markdown"], EXPECTED)
        self.fixture(GRID + TEXT + "q 1 0 0 1 300 0 cm\n" + GRID + TEXT.replace("Alpha", "Beta") + "Q\n")
        self.assertEqual(len(json.loads(self.success(self.invoke()))["candidates"]), 2)
        value = json.loads(self.success(self.invoke("pdf-table", "--candidate", "2")))
        self.assertEqual(value["markdown"], EXPECTED.replace("Alpha", "Beta"))

    def test_no_export_or_model_cache_side_effects(self):
        before = sorted(str(p.relative_to(self.directory)) for p in self.directory.rglob("*"))
        self.success(self.invoke())
        self.success(self.invoke("pdf-table", "--candidate", "1"))
        after = sorted(str(p.relative_to(self.directory)) for p in self.directory.rglob("*"))
        self.assertEqual(before, after)
        self.assertEqual(self.path.read_bytes(), self.data)
        self.assertEqual(list(self.cache.iterdir()), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
