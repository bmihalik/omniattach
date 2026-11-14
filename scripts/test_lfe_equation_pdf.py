"""Stub exports and dispatch only: no real PDF extraction or inference."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
from PIL import Image
from lfe_equation_pdf import digest, prepare, recognize

class PdfCropTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.out=self.root/'out'
        (self.root/'input.pdf').write_bytes(b'stub PDF')
        im=Image.new('RGB',(8,8),'white');im.putpixel((2,2),(0,0,0));im.save(self.root/'ref.png')
        self.manifest=dict(schema_version='omniattach/lfe-equation-fixtures/0.1',
            documents=[dict(path='input.pdf',pages=1,sha256=digest(self.root/'input.pdf'))],
            assets=[dict(path='ref.png',sha256=digest(self.root/'ref.png'))],
            occurrences=[dict(id=str(i),pdf='input.pdf',page=1,formula_id='x',latex='x',bbox_pdf=[0,0,8,8],
                representation='image_xobject' if i<2 else 'native_mathtext',formula_image='ref.png') for i in range(3)])
        (self.root/'manifest.json').write_text(json.dumps(self.manifest))
        self.mode='ok'

    def export(self,pdf,directory):
        directory.mkdir();shutil.copyfile(self.root/'ref.png',directory/'image.png')
        elements=[dict(kind='image',sequence=i,anchor=dict(page=1,operation_visit=i,form_path=[]),
                       bounds=dict(left=0,bottom=0,right=8,top=8,estimated=False)) for i in [1,2]]
        blocks=[dict(kind=dict(type='image',asset_id='a'),source=dict(page=1,path=f'content-operation {i} forms []')) for i in [1,2]]
        asset=dict(id='a',filename='image.png',sha256=digest(directory/'image.png'))
        if self.mode=='hash': asset['sha256']='0'*64
        if self.mode=='escape': asset['filename']='../escape.png'
        if self.mode=='missing': blocks.pop()
        if self.mode=='placement': elements[0]['bounds']['left']=1
        if self.mode=='pixels':
            Image.new('RGB',(8,8),'black').save(directory/'image.png');asset['sha256']=digest(directory/'image.png')
        identity=dict(sha256=digest(pdf))
        return (dict(schema_version='omniattach/pdf-inspection/0.1',source=identity,page_count=1,elements=elements),
                dict(schema_version='omniattach-documents/0.2',source=identity,assets=[asset],blocks=blocks))

    def run_prepare(self): return prepare(self.root,self.out,self.export)

    def test_coverage_reuse_and_exclusive_output(self):
        report=self.run_prepare();rows=report['occurrences']
        self.assertEqual(report['coverage']['statuses'],dict(ready=2,unsupported_representation=1))
        self.assertEqual(rows[0]['image'],rows[1]['image']);self.assertNotEqual(rows[0]['sequence'],rows[1]['sequence'])
        with self.assertRaises(FileExistsError): self.run_prepare()

    def test_changed_source_rejected_before_output(self):
        (self.root/'input.pdf').write_bytes(b'changed')
        with self.assertRaises(ValueError): self.run_prepare()
        self.assertFalse(self.out.exists())

    def test_unsafe_or_changed_export_rejected(self):
        for mode in ['hash','escape']:
            self.mode=mode
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temp:
                with self.assertRaises((ValueError,FileNotFoundError)):
                    prepare(self.root,Path(temp)/'new',self.export)

    def test_missing_images_not_dropped(self):
        self.mode='missing';report=self.run_prepare()
        self.assertEqual(len(report['occurrences']),3)
        self.assertEqual(report['coverage']['statuses']['missing_or_ambiguous_image_occurrence'],2)

    def test_fidelity_and_placement_independent(self):
        for mode in ['pixels','placement']:
            self.mode=mode
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temp:
                row=prepare(self.root,Path(temp)/'new',self.export)['occurrences'][0]
                self.assertEqual(row['status'],'ready')
                self.assertEqual(row['pixel_exact'],mode!='pixels');self.assertEqual(row['placement_ok'],mode!='placement')

    def test_dispatch_no_labels_and_full_denominator(self):
        report=self.run_prepare();calls=[]
        def invoke(argv,request,timeout):
            calls.append(request)
            for item in request['images']:
                self.assertEqual(set(item),{'id','path'})
                self.assertTrue(Path(item['path']).is_relative_to(self.out/'exports'))
            return dict(results=[dict(latex='x') for _ in request['images']]),1,''
        def score(response,cases):
            return dict(rows=[dict(id=c['id'],usable=True,prediction=p,formatting_normalized_exact=True) for c,p in zip(cases,response['results'])])
        evaluator=types.SimpleNamespace(digest=digest,invoke=invoke,score=score)
        metrics=types.SimpleNamespace(POLICY={},formatting_equal=lambda a,b:a==b)
        artifact=dict(path=str(self.root/'ref.png'),sha256=digest(self.root/'ref.png'))
        config=dict(backends=[dict(id=name,argv=['stub'],artifacts=[artifact]) for name in ['a','b']])
        with patch.dict(sys.modules,{'lfe_equation_eval':evaluator,'lfe_equation_metrics':metrics}): recognize(report,config,3,10)
        self.assertEqual(len(calls),2);self.assertEqual(report['requested_cases_per_backend'],6)
        self.assertEqual(report['backends'][0]['all_positive_repeat_cases'],9)
        self.assertEqual(report['backends'][0]['all_positive_normalized_rate'],2/3)
        self.assertEqual(report['comparisons'][0]['exact_agreements'],6)

if __name__=='__main__': unittest.main()
