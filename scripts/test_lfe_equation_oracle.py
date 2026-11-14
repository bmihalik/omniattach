"""Real PDF rendering/geometry tests; the dispatch test uses stub workers/scoring."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
from PIL import Image,ImageDraw
from pypdf import PdfReader,PdfWriter
from pypdf.generic import NameObject,NumberObject,RectangleObject
from reportlab.pdfgen.canvas import Canvas
from reportlab.lib.utils import ImageReader
import lfe_equation_oracle as oracle

class OracleTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.pdf=self.root/'control.pdf';self.out=self.root/'out'
        canvas=Canvas(str(self.pdf),pagesize=(100,80),invariant=1)
        for page in range(4):
            canvas.drawString(12,65,'outside control region')
            if page==1:
                canvas.setFillColorRGB(1,0,0);canvas.rect(12,12,10,10,stroke=0,fill=1)
            elif page==2:
                image=Image.new('RGB',(30,30),'white');ImageDraw.Draw(image).line((5,5,25,25),fill='black',width=3)
                canvas.drawImage(ImageReader(image),10,10,width=30,height=30)
            else: canvas.drawString(12,20,'x')
            canvas.showPage()
        canvas.save()
        representations=['native_mathtext','vector_paths','full_page_raster','image_xobject']
        self.manifest=dict(schema_version='omniattach/lfe-equation-fixtures/0.1',
            documents=[dict(path='control.pdf',pages=4,sha256=oracle.digest(self.pdf))],
            assets=[dict(path='missing-reference.png',sha256='0'*64)],
            occurrences=[dict(id=str(i),formula_id='x',pdf='control.pdf',page=i+1,latex='x',representation=r,
                bbox_pdf=[10,10,40,40],bbox_kind='allocated_region_not_ink',coordinate_space=oracle.COORDINATES,
                formula_image='missing-reference.png') for i,r in enumerate(representations)])
        self.save_manifest()

    def save_manifest(self): (self.root/'manifest.json').write_text(json.dumps(self.manifest))

    def test_real_rendering_and_coordinate_orientation(self):
        report=oracle.prepare(self.root,self.out,72)
        self.assertEqual(report['coverage']['statuses'],dict(ready=3,out_of_control_scope=1))
        self.assertTrue(report['ground_truth_guided']);self.assertFalse(report['detector_evaluated'])
        for row in report['occurrences'][:3]:
            self.assertEqual(row['pixel_box'],[10,40,40,70]);self.assertEqual(row['image_size'],[30,30])
            self.assertGreater(row['nonwhite_pixels'],0)
        with Image.open(report['occurrences'][1]['image']) as image:
            self.assertEqual(image.getpixel((7,23)),(255,0,0))
        with self.assertRaises(FileExistsError): oracle.prepare(self.root,self.out,72)

    def test_invalid_bounds_budget_and_fractional_rounding(self):
        self.assertEqual(oracle.pixel_box([.5,1.1,2.3,3.7],10,10,72),[0,6,3,9])
        for bounds,dpi in [([-1,0,10,10],72),([0,0,0,10],72),([0,0,10,10],float('nan'))]:
            with self.subTest(bounds=bounds,dpi=dpi),self.assertRaises(ValueError): oracle.pixel_box(bounds,100,80,dpi)
        with self.assertRaises(ValueError): oracle.pixel_box([0,0,20,20],10000,10000,72)

    def test_source_hash_checked_before_output(self):
        self.pdf.write_bytes(b'changed')
        with self.assertRaises(ValueError): oracle.prepare(self.root,self.out,72)
        self.assertFalse(self.out.exists())

    def test_transformed_pages_fail_closed(self):
        original=self.pdf.read_bytes()
        for mode in ['rotate','crop','origin','unit']:
            with self.subTest(mode=mode):
                writer=PdfWriter();writer.append(PdfReader(io.BytesIO(original)));page=writer.pages[0]
                if mode=='rotate': page.rotate(90)
                if mode=='crop': page.cropbox=RectangleObject([5,5,95,75])
                if mode=='origin': page.mediabox=RectangleObject([5,0,105,80])
                if mode=='unit': page[NameObject('/UserUnit')]=NumberObject(2)
                writer.write(self.pdf);self.manifest['documents'][0]['sha256']=oracle.digest(self.pdf);self.save_manifest()
                report=oracle.prepare(self.root,self.root/mode,72)
                self.assertEqual(report['occurrences'][0]['status'],'crop_failed')
                self.assertIn('requires zero-origin',report['occurrences'][0]['error'])
                self.assertEqual(len(report['occurrences']),4)

    def test_cli_model_free(self):
        result=subprocess.run([sys.executable,oracle.__file__,str(self.root),str(self.out),'--dpi','72'],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        report=json.loads((self.out/'report.json').read_text())
        self.assertEqual(report['status'],'prepared_oracle_control');self.assertNotIn('backends',report)

    def test_dispatch_and_blank_crop_denominator(self):
        for blank in [False,True]:
            if blank: self.manifest['occurrences'][0]['bbox_pdf']=[50,10,70,30];self.save_manifest()
            report=oracle.prepare(self.root,self.root/str(blank),72);calls=[]
            if blank: self.assertEqual(report['occurrences'][0]['status'],'blank_crop')
            def invoke(argv,request,timeout):
                calls.append(request)
                for item in request['images']:
                    self.assertEqual(set(item),{'id','path'});self.assertEqual(Path(item['path']).parent.name,'crops')
                return dict(results=[dict(id=i['id'],latex='x') for i in request['images']]),1,''
            def score(response,cases):
                return dict(cases=len(cases),usable=len(cases),rows=[dict(id=c['id'],usable=True,prediction=p,formatting_normalized_exact=True)
                    for c,p in zip(cases,response['results'])])
            evaluator=types.SimpleNamespace(invoke=invoke,score=score);metrics=types.SimpleNamespace(POLICY={})
            artifact=dict(path=str(self.pdf),sha256=oracle.digest(self.pdf))
            config=dict(backends=[dict(id=name,argv=[sys.executable],artifacts=[artifact]) for name in ['a','b']])
            with patch.dict(sys.modules,{'lfe_equation_eval':evaluator,'lfe_equation_metrics':metrics}): oracle.recognize(report,config,2,10)
            self.assertEqual(len(calls),2);expected=4 if blank else 6
            self.assertEqual(report['requested_cases_per_backend'],expected)
            self.assertEqual(report['backends'][0]['selected_repeat_cases'],6)
            self.assertEqual(report['backends'][0]['oracle_selected_match_rate'],expected/6)
            self.assertEqual(report['comparisons'][0]['exact_agreements'],expected)
            self.assertTrue(all(c['raw_consistent'] for c in report['backends'][0]['per_occurrence_repeat_consistency']))

    def test_letter_page_raster_rounding(self):
        writer=PdfWriter();writer.append(PdfReader(io.BytesIO(self.pdf.read_bytes())))
        for page in writer.pages:
            page.mediabox=RectangleObject([0,0,612,792]);page.cropbox=RectangleObject([0,0,612,792])
        writer.write(self.pdf);self.manifest['documents'][0]['sha256']=oracle.digest(self.pdf);self.save_manifest()
        for dpi,size in [(72,[612,792]),(150,[1275,1651]),(300,[2550,3301])]:
            with self.subTest(dpi=dpi):
                report=oracle.prepare(self.root,self.root/str(dpi),dpi)
                self.assertEqual(report['coverage']['statuses'],dict(ready=3,out_of_control_scope=1))
                for row in report['occurrences'][:3]: self.assertEqual(row['page_raster_size'],size)
        self.assertEqual(oracle.pixel_box([60,590,552,710],612,792,150),[125,170,1150,422])
        self.assertEqual(oracle.pixel_box([60,600,540,700],612,792,150),[125,191,1125,401])

    def test_unexpected_raster_size_still_rejected(self):
        wrong=types.SimpleNamespace(to_pil=lambda:Image.new('RGB',(1,1),'white'),close=lambda:None)
        with patch.object(oracle.pdfium.PdfPage,'render',return_value=wrong):
            report=oracle.prepare(self.root,self.out,150)
        self.assertEqual(report['coverage']['statuses'],dict(crop_failed=3,out_of_control_scope=1))
        for row in report['occurrences'][:3]:
            self.assertIn('actual=(1, 1)',row['error']);self.assertIn('expected=(209, 167)',row['error'])

if __name__=='__main__': unittest.main()
