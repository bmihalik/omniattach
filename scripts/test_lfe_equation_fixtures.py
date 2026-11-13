#!/usr/bin/env python3
"""Run with pinned fixture requirements installed; no models or TeX needed."""
import shutil
import json
from pathlib import Path
import tempfile
import unittest

from lfe_equation_fixtures import generate
from validate_lfe_equation_fixtures import validate


class FixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.tmp.name)/"corpus"
        cls.manifest = generate(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_independent_validation(self):
        self.assertTrue(validate(self.root)["ok"])

    def test_never_overwrites(self):
        before = (self.root/"manifest.json").read_bytes()
        with self.assertRaises(FileExistsError):
            generate(self.root)
        self.assertEqual(before, (self.root/"manifest.json").read_bytes())

    def test_expected_coverage(self):
        self.assertEqual(len(self.manifest["source_formulas"]), 8)
        self.assertEqual(len(self.manifest["occurrences"]), 36)
        self.assertEqual(len(self.manifest["documents"]), 8)

    def test_same_environment_repeatability(self):
        other = Path(self.tmp.name)/"second"
        generate(other)
        self.assertEqual((other/"manifest.json").read_bytes(), (self.root/"manifest.json").read_bytes())
        for path in self.root.rglob("*"):
            if path.is_file():
                self.assertEqual(path.read_bytes(), (other/path.relative_to(self.root)).read_bytes(), str(path))

    def test_tamper_is_rejected(self):
        import shutil
        broken = Path(self.tmp.name)/"broken"
        shutil.copytree(self.root, broken)
        path = broken/"native.pdf"
        path.write_bytes(path.read_bytes()+b"tampered")
        with self.assertRaises(AssertionError):
            validate(broken)

    def test_wrong_region_is_rejected(self):
        broken = Path(self.tmp.name)/"wrong-region"
        shutil.copytree(self.root, broken)
        path = broken/"manifest.json"
        manifest = json.loads(path.read_text())
        manifest["occurrences"][0]["bbox_pdf"] = [0,0,20,20]
        path.write_text(json.dumps(manifest))
        with self.assertRaises(AssertionError):
            validate(broken)

    @unittest.skipUnless(shutil.which("pdflatex"), "pdflatex not provisioned")
    def test_optional_latex(self):
        root = Path(self.tmp.name)/"advanced"
        manifest = generate(root, latex=True)
        self.assertEqual(len(manifest["occurrences"]),40)
        self.assertTrue(validate(root)["ok"])


if __name__ == "__main__":
    unittest.main()
