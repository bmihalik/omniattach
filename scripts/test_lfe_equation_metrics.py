"""Formatting policy regressions; no recognition models."""
import copy
import unittest
from lfe_equation_eval import score
from lfe_equation_metrics import formatting_equal, formatting_tokens, rescore_report, strip_outer_math, timing_summary
import test_lfe_equation_eval as fixtures

class MetricsTests(unittest.TestCase):
    def test_supported_formatting(self):
        for a,b in [('x','\\[x\\]'),('x^2','x^{2}'),('x_i','x_{i}'),
                    (r'\frac{a}{b}',r'\tfrac{a}{b}'),('(x)',r'\left( x\right)'),
                    (r'\lim_{x\to0}\frac{\sin x}{x}=1',r'\[\lim_{x\to 0}\!\!\tfrac{\sin x}{x}=1\]')]:
            with self.subTest(pair=(a,b)):
                self.assertTrue(formatting_equal(a,b))

    def test_wrong_math_and_opaque_text(self):
        for a,b in [('x+y','x-y'),('x^2','x^3'),('x_i','x_j'),
                    ('x^{12}','x^12'),(r'\frac{a}{b}',r'\frac{b}{a}'),
                    (r'\text{a b}',r'\text{ab}'),(r'\sin x',r'\sinx'),
                    (r'\unknown{x}','x'),('(x)','[x]')]:
            with self.subTest(pair=(a,b)):
                self.assertFalse(formatting_equal(a,b))
        for text in ['x^{2',r'\left(x',r'\left(x\right]',r'{\left(x}\right)',r'\text{a b}']:
            self.assertIsNone(formatting_tokens(text))
        for text in [r'\[x\)',r'\[x\] + \[y\]',r'\[\[x\]\]','$x$']:
            self.assertEqual(strip_outer_math(text),text)

    def test_denominators_consistency_and_offline_copy(self):
        cases=[dict(id=str(i),formula_id='x',sha256='test',latex='x') for i in range(3)]
        predictions=[dict(id='0',status='ok',latex='x',elapsed_ms=1),
                     dict(id='1',status='ok',latex=r'\[x\]',elapsed_ms=2),
                     dict(id='2',status='error',error='test')]
        backend=dict(score(dict(results=predictions),cases),status='completed')
        metrics=backend['formatting_metrics']
        self.assertEqual(metrics['repeated_cases']['counts']['formatting_normalized_exact'],2)
        self.assertEqual(metrics['repeated_cases']['cases'],3)
        self.assertEqual(metrics['unique_formulas']['all_repeats_match_counts']['formatting_normalized_exact'],0)
        self.assertEqual(metrics['repeat_consistency']['raw_consistent_formulas'],0)
        original=dict(schema_version='omniattach/formula-evaluation/0.1',backends=[backend])
        saved=copy.deepcopy(original)
        result=rescore_report(original)
        self.assertEqual(original,saved)
        self.assertFalse(result['rescoring']['new_inference'])
        self.assertEqual(result['backends'][0]['rows'][1]['prediction']['latex'],r'\[x\]')
        self.assertIsNone(timing_summary([])['median'])
        self.assertEqual(timing_summary([1,2,3])['p95'],3)

    def test_fresh_stub_integration(self):
        fixture=fixtures.EvalTests(); fixture.setUp()
        try:
            from lfe_equation_eval import evaluate
            report=evaluate(fixture.root,dict(backends=[fixture.backend('x'),fixture.backend(r'\[x\]')]),repeats=2)
            self.assertEqual(report['comparisons'][0]['exact_agreements'],0)
            self.assertEqual(report['comparisons'][0]['formatting_normalized_agreements'],2)
            self.assertIn('sha256',report['backends'][0]['executable_artifact'])
        finally:
            fixture.doCleanups()

if __name__ == '__main__':
    unittest.main()
