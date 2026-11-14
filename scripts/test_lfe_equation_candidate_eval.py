"""Fake subprocess workers exercise the shared protocol/scorer, not model accuracy."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from PIL import Image
import lfe_equation_candidate_eval as candidate

WORKER=r"""import json,sys,time
request=json.load(sys.stdin);mode=sys.argv[1]
open(sys.argv[2],'w').write(json.dumps(request))
if mode=='timeout':time.sleep(2)
results=[]
for item in request['images']:
    proposal=item['id'].rsplit(':',1)[0]
    text={'p0':'x','p1':'y'}.get(proposal,r'\text{prose}')
    if mode=='spacing' and proposal in ['p0','p1']:text=r'\['+text+r'\!\]'
    row=dict(id=item['id'],status='ok',latex=text,elapsed_ms=1.0)
    if mode=='error_empty':
        if proposal=='p0':row=dict(id=item['id'],status='error',error='sample error')
        elif proposal in ['p3','p4']:row['latex']=''
    results.append(row)
if mode=='missing':results=results[:-1]
if mode=='duplicate':results.append(results[0])
if mode=='unknown':results[0]['id']='unknown'
if mode=='bad_status':results[0]['status']='unexpected'
if mode=='bad_elapsed':results[0]['elapsed_ms']=True
if mode=='mutate':open(request['images'][0]['path'],'wb').write(b'changed')
print(json.dumps([] if mode=='not_object' else dict(results=list(reversed(results)),metadata=dict(fake=True))))
"""

class CandidateTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.input=self.root/'regions';self.input.mkdir()
        (self.input/'crops').mkdir();self.output=self.root/'out';self.config=self.root/'config.json'
        self.worker=self.root/'worker.py';self.worker.write_text(WORKER)
        boxes=[[10,10,30,30],[50,50,70,70],[0,0,100,100],[10,10,30,30],[40,40,60,60]]
        proposals=[]
        for index,box in enumerate(boxes):
            path=self.input/'crops'/f'{index}.png'
            Image.new('RGB',(box[2]-box[0],box[3]-box[1]),'white').save(path)
            proposals.append(dict(id=f'p{index}',page_id='positive' if index<3 else 'negative',bbox_pixels=box,image=str(path),image_sha256=candidate.digest(path)))
        self.source=dict(schema_version='omniattach/region-proposal-evaluation/0.1',status='completed_proposal_baseline',
            script_sha256=candidate.digest(candidate.regions.__file__),baseline=dict(version='test'),dpi=150,iou_threshold=.5,
            source_manifest_sha256='0'*64,reference_policy='test fixture envelopes',
            pages=[dict(id=name,image_size=[100,100],status='ready',negative=name=='negative') for name in ['positive','negative']],
            references=[dict(id=name,page_id='positive',status='ready',bbox_pixels=box,formula_id=name,latex=label,representation='test')
                for name,box,label in [('a',boxes[0],'x'),('b',boxes[1],'y'),('c',[80,80,90,90],'z')]],proposals=proposals,
            metrics=dict(total_positive_occurrences=3,proposed_regions=5))
        self.save();self.configure()

    def save(self):
        self.source['matches']=candidate.regions.match(self.source['proposals'],self.source['references'],.5)
        (self.input/'report.json').write_text(json.dumps(self.source))
        (self.input/'proposals.json').write_text(json.dumps(dict(baseline=self.source['baseline'],dpi=150,proposals=self.source['proposals'])))

    def configure(self,modes=('ok','spacing')):
        backends=[dict(id=str(i),argv=[sys.executable,str(self.worker),mode,str(self.root/f'log-{i}.json')],
            artifacts=[dict(path=str(self.worker),sha256=candidate.digest(self.worker))]) for i,mode in enumerate(modes)]
        self.config.write_text(json.dumps(dict(backends=backends)))

    def execute(self,timeout=5): return candidate.evaluate(self.input,self.config,self.output,2,timeout)

    def test_all_candidates_duplicates_negatives_and_denominators(self):
        report=self.execute();self.assertEqual(report['requested_cases_per_backend'],10)
        self.assertEqual(len({p['image_sha256'] for p in self.source['proposals']}),2)
        for backend in report['backends']:
            self.assertEqual(backend['usable'],10)
            self.assertEqual(backend['conditional_localized_recognition']['cases'],4)
            self.assertEqual(backend['localized_recognition']['formatting_normalized_matches'],4)
            self.assertEqual(backend['localized_recognition']['all_positive_repeat_cases'],6)
            self.assertEqual(backend['localized_recognition']['all_positive_normalized_rate'],4/6)
            self.assertEqual(backend['unclassified_output_diagnostics']['unmatched']['requested_cases'],6)
            self.assertEqual(backend['unclassified_output_diagnostics']['negative_page']['usable_nonempty'],4)
        request=json.loads((self.root/'log-0.json').read_text())
        self.assertEqual(set(request),{'schema_version','images'});self.assertEqual(len(request['images']),10)
        self.assertTrue(all(set(item)=={'id','path'} for item in request['images']))
        self.assertEqual(report['comparisons'][0]['comparable'],10)
        self.assertEqual(report['comparisons'][0]['formatting_policy_agreements'],10)
        with self.assertRaises(FileExistsError): self.execute()

    def test_labels_do_not_change_worker_request(self):
        self.execute();first=json.loads((self.root/'log-0.json').read_text())
        for reference in self.source['references']:reference['latex']='changed';reference['formula_id']='changed'
        self.save();self.output=self.root/'other';self.execute()
        self.assertEqual(first,json.loads((self.root/'log-0.json').read_text()))

    def test_sample_errors_and_empty_outputs_are_not_classification(self):
        self.configure(('error_empty',));report=self.execute();backend=report['backends'][0]
        self.assertEqual(report['status'],'completed_candidate_control')
        self.assertEqual(backend['localized_recognition']['formatting_normalized_matches'],2)
        self.assertEqual(backend['unclassified_output_diagnostics']['negative_page']['empty_ok_results'],4)
        self.assertFalse(report['equation_classifier'])

    def test_invalid_worker_protocol_has_zero_credit(self):
        for mode in ['missing','duplicate','unknown','bad_status','bad_elapsed','not_object']:
            with self.subTest(mode=mode):
                self.configure((mode,));self.output=self.root/mode;report=self.execute()
                self.assertEqual(report['status'],'failed_candidate_control')
                self.assertEqual(report['backends'][0]['localized_recognition']['formatting_normalized_matches'],0)

    def test_changed_crop_preflight_and_output_inside_input(self):
        path=Path(self.source['proposals'][0]['image']);path.write_bytes(b'changed')
        with self.assertRaises(ValueError): self.execute()
        self.assertFalse(self.output.exists())
        with self.assertRaises(ValueError): candidate.evaluate(self.input,self.config,self.input/'out')

    def test_worker_mutation_and_timeout_fail_closed(self):
        self.configure(('mutate','ok'));report=self.execute()
        self.assertTrue(all(b['status']=='failed' for b in report['backends']))
        self.assertFalse((self.root/'log-1.json').exists())
        # Restore the crop so the timeout check reaches the actual subprocess.
        Image.new('RGB',(20,20),'white').save(self.source['proposals'][0]['image'])
        self.configure(('timeout',));self.output=self.root/'timeout';report=self.execute(.03)
        self.assertEqual(report['status'],'failed_candidate_control')
        self.assertIn('TimeoutError',report['backends'][0]['error'])

    def test_zero_candidates_and_no_matches(self):
        original=copy.deepcopy(self.source['proposals'])
        self.configure(('ok',));self.source['proposals']=[];self.source['metrics']['proposed_regions']=0;self.save()
        report=self.execute();self.assertEqual(report['status'],'completed_candidate_control')
        self.assertEqual(report['backends'][0]['status'],'not_run_no_candidates')
        self.assertEqual(report['backends'][0]['localized_recognition']['all_positive_normalized_rate'],0)
        self.assertFalse((self.root/'log-0.json').exists())
        self.source['proposals']=original;self.source['metrics']['proposed_regions']=5
        for reference in self.source['references']:reference['bbox_pixels']=[1,1,2,2]
        self.save();self.output=self.root/'no-matches';report=self.execute()
        self.assertEqual(report['requested_cases_per_backend'],10)
        self.assertIsNone(report['backends'][0]['conditional_localized_recognition'])
        self.assertEqual(report['backends'][0]['localized_recognition']['all_positive_normalized_rate'],0)

if __name__=='__main__':unittest.main()
