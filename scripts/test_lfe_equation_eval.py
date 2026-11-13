#!/usr/bin/env python3
"""Synthetic transport/scoring tests only: no recognition model is exercised."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from lfe_equation_eval import evaluate, invoke, samples, score


class EvalTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)
        # Deliberately not a real PNG: workers below are protocol stubs, not OCR.
        image = self.root/'image.png'
        image.write_bytes(b'protocol-test-image')
        self.hash = hashlib.sha256(image.read_bytes()).hexdigest()
        occurrence = dict(formula_id='x',latex='x',formula_image='image.png')
        manifest = dict(schema_version='omniattach/lfe-equation-fixtures/0.1',
            assets=[dict(path='image.png',sha256=self.hash)],occurrences=[occurrence,occurrence])
        (self.root/'manifest.json').write_text(json.dumps(manifest))
        self.case = dict(id='x:0',formula_id='x',sha256=self.hash,latex='x')

    def backend(self, text='x'):
        code = ("import json,sys; r=json.load(sys.stdin); "
                "assert all(set(i)=={'id','path'} for i in r['images']); "
                "print(json.dumps({'results':[dict(id=i['id'],status='ok',latex="+repr(text)+
                ",elapsed_ms=1.0) for i in r['images']]}))")
        return dict(id=text,argv=[sys.executable,'-c',code],artifacts=[dict(
            path=str(self.root/'image.png'),sha256=self.hash)])

    def test_deduplicates_occurrences(self):
        self.assertEqual(len(samples(self.root)),1)

    def test_success_repeats_and_agreement(self):
        report = evaluate(self.root,dict(backends=[self.backend(),self.backend('y')]),repeats=2)
        self.assertEqual(report['unique_formulas'],1)
        self.assertEqual(report['backends'][0]['exact'],2)
        self.assertEqual(report['backends'][1]['exact'],0)
        self.assertEqual(report['comparisons'][0]['comparable'],2)
        self.assertEqual(report['comparisons'][0]['exact_agreements'],0)

    def test_errors_and_empty_outputs_remain_in_denominator(self):
        for pred in [dict(id='x:0',status='error',error='test'),dict(id='x:0',status='ok',latex='')]:
            result=score(dict(results=[pred]),[self.case])
            self.assertEqual((result['cases'],result['usable'],result['exact_rate']),(1,0,0))

    def test_duplicate_and_missing_ids_rejected(self):
        row=dict(id='x:0',status='ok',latex='x')
        for rows in [[],[row,row],[dict(row,id='other')]]:
            with self.assertRaises(ValueError):
                score(dict(results=rows),[self.case])

    def test_invalid_latency_rejected(self):
        for duration in [-1,float('nan'),True]:
            with self.assertRaises(ValueError):
                score(dict(results=[dict(id='x:0',status='ok',latex='x',elapsed_ms=duration)]),[self.case])

    def test_model_hash_failure_is_not_an_accuracy_result(self):
        backend=self.backend()
        backend['artifacts'][0]['sha256']='0'*64
        with self.assertRaises(ValueError):
            evaluate(self.root,dict(backends=[backend]))

    def test_image_hash_failure(self):
        (self.root/'image.png').write_bytes(b'changed')
        with self.assertRaises(ValueError):
            samples(self.root)

    def test_process_failure_retained(self):
        backend=self.backend()
        backend['argv']=[sys.executable,'-c','raise SystemExit(2)']
        result=evaluate(self.root,dict(backends=[backend]))['backends'][0]
        self.assertEqual(result['status'],'failed')
        self.assertEqual(result['exact_rate'],0)

    def test_timeout(self):
        with self.assertRaises(TimeoutError):
            invoke([sys.executable,'-c','import time; time.sleep(2)'],{},.05)

    def test_trim_does_not_erase_internal_latex_whitespace(self):
        pred=dict(id='x:0',status='ok',latex=' x ')
        result=score(dict(results=[pred]),[self.case])
        self.assertFalse(result['rows'][0]['exact'])
        self.assertTrue(result['rows'][0]['trimmed_exact'])
        case=dict(self.case,latex=r'\text{a b}')
        pred['latex']=r'\text{ab}'
        self.assertFalse(score(dict(results=[pred]),[case])['rows'][0]['trimmed_exact'])


if __name__=='__main__':
    unittest.main()
