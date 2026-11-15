import copy
import json
from pathlib import Path
import tempfile
import unittest
from PIL import Image,PngImagePlugin
import lfe_equation_review as review

class ReviewTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup);self.root=Path(temp.name)
        self.source=self.root/'source';self.source.mkdir();self.output=self.root/'kit';items=[]
        for i in range(3):
            image=Image.new('RGB',(20,15),'white' if i<2 else 'black');path=self.source/f'{i}.png'
            meta=PngImagePlugin.PngInfo();meta.add_text('expected_latex',f'secret-{i}');image.save(path,pnginfo=meta)
            items.append(dict(id=f'secret-proposal-{i}',image=str(path),image_sha256=review.digest(path),page_id='secret-page',bbox_pixels=[0,0,20,15]))
        (self.source/'proposals.json').write_text(json.dumps(dict(proposals=items)))
        review.prepare(self.source,self.output);self.blind=self.output/'blind'

    def filled(self,name,person):
        path=self.blind/name;doc=review.load(path);doc['reviewer_id']=person
        for row in doc['items']:row.update(category='equation',clockwise_degrees=0)
        target=self.root/name;target.write_text(json.dumps(doc));return target,doc

    def test_blind_pack_blank_metadata_free_and_grouped(self):
        manifest=review.load(self.blind/'pack.json');mapping=review.load(self.output/'private-map.json')
        self.assertEqual(set(manifest),{'schema_version','items'});self.assertNotIn('secret',json.dumps(manifest))
        self.assertEqual(len(manifest['items']),3);self.assertEqual(len({x['pixel_group'] for x in mapping['items']}),2)
        groups={}
        for item in mapping['items']:groups.setdefault(item['pixel_group'],set()).add(item['provisional_split'])
        self.assertTrue(all(len(values)==1 for values in groups.values()))
        for row in review.load(self.blind/'review-a.json')['items']:
            self.assertIsNone(row['category']);self.assertIsNone(row['clockwise_degrees'])
        for item in manifest['items']:
            with Image.open(self.blind/item['image']) as image:self.assertNotIn('expected_latex',image.info)
        self.assertTrue((self.blind/'index.html').exists())

    def test_consensus_preserves_full_denominator(self):
        a,_=self.filled('review-a.json','Alice');b,_=self.filled('review-b.json','Bob')
        report=review.compare(self.blind,a,b,self.root/'consensus')
        self.assertEqual(report['status'],'complete');self.assertEqual(report['joint_label_agreements'],3)
        self.assertEqual(len(review.load(self.root/'consensus/labels.json')['items']),3)

    def test_disagreement_requires_bound_adjudication(self):
        a,_=self.filled('review-a.json','Alice');b,doc=self.filled('review-b.json','Bob')
        doc['items'][0]['clockwise_degrees']=90;b.write_text(json.dumps(doc))
        pending=self.root/'pending';report=review.compare(self.blind,a,b,pending)
        self.assertEqual(report['status'],'pending_adjudication');self.assertFalse((pending/'labels.json').exists())
        template=review.load(pending/'adjudication-template.json');template['reviewer_id']='Carol'
        template['items'][0].update(category='ambiguous',clockwise_degrees=None)
        path=self.root/'adjudication.json';path.write_text(json.dumps(template))
        report=review.compare(self.blind,a,b,self.root/'resolved',path)
        self.assertEqual(report['status'],'complete');self.assertEqual(report['adjudicated'],1)
        template['review_pair_sha256']='wrong';path.write_text(json.dumps(template))
        with self.assertRaises(ValueError):review.compare(self.blind,a,b,self.root/'foreign',path)

    def test_unfinished_and_same_identity_rejected(self):
        with self.assertRaises(ValueError):review.compare(self.blind,self.blind/'review-a.json',self.blind/'review-b.json',self.root/'bad')
        a,_=self.filled('review-a.json','Alice');b,_=self.filled('review-b.json','alice')
        with self.assertRaises(ValueError):review.compare(self.blind,a,b,self.root/'same')

    def test_annotation_schema_rejects_loss_and_invalid_orientation(self):
        a,_=self.filled('review-a.json','Alice');b,original=self.filled('review-b.json','Bob')
        for mode in ['missing','duplicate','unknown','angle','category','nonmath_angle','pack']:
            doc=copy.deepcopy(original)
            if mode=='missing':doc['items'].pop()
            elif mode=='duplicate':doc['items'].append(doc['items'][0])
            elif mode=='unknown':doc['items'][0]['review_id']='unknown'
            elif mode=='angle':doc['items'][0]['clockwise_degrees']=False
            elif mode=='category':doc['items'][0]['category']='model_says_math'
            elif mode=='nonmath_angle':doc['items'][0]['category']='non_equation'
            else:doc['pack_sha256']='wrong'
            b.write_text(json.dumps(doc))
            with self.assertRaises(ValueError):review.compare(self.blind,a,b,self.root/mode)

    def test_tampered_image_rejected(self):
        item=review.load(self.blind/'pack.json')['items'][0];(self.blind/item['image']).write_bytes(b'changed')
        with self.assertRaises(ValueError):review.pack(self.blind)

    def test_source_hash_and_path_escape_rejected(self):
        source=review.load(self.source/'proposals.json');source['proposals'][0]['image_sha256']='wrong'
        (self.source/'proposals.json').write_text(json.dumps(source))
        with self.assertRaises(ValueError):review.prepare(self.source,self.root/'bad-source')
        external=self.root/'external.png';Image.new('RGB',(20,15)).save(external)
        source['proposals'][0].update(image=str(external),image_sha256=review.digest(external));(self.source/'proposals.json').write_text(json.dumps(source))
        with self.assertRaises(ValueError):review.prepare(self.source,self.root/'outside')

    def test_no_overwrite_or_input_output_overlap(self):
        with self.assertRaises(FileExistsError):review.prepare(self.source,self.output)
        with self.assertRaises(ValueError):review.prepare(self.source,self.source/'kit')
        a,_=self.filled('review-a.json','Alice');b,_=self.filled('review-b.json','Bob')
        with self.assertRaises(ValueError):review.compare(self.blind,a,b,self.blind/'report')

    def test_duplicate_json_keys_rejected(self):
        path=self.root/'duplicate.json';path.write_text('{"items": [], "items": []}')
        with self.assertRaises(ValueError):review.load(path)

if __name__=='__main__':unittest.main()
