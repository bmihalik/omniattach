import copy
import json
from pathlib import Path
import tempfile
import unittest
from PIL import Image
import lfe_equation_label_audit as audit

class AuditTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup);self.root=Path(temp.name)
        self.source=self.root/'source';self.source.mkdir();self.kit=self.root/'kit';self.blind=self.kit/'blind';self.blind.mkdir(parents=True)
        self.labels_path=self.root/'labels.json';self.out=self.root/'out';self.proposals=[];self.public=[];self.mapping=[];self.labels=[]
        for i in range(3):
            path=self.source/f'{i}.png';image=Image.new('RGB',(20,15),'black' if i==1 else 'white');image.save(path)
            target=self.blind/f'r{i}.png';image.save(target)
            group=audit.sha(b'RGB:20:15:'+image.tobytes())
            self.proposals.append(dict(id=f'p{i}',image=str(path),image_sha256=audit.digest(path),page_id='negative' if i==1 else 'positive'))
            self.public.append(dict(review_id=f'r{i}',image=target.name,sha256=audit.digest(target)))
            self.mapping.append(dict(review_id=f'r{i}',proposal_id=f'p{i}',source_image_sha256=audit.digest(path),pixel_group=group,provisional_split='holdout' if int(group[:8],16)%5==0 else 'development'))
            self.labels.append(dict(review_id=f'r{i}',category='non_equation' if i==1 else 'equation',clockwise_degrees=None,notes='',basis='reviewer_agreement'))
        self.save()
    def save(self):
        def put(path,value):path.write_text(json.dumps(value))
        put(self.source/'proposals.json',dict(proposals=self.proposals))
        put(self.source/'report.json',dict(schema_version='omniattach/region-proposal-evaluation/0.1',status='completed_proposal_baseline',proposals=self.proposals,references=[dict(id='a')],matches=[dict(proposal_id='p0',reference_id='a')],pages=[dict(id='positive',negative=False),dict(id='negative',negative=True)]))
        put(self.blind/'pack.json',dict(schema_version='omniattach/blind-pack/0.1',items=self.public));pack_hash=audit.digest(self.blind/'pack.json')
        put(self.kit/'private-map.json',dict(schema_version='omniattach/review-map/0.1',pack_sha256=pack_hash,source_proposals_sha256=audit.digest(self.source/'proposals.json'),items=self.mapping))
        put(self.labels_path,dict(schema_version='omniattach/reviewed-labels/0.1',pack_sha256=pack_hash,items=self.labels))
    def execute(self,amendment=None):return audit.audit(self.source,self.kit,self.labels_path,self.out,amendment)
    def amendment(self):
        path=self.root/'amendment.json';path.write_text(json.dumps(dict(schema_version='omniattach/upright-amendment/0.1',producer_id='test-user',source_labels_sha256=audit.digest(self.labels_path),pack_sha256=audit.digest(self.blind/'pack.json'),changes=[dict(review_id=f'r{i}',old=None,new=0) for i in [0,2]])));return path
    def test_unknowns_remain_unknown_without_amendment(self):
        report=self.execute();self.assertEqual(report['status'],'needs_review');self.assertEqual(len(report['unknown_equation_orientation_ids']),2)
    def test_explicit_amendment_and_mapped_roles(self):
        original=self.labels_path.read_bytes();report=self.execute(self.amendment())
        self.assertEqual(report['status'],'checked');self.assertEqual(report['equation_orientations'],{'0':2});self.assertEqual(report['exact_pixel_groups'],2)
        self.assertEqual(report['unmatched_equation_candidates'][0]['proposal_id'],'p2')
        self.assertEqual(self.labels_path.read_bytes(),original);self.assertFalse(report['benchmark_validated'])
    def test_duplicate_label_conflict(self):
        self.labels[0]['clockwise_degrees']=0;self.labels[2]['clockwise_degrees']=90;self.save()
        report=self.execute();self.assertEqual(len(report['duplicate_label_conflicts']),1);self.assertEqual(report['status'],'needs_review')
    def test_wrong_amendment_binding_and_scope(self):
        path=self.amendment();original=json.loads(path.read_text())
        for mode in ['hash','id','new','duplicate']:
            value=copy.deepcopy(original)
            if mode=='hash':value['source_labels_sha256']='wrong'
            elif mode=='id':value['changes'][0]['review_id']='unknown'
            elif mode=='new':value['changes'][0]['new']=90
            else:value['changes'].append(value['changes'][0])
            path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):self.execute(path)
    def test_missing_mapping_and_split_mismatch(self):
        self.mapping[0]['provisional_split']='invalid';self.save()
        with self.assertRaises(ValueError):self.execute()
        self.mapping.pop();self.save()
        with self.assertRaises(ValueError):self.execute()
    def test_altered_pixels_even_with_updated_hashes(self):
        path=self.blind/'r0.png';Image.new('RGB',(20,15),'red').save(path)
        self.public[0]['sha256']=audit.digest(path);self.save()
        with self.assertRaisesRegex(ValueError,'pixels differ'):self.execute()
    def test_proxy_contradictions_are_flags_not_relabels(self):
        self.labels[0].update(category='non_equation',clockwise_degrees=None);self.labels[1].update(category='equation',clockwise_degrees=0);self.save()
        report=self.execute();self.assertEqual(set(report['proxy_label_conflict_ids']),{'r0','r1'});self.assertEqual(report['categories'],{'non_equation':1,'equation':2})
    def test_no_overwrite_or_output_inside_inputs(self):
        self.execute()
        with self.assertRaises(FileExistsError):self.execute()
        with self.assertRaises(ValueError):audit.audit(self.source,self.kit,self.labels_path,self.kit/'out')
    def test_source_crop_mutation(self):
        (self.source/'0.png').write_bytes(b'changed')
        with self.assertRaises(ValueError):self.execute()

if __name__=='__main__':unittest.main()
