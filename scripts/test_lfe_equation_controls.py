"""Model-free tests; fake workers test protocol plumbing, not OCR accuracy."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from PIL import Image
import lfe_equation_controls as controls

WORKER=r"""import json,sys,time
request=json.load(sys.stdin);mode=sys.argv[1]
if mode=='timeout':time.sleep(2)
results=[dict(id=c['id'],status='ok',latex='x' if c['id'].startswith('p0:') else r'\text{prose}',elapsed_ms=1) for c in request['images']]
if mode=='duplicate':results.append(results[0])
if mode=='mutate':open(request['images'][0]['path'],'wb').write(b'changed')
print(json.dumps(dict(results=list(reversed(results)))))
"""

class ControlTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup);self.root=Path(temp.name)
        self.source=self.root/'source';self.source.mkdir();self.plan=self.root/'plan'
        boxes=[[10,10,30,25],[0,0,100,100],[40,40,60,55],[10,10,20,20]];proposals=[]
        for i,box in enumerate(boxes):
            path=self.source/f'{i}.png';image=Image.new('RGB',(box[2]-box[0],box[3]-box[1]),'white')
            if i in [0,2]:image.putpixel((0,0),(255,0,0))
            image.save(path)
            proposals.append(dict(id=f'p{i}',page_id='negative' if i==3 else 'positive',bbox_pixels=box,image=str(path),image_sha256=controls.digest(path)))
        self.data=dict(schema_version='omniattach/region-proposal-evaluation/0.1',status='completed_proposal_baseline',
            script_sha256=controls.digest(controls.d4.regions.__file__),baseline=dict(version='test'),dpi=150,iou_threshold=.5,
            source_manifest_sha256='0'*64,reference_policy='test envelopes',
            pages=[dict(id=name,image_size=[100,100],status='ready',negative=name=='negative') for name in ['positive','negative']],
            references=[dict(id='a',page_id='positive',status='ready',bbox_pixels=boxes[0],formula_id='a',latex='x',representation='test'),
                        dict(id='b',page_id='positive',status='ready',bbox_pixels=[70,70,90,90],formula_id='b',latex='y',representation='test')],
            proposals=proposals,metrics=dict(total_positive_occurrences=2,proposed_regions=4))
        self.save();self.worker=self.root/'worker.py';self.worker.write_text(WORKER);self.config=self.root/'config.json';self.configure()

    def save(self):
        self.data['matches']=controls.d4.regions.match(self.data['proposals'],self.data['references'],.5)
        (self.source/'report.json').write_text(json.dumps(self.data))
        (self.source/'proposals.json').write_text(json.dumps(dict(baseline=self.data['baseline'],dpi=150,proposals=self.data['proposals'])))

    def configure(self,mode='ok'):
        self.config.write_text(json.dumps(dict(backends=[dict(id='fake',argv=[sys.executable,str(self.worker),mode],artifacts=[dict(path=str(self.worker),sha256=controls.digest(self.worker))])])))

    def test_pixel_geometry_identity_and_label_free_requests(self):
        plan=controls.prepare(self.source,self.plan)
        self.assertEqual((plan['requests_per_policy'],plan['requests_per_backend']),(4,16))
        self.assertEqual(controls.digest(self.plan/'identity/0000.png'),self.data['proposals'][0]['image_sha256'])
        with Image.open(self.plan/'ccw90_pad16/0000.png') as image:
            self.assertEqual(image.size,(47,52));self.assertEqual(image.getpixel((16,35)),(255,0,0));self.assertEqual(image.getpixel((0,0)),(255,255,255))
        with Image.open(self.plan/'cw90_pad16/0000.png') as image:self.assertEqual(image.getpixel((30,16)),(255,0,0))
        for policy in controls.POLICIES:
            request=json.loads((self.plan/policy/'request.json').read_text())
            self.assertEqual(set(request),{'schema_version','images'});self.assertEqual(len(request['images']),4)
            self.assertTrue(all(set(row)=={'id','path'} for row in request['images']))
        controls.read_plan(self.plan)
        with self.assertRaises(FileExistsError):controls.prepare(self.source,self.plan)

    def test_fake_subprocesses_keep_every_policy_and_positive_denominator(self):
        controls.prepare(self.source,self.plan,repeats=2)
        result=controls.run(self.plan,self.config,self.root/'run')
        self.assertEqual(result['status'],'completed');self.assertEqual(result['requests_per_backend'],32)
        self.assertEqual(len(result['results']),4);self.assertFalse(result['ground_truth_policy_selection'])
        for entry in result['results']:
            self.assertEqual(entry['usable'],8)
            self.assertEqual(entry['conditional_localized_recognition']['cases'],2)
            self.assertEqual(entry['localized_recognition']['all_positive_repeat_cases'],4)
            self.assertEqual(entry['localized_recognition']['formatting_normalized_matches'],2)
            self.assertEqual(entry['unclassified_output_diagnostics']['negative_page']['requested_cases'],2)
        self.assertEqual(len({p['image_sha256'] for p in self.data['proposals']}),3)

    def test_labels_do_not_select_transforms_or_candidates(self):
        first=controls.prepare(self.source,self.plan)
        for reference in self.data['references']:reference['latex']='changed';reference['formula_id']='changed'
        self.save();second=controls.prepare(self.source,self.root/'other')
        for a,b in zip(first['policies'],second['policies']):
            self.assertEqual([(x['id'],x['image_sha256']) for x in a['candidates']],[(x['id'],x['image_sha256']) for x in b['candidates']])

    def test_pixel_substitution_rejected_even_with_updated_plan_hash(self):
        plan=controls.prepare(self.source,self.plan);record=plan['policies'][1]['candidates'][0];path=Path(record['image'])
        with Image.open(path) as image:image.putpixel((16,16),(0,0,0));image.save(path)
        record['image_sha256']=controls.digest(path);(self.plan/'plan.json').write_text(json.dumps(plan))
        with self.assertRaisesRegex(ValueError,'declared policy'):controls.read_plan(self.plan)

    def test_protocol_errors_and_timeouts_are_not_success(self):
        controls.prepare(self.source,self.plan)
        for mode in ['duplicate','timeout','mutate']:
            self.configure(mode);result=controls.run(self.plan,self.config,self.root/mode,timeout=.03 if mode=='timeout' else 5)
            self.assertEqual(result['status'],'failed');self.assertTrue(all(row['status']=='failed' for row in result['results']))
            self.assertTrue(all(row['localized_recognition']['formatting_normalized_matches']==0 for row in result['results']))

    def test_input_changes_and_unsafe_outputs_rejected(self):
        controls.prepare(self.source,self.plan)
        with self.assertRaises(ValueError):controls.run(self.plan,self.config,self.plan/'run')
        Path(self.data['proposals'][0]['image']).write_bytes(b'changed')
        with self.assertRaises(ValueError):controls.read_plan(self.plan)

    def test_selector_proxy_roles_abstention_and_unknowns(self):
        controls.prepare(self.source,self.plan);path=self.plan/'selector-template.json';data=json.loads(path.read_text())
        data['producer_id']='fake-test';decisions={'p0':'accept','p1':'reject','p2':'abstain','p3':'reject'}
        data['decisions']=[dict(id=k,decision=v) for k,v in decisions.items()];path.write_text(json.dumps(data))
        result=controls.select_audit(self.source,path,self.root/'selection.json')
        self.assertEqual(result['all_positive_proxy_retention'],.5);self.assertEqual(result['conditional_proxy_retention'],1)
        self.assertEqual(result['counts']['known_negative_page']['reject'],1)
        self.assertEqual(result['counts']['unlabeled'],dict(accept=0,reject=1,abstain=1));self.assertFalse(result['selector_executed'])
        self.assertNotIn('precision',result)

    def test_selector_rejects_incomplete_duplicate_and_foreign_decisions(self):
        controls.prepare(self.source,self.plan);path=self.plan/'selector-template.json';original=json.loads(path.read_text())
        with self.assertRaises(ValueError):controls.select_audit(self.source,path,self.root/'selection.json')
        for mode in ['missing','duplicate','foreign','invalid']:
            data=json.loads(json.dumps(original));data['producer_id']='fake-test'
            if mode=='missing':data['decisions'].pop()
            elif mode=='duplicate':data['decisions'].append(data['decisions'][0])
            elif mode=='foreign':data['source_proposals_sha256']='0'*64
            else:data['decisions'][0]['decision']='nonempty_latex'
            path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):controls.select_audit(self.source,path,self.root/f'{mode}.json')

    def test_zero_candidates_do_not_launch_models(self):
        self.data['proposals']=[];self.data['metrics']['proposed_regions']=0;self.save()
        controls.prepare(self.source,self.plan);self.configure('timeout')
        result=controls.run(self.plan,self.config,self.root/'empty',timeout=.03)
        self.assertEqual(result['status'],'completed');self.assertEqual(result['requests_per_backend'],0)
        self.assertTrue(all(row['status']=='not_run_no_candidates' for row in result['results']))

if __name__=='__main__':unittest.main()
