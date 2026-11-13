#!/usr/bin/env python3
"""Synthetic triage tests. Run: python3 scripts/test_triage_pdf_image_export.py"""
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
import triage_pdf_image_export as triage


def document(texts):
    return {'source': {'sha256': 'a' * 64}, 'diagnostics': [], 'blocks':
            [{'kind': {'type': 'paragraph'}, 'text': text, 'source': {'page': page}}
             for page, text in texts]}


class TriageTests(unittest.TestCase):
    def test_equal(self):
        doc = document([(1, 'secret text')])
        report = triage.compare(doc, doc)
        self.assertEqual(report['differing_normalized_pages'], 0)
        self.assertNotIn('secret text', json.dumps(report))

    def test_localizes_page_and_original_block_index(self):
        a = document([(1, 'same'), (2, 'old')])
        b = document([(1, 'same'), (2, 'new')])
        a['blocks'].insert(1, {'kind': {'type': 'image'}, 'text': 'PRIVATE', 'source': {'page': 2}})
        report = triage.compare(a, b)
        self.assertEqual(report['differing_alphanumeric_pages'], 1)
        self.assertEqual(report['pages'][1]['regions'][0]['image_block_indices'], [2])
        self.assertNotIn('PRIVATE', json.dumps(report))

    def test_segmentation_is_not_character_loss(self):
        report = triage.compare(document([(1, 'one'), (1, 'two')]), document([(1, 'one two')]))
        self.assertTrue(report['pages'][0]['alphanumeric_sequence_equal'])
        self.assertTrue(report['pages'][0]['regions'])

    def test_whitespace_and_punctuation(self):
        a = document([(1, 'a  b!')])
        report = triage.compare(a, document([(1, 'a b!')]))
        self.assertTrue(report['pages'][0]['whitespace_normalized_equal'])
        report = triage.compare(a, document([(1, 'ab')]))
        self.assertTrue(report['pages'][0]['alphanumeric_sequence_equal'])
        self.assertFalse(report['pages'][0]['whitespace_normalized_equal'])

    def test_source_mismatch_and_missing_hash(self):
        a = document([])
        b = copy.deepcopy(a)
        b['source']['sha256'] = 'b' * 64
        self.assertEqual(triage.compare(a, b)['comparison_status'], 'refused_source_mismatch')
        b['source']['sha256'] = ''
        with self.assertRaises(ValueError):
            triage.compare(a, b)

    def test_skip_groups_do_not_expose_unknown_messages(self):
        a = document([])
        a['diagnostics'] = [{'code': 'PDF_IMAGE_SKIPPED', 'message': message, 'source': {'page': 3}}
                            for message in [next(iter(triage.KNOWN_REASONS)), 'PRIVATE MESSAGE']]
        report = triage.compare(a, document([]))
        self.assertEqual(sum(g['diagnostic_entries'] for g in report['skipped_image_groups']), 2)
        self.assertNotIn('PRIVATE MESSAGE', json.dumps(report))

    def test_missing_page_and_one_sided_page(self):
        a = document([(None, 'unknown'), (4, 'extra')])
        report = triage.compare(a, document([]))
        self.assertEqual([p['page'] for p in report['pages']], [None, 4])
        self.assertEqual(report['differing_alphanumeric_pages'], 2)

    def test_alignment_and_region_limits(self):
        a, b = document([(1, 'a'), (2, 'b')]), document([(1, 'x'), (2, 'y')])
        report = triage.compare(a, b, max_regions=1)
        self.assertEqual(report['pages'][1]['omitted_regions'], 1)
        a['blocks'].append(copy.deepcopy(a['blocks'][0]))
        self.assertEqual(triage.compare(a, b, max_paragraphs=1)['pages'][0]['alignment_status'], 'omitted_paragraph_limit')

    def test_cli_missing_input_and_report_exclusivity(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            args = [str(root), '--reference', str(root)]
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(triage.main(args), 1)
            self.assertEqual(json.loads(out.getvalue())['stage'], 'image_document')
            (root / 'document.json').write_text(json.dumps(document([(1, 'PRIVATE TEXT')])), encoding='utf-8')
            report = root / 'report.json'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(triage.main(args + ['--report', str(report)]), 0)
                prior = report.read_bytes()
                self.assertEqual(triage.main(args + ['--report', str(report)]), 1)
            self.assertEqual(report.read_bytes(), prior)
            self.assertNotIn('PRIVATE TEXT', prior.decode())
            with self.assertRaises(ValueError):
                triage.load_document(root, 1)


if __name__ == '__main__':
    unittest.main()
