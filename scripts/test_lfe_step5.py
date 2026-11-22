#!/usr/bin/env python3
"""Model-free end-to-end tests for both LFE-STEP-5 examples (Python standard library)."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
GRID = "50 100 m 250 100 l 50 130 m 250 130 l 50 160 m 250 160 l 50 100 m 50 160 l 150 100 m 150 160 l 250 100 m 250 160 l S\n"
TEXT = "BT /F1 10 Tf 1 0 0 1 60 140 Tm (Name) Tj 1 0 0 1 160 140 Tm (Value) Tj 1 0 0 1 60 110 Tm (Alpha) Tj 1 0 0 1 160 110 Tm (42) Tj ET\n"
EXPECTED = "| Name | Value |\n| --- | --- |\n| Alpha | 42 |\n"


def pdf_bytes(content, page_extra=""):
    stream = content.encode("ascii")
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        ("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 600] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R " + page_extra + " >>").encode("ascii"),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"<< /Length " + str(len(stream)).encode("ascii") + b" >>\nstream\n" + stream + b"endstream",
    ]
    data = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for index, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f"{index} 0 obj\n".encode("ascii") + obj + b"\nendobj\n")
    xref = len(data)
    data.extend(f"xref\n0 {len(objects)+1}\n0000000000 65535 f \n".encode("ascii"))
    for offset in offsets[1:]:
        data.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
    data.extend(f"trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode("ascii"))
    return bytes(data)


class TableCLI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(["cargo", "build", "--locked", "-p", "omniattach-documents", "--features", "pdf", "--example", "pdf-table", "--example", "pdf-ruled-table"], cwd=ROOT, check=True, timeout=1800)
        raw = subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT, timeout=60)
        target = Path(json.loads(raw)["target_directory"])
        suffix = ".exe" if os.name == "nt" else ""
        cls.auto = target / "debug" / "examples" / ("pdf-ruled-table" + suffix)
        cls.explicit = target / "debug" / "examples" / ("pdf-table" + suffix)
        if not cls.auto.is_file() or not cls.explicit.is_file():
            raise RuntimeError("expected host debug binaries missing; unset cross-compilation target configuration")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="lfe-step5-")
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "fixture.pdf"

    def fixture(self, content=GRID + TEXT, page_extra=""):
        data = pdf_bytes(content, page_extra)
        self.path.write_bytes(data)
        return data

    def run_cli(self, command="list", page="1", binary=None, extra=()):
        return subprocess.run([str(binary or self.auto), str(self.path), page, command, *extra], cwd=ROOT, capture_output=True, text=True, timeout=30)

    def successful(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)

    def rejected(self, result, message):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(result.stdout, "", "failure must not emit partial Markdown")
        self.assertIn(message, result.stderr)

    def test_list_and_exact_markdown(self):
        data = self.fixture()
        listing = self.run_cli()
        self.successful(listing)
        parsed = json.loads(listing.stdout)
        self.assertEqual(parsed["page"], 1)
        self.assertEqual(parsed["source"], str(self.path))
        self.assertEqual(parsed["candidates"], [{"candidate": 1, "page": 1, "x_boundaries": [50.0, 150.0, 250.0], "y_boundaries": [100.0, 130.0, 160.0]}])
        result = self.run_cli("1")
        self.successful(result)
        self.assertEqual(result.stdout, EXPECTED)
        self.assertIn(hashlib.sha256(data).hexdigest(), result.stderr)
        self.assertEqual(self.path.read_bytes(), data)

    def test_manual_and_discovered_grids_agree(self):
        self.fixture()
        manual = self.run_cli("50,150,250", binary=self.explicit, extra=("100,130,160",))
        auto = self.run_cli("1")
        self.successful(manual)
        self.successful(auto)
        self.assertEqual(manual.stdout, EXPECTED)
        self.assertEqual(manual.stdout, auto.stdout)

    def test_transform_matches_text_and_grid(self):
        self.fixture("q 2 0 0 2 5 7 cm\n" + GRID + TEXT + "Q\n")
        listing = self.run_cli()
        self.successful(listing)
        grid = json.loads(listing.stdout)["candidates"][0]
        self.assertEqual(grid["x_boundaries"], [105.0, 305.0, 505.0])
        self.assertEqual(grid["y_boundaries"], [207.0, 267.0, 327.0])
        result = self.run_cli("1")
        self.successful(result)
        self.assertEqual(result.stdout, EXPECTED)

    def test_multiple_candidates_and_selection(self):
        self.fixture(GRID + TEXT + "q 1 0 0 1 300 0 cm\n" + GRID + TEXT.replace("Alpha", "Beta") + "Q\n")
        listing = self.run_cli()
        self.successful(listing)
        self.assertEqual(len(json.loads(listing.stdout)["candidates"]), 2)
        result = self.run_cli("2")
        self.successful(result)
        self.assertEqual(result.stdout, EXPECTED.replace("Alpha", "Beta"))

    def test_grid_without_text_is_not_a_successful_conversion(self):
        self.fixture(GRID)
        self.rejected(self.run_cli("1"), "no decoded text")

    def test_prose_only_has_no_candidates(self):
        self.fixture(TEXT)
        result = self.run_cli()
        self.successful(result)
        self.assertEqual(json.loads(result.stdout)["candidates"], [])
        self.rejected(self.run_cli("1"), "candidate not found")

    def test_crossing_text_is_rejected(self):
        self.fixture(GRID + TEXT.replace("60 110", "145 110"))
        self.rejected(self.run_cli("1"), "crosses grid boundary")

    def test_invalid_page_and_candidate(self):
        self.fixture()
        self.rejected(self.run_cli(page="0"), "page not found")
        self.rejected(self.run_cli("0"), "one-based")
        self.rejected(self.run_cli("99"), "candidate not found")

    def test_clipping_is_rejected(self):
        self.fixture("0 0 600 600 re W n\n" + GRID + TEXT)
        self.rejected(self.run_cli(), "clipped")

    def test_rotation_is_rejected(self):
        self.fixture(page_extra="/Rotate 90")
        self.rejected(self.run_cli(), "rotation")

    def test_userunit_is_rejected(self):
        self.fixture(page_extra="/UserUnit 2")
        self.rejected(self.run_cli(), "UserUnit")

    def test_q_restores_transform_between_candidates(self):
        self.fixture("q 1 0 0 1 300 0 cm\n" + GRID + "Q\n" + GRID + TEXT)
        result = self.run_cli("2")
        self.successful(result)
        self.assertEqual(result.stdout, EXPECTED)


if __name__ == "__main__":
    unittest.main(verbosity=2)
