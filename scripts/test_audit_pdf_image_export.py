#!/usr/bin/env python3
"""Synthetic export-audit regressions; no PDF, Rust toolchain or codec required."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import audit_pdf_image_export as audit


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'export'
        self.root.mkdir()
        (self.root / 'images').mkdir()
        self.image = b'structural fixture bytes, not a codec image'
        self.name = 'images/fixture.png'
        digest = hashlib.sha256(self.image).hexdigest()
        (self.root / self.name).write_bytes(self.image)
        self.doc = {
            'source': {'sha256': 'a' * 64},
            'blocks': [
                {'kind': {'type': 'paragraph'}, 'text': 'PRIVATE PROSE SENTINEL'},
                {'kind': {'type': 'image', 'asset_id': 'a1'}, 'text': 'Image 1'},
                {'kind': {'type': 'image', 'asset_id': 'a1'}, 'text': 'Image 2'},
            ],
            'assets': [{'id': 'a1', 'filename': self.name, 'media_type': 'image/png',
                        'sha256': digest, 'byte_length': len(self.image), 'bytes': list(self.image)}],
            'diagnostics': [{'code': 'LFE_IMAGE_EXPORT_EXPERIMENTAL'}],
        }
        self.markdown = '![one](images/fixture.png)\n![two](images/fixture.png)\n'
        self.manifest = {
            'source': {'sha256': 'a' * 64},
            'artifacts': [
                {'path': self.name, 'media_type': 'image/png', 'bytes': len(self.image),
                 'location': {'asset_id': 'a1', 'sha256': digest}},
                {'path': 'document.json', 'media_type': 'application/json', 'bytes': 0},
                {'path': 'document.md', 'media_type': 'text/markdown', 'bytes': 0},
            ],
        }
        self.save()

    def save(self):
        (self.root / 'document.json').write_text(json.dumps(self.doc), encoding='utf-8')
        (self.root / 'document.md').write_text(self.markdown, encoding='utf-8')
        for item in self.manifest['artifacts']:
            if item['path'] in ('document.json', 'document.md'):
                item['bytes'] = (self.root / item['path']).stat().st_size
        (self.root / 'manifest.json').write_text(json.dumps(self.manifest), encoding='utf-8')

    def test_repeated_asset_counts_and_no_prose_in_report(self):
        result = audit.audit(self.root)
        self.assertTrue(result['ok'], result)
        self.assertEqual((result['image_assets'], result['image_blocks'], result['markdown_image_links']), (1, 2, 2))
        self.assertNotIn('PRIVATE PROSE SENTINEL', json.dumps(result))

    def test_saved_byte_corruption_is_detected(self):
        (self.root / self.name).write_bytes(b'x' * len(self.image))
        result = audit.audit(self.root)
        self.assertFalse(result['ok'])
        self.assertIn('saved image length/hash disagrees with document', result['issues'])

    def test_missing_image_is_rejected(self):
        (self.root / self.name).unlink()
        with self.assertRaises((OSError, ValueError)):
            audit.audit(self.root)

    def test_missing_markdown_occurrence_is_detected(self):
        self.markdown = '![one](images/fixture.png)\n'
        self.save()
        self.assertIn('Markdown image occurrence counts disagree with typed blocks', audit.audit(self.root)['issues'])

    def test_escaped_alt_label_and_escaped_prose(self):
        self.markdown = r'![a \[label\]](images/fixture.png)' + '\n![two](images/fixture.png)\n' + r'\!\[not image\](missing.png)'
        self.save()
        self.assertTrue(audit.audit(self.root)['ok'])

    def test_wrong_image_block_reference_is_detected(self):
        self.doc['blocks'][1]['kind']['asset_id'] = 'missing'
        self.save()
        self.assertIn('image block references a missing asset', audit.audit(self.root)['issues'])

    def test_boolean_embedded_byte_is_rejected(self):
        self.doc['assets'][0]['bytes'][0] = True
        self.save()
        self.assertIn('invalid embedded image byte array', audit.audit(self.root)['issues'])

    def test_manifest_metadata_disagreement_is_detected(self):
        self.manifest['artifacts'][0]['location']['sha256'] = 'b' * 64
        self.save()
        self.assertIn('image artifact location hash/ID disagrees', audit.audit(self.root)['issues'])

    def test_read_budgets_are_enforced(self):
        with self.assertRaises(ValueError):
            audit.audit(self.root, json_limit=2)
        with self.assertRaises(ValueError):
            audit.audit(self.root, image_limit=len(self.image) - 1)

    def test_traversal_and_absolute_artifact_paths_are_rejected(self):
        for name in ('../outside', '/tmp/outside', 'images/../outside', 'images\\outside', 'C:/outside'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                audit.contained_file(self.root.resolve(), name)

    def test_symlink_image_is_rejected(self):
        original = self.root / self.name
        target = Path(self.temp.name) / 'outside.png'
        target.write_bytes(self.image)
        original.unlink()
        try:
            original.symlink_to(target)
        except (OSError, NotImplementedError):
            self.skipTest('symlinks unavailable')
        with self.assertRaises(ValueError):
            audit.audit(self.root)

    def test_reference_differences_do_not_claim_semantic_failure(self):
        reference = Path(self.temp.name) / 'reference'
        reference.mkdir()
        other = {'source': {'sha256': 'a' * 64}, 'blocks': [{'kind': {'type': 'paragraph'}, 'text': 'Different prose'}]}
        (reference / 'document.json').write_text(json.dumps(other), encoding='utf-8')
        result = audit.audit(self.root, reference)
        self.assertTrue(result['ok'])
        self.assertFalse(result['reference_comparison']['alphanumeric_sequence_equal'])
        other['source']['sha256'] = 'b' * 64
        (reference / 'document.json').write_text(json.dumps(other), encoding='utf-8')
        self.assertFalse(audit.audit(self.root, reference)['ok'])

    def test_zero_images_can_be_structurally_valid(self):
        self.doc['assets'] = []
        self.doc['blocks'] = self.doc['blocks'][:1]
        self.manifest['artifacts'] = self.manifest['artifacts'][1:]
        self.markdown = 'PRIVATE PROSE SENTINEL\n'
        self.save()
        result = audit.audit(self.root)
        self.assertTrue(result['ok'])
        self.assertEqual(result['image_assets'], 0)

    def test_cli_report_is_exclusive_and_failure_output_omits_prose(self):
        report = Path(self.temp.name) / 'report.json'
        argv = ['audit', str(self.root), '--report', str(report)]
        with patch.object(audit.sys, 'argv', argv), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(audit.main(), 0)
        previous = report.read_bytes()
        with patch.object(audit.sys, 'argv', argv), self.assertRaises(FileExistsError):
            audit.main()
        self.assertEqual(report.read_bytes(), previous)
        (self.root / self.name).unlink()
        output = io.StringIO()
        with patch.object(audit.sys, 'argv', ['audit', str(self.root)]), contextlib.redirect_stdout(output):
            self.assertEqual(audit.main(), 1)
        self.assertFalse(json.loads(output.getvalue())['ok'])
        self.assertNotIn('PRIVATE PROSE SENTINEL', output.getvalue())


if __name__ == '__main__':
    unittest.main()
