#!/usr/bin/env python3
"""Run from the repository root: python3 scripts/test_compare_pdf_export_text.py"""
import contextlib
import copy
import io
import json
from pathlib import Path
import random
import tempfile
import unittest
import compare_pdf_export_text as compare


def doc(*pairs):
    return {'source': {'sha256': 'a' * 64}, 'blocks': [
        {'kind': {'type': 'paragraph'}, 'text': text, 'source': {'page': page}}
        for page, text in pairs]}


class ComparisonTests(unittest.TestCase):
    def test_cross_page_segmentation_is_not_character_difference(self):
        result = compare.compare(doc((2, 'hello'), (3, 'world')), doc((2, 'hello\nworld')))
        self.assertTrue(result['alphanumeric_sequence_equal'])
        self.assertFalse(result['paragraph_count_equal'])
        self.assertEqual(result['regions'], [])

    def test_real_edit_maps_original_indices_not_exact_pages(self):
        a, b = doc((2, 'abcXdef')), doc((1, 'abcYdef'))
        a['blocks'].insert(0, {'kind': {'type': 'image'}, 'text': 'PRIVATE'})
        result = compare.compare(a, b)
        region = result['regions'][0]
        self.assertEqual(region['image']['alphanumeric_range'], [3, 4])
        self.assertEqual(region['image']['blocks'], [{'block_index': 1, 'paragraph_start_page': 2}])
        self.assertNotIn('PRIVATE', json.dumps(result))

    def test_whitespace_punctuation_unicode_and_case(self):
        result = compare.compare(doc((None, 'a  b!')), doc((4, 'ab')))
        self.assertTrue(result['alphanumeric_sequence_equal'])
        self.assertFalse(result['whitespace_normalized_equal'])
        self.assertFalse(compare.compare(doc((1, 'éA')), doc((1, 'éa')))['alphanumeric_sequence_equal'])

    def test_insert_delete_empty_and_boundary_context(self):
        for a, b in [('', ''), ('x', ''), ('', 'x'), ('abc', 'abXc')]:
            with self.subTest(a=a, b=b):
                result = compare.compare(doc((1, a)), doc((2, b)))
                self.assertEqual(result['alphanumeric_sequence_equal'], a == b)
                if a != b:
                    self.assertEqual(len(result['regions']), 1)
        region = compare.compare(doc((1, 'abc')), doc((2, 'abXc')))['regions'][0]
        self.assertEqual(region['image']['mapping_role'], 'insertion_context_only')

    def test_budget_limits_are_explicit(self):
        with self.assertRaises(ValueError):
            compare.compare(doc((1, 'long')), doc(), max_chars=2)
        result = compare.compare(doc((1, 'abc')), doc((1, 'xyz')), work_limit=1)
        self.assertEqual(result['regions'][0]['localization'], 'unresolved_work_limit')
        result = compare.compare(doc((1, 'abc')), doc((1, 'xyz')), max_regions=1)
        self.assertEqual(result['regions'][0]['localization'], 'unresolved_region_limit')

    def test_source_hash_validation_and_mismatch(self):
        a, b = doc(), doc()
        b['source']['sha256'] = 'b' * 64
        self.assertEqual(compare.compare(a, b)['comparison_status'], 'refused_source_mismatch')
        b['source']['sha256'] = ''
        with self.assertRaises(ValueError):
            compare.compare(a, b)

    def test_book_length_multiple_edits_and_segmentation(self):
        rng = random.Random(17)
        text = ''.join(rng.choices('abcdefghijklmnopqrstuvwxyz0123456789', k=110000))
        changed = text[:2000] + 'INSERTED' + text[2000:50000] + 'REPLACEMENT' + text[50003:]
        result = compare.compare(doc((1, changed[:70000]), (2, changed[70000:])), doc((1, text)))
        self.assertFalse(result['alphanumeric_sequence_equal'])
        self.assertEqual(result['unresolved_regions'], 0)
        self.assertEqual(len(result['regions']), 2)
        self.assertEqual(result['regions'][0]['image']['alphanumeric_range'], [2000, 2008])

    def test_repetitive_text_reports_coarse_region(self):
        result = compare.compare(doc((1, 'a' * 5000)), doc((1, 'b' * 5000)))
        self.assertEqual(result['regions'][0]['localization'], 'unresolved_no_unique_anchor')
        self.assertEqual(result['unresolved_regions'], 1)

    def test_cli_errors_privacy_and_exclusive_report(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            args = [str(root), '--reference', str(root)]
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main(args), 1)
            self.assertEqual(json.loads(out.getvalue())['stage'], 'image_document')
            (root / 'document.json').write_text(json.dumps(doc((1, 'PRIVATE TEXT'))), encoding='utf-8')
            report = root / 'report.json'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(compare.main(args + ['--report', str(report)]), 0)
                prior = report.read_bytes()
                self.assertEqual(compare.main(args + ['--report', str(report)]), 1)
                self.assertEqual(compare.main(args + ['--max-json-bytes', '1']), 1)
            self.assertEqual(prior, report.read_bytes())
            self.assertNotIn('PRIVATE TEXT', prior.decode())


if __name__ == '__main__':
    unittest.main()
