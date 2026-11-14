"""Region proposals are image-only; reference matching is a separate fixture proxy."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from PIL import Image,ImageDraw
from pypdf import PdfReader,PdfWriter
from pypdf.generic import RectangleObject
from reportlab.pdfgen.canvas import Canvas
from reportlab.lib.utils import ImageReader
import lfe_equation_regions as regions

class RegionTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.out=self.root/'out';docs=[]
        canvas=Canvas(str(self.root/'positive.pdf'),pagesize=(612,792),invariant=1)
        for index in range(3):
            canvas.drawString(72,740,'prose before');canvas.drawString(72,550,'prose after')
            if index==0:
                canvas.setFont('Helvetica',22);canvas.drawString(72,650,'x+y=z');canvas.setFont('Helvetica',12)
            elif index==1:
                canvas.setLineWidth(2);canvas.line(72,640,100,668);canvas.line(72,668,100,640)
            else:
                image=Image.new('RGB',(100,60),'white');draw=ImageDraw.Draw(image)
                draw.line((10,10,90,50),fill='black',width=3);draw.line((10,50,90,10),fill='black',width=3)
                canvas.drawImage(ImageReader(image),72,620,width=200,height=80)
            canvas.showPage()
        canvas.save()
        canvas=Canvas(str(self.root/'negative.pdf'),pagesize=(612,792),invariant=1)
        for y in [740,650,550]: canvas.drawString(72,y,'ordinary prose only')
        canvas.save()
        docs=[dict(path=name,pages=pages,negative=negative,sha256=regions.digest(self.root/name))
              for name,pages,negative in [('positive.pdf',3,False),('negative.pdf',1,True)]]
        self.manifest=dict(schema_version='omniattach/lfe-equation-fixtures/0.1',documents=docs,occurrences=[
            dict(id=str(i),pdf='positive.pdf',page=i+1,formula_id='mark',latex='x',representation=representation,
                 bbox_pdf=[60,590,552,710],bbox_kind='allocated_region_not_ink',coordinate_space='unrotated_pdf_default_user_space')
            for i,representation in enumerate(['native_mathtext','vector_paths','full_page_raster'])])
        self.save()

    def save(self): (self.root/'manifest.json').write_text(json.dumps(self.manifest))

    def test_blank_and_band_grouping(self):
        image=Image.new('RGB',(100,100),'white');self.assertEqual(regions.propose(image),[])
        draw=ImageDraw.Draw(image);draw.rectangle([10,10,20,15],fill='black');draw.rectangle([15,20,30,25],fill='black')
        draw.rectangle([10,70,20,75],fill='black')
        self.assertEqual(regions.propose(image),[[6,6,35,30],[6,66,25,80]])

    def test_maximum_matching_not_greedy(self):
        refs=[dict(id='a',page_id='p',status='ready',bbox_pixels=[0,0,100,100]),dict(id='b',page_id='p',status='ready',bbox_pixels=[60,0,160,100])]
        proposals=[dict(id='x',page_id='p',bbox_pixels=[20,0,120,100]),dict(id='y',page_id='p',bbox_pixels=[0,0,40,100])]
        pairs=regions.match(proposals,refs,.4)
        self.assertEqual({(m['reference_id'],m['proposal_id']) for m in pairs},{('a','y'),('b','x')})
        self.assertEqual(len(regions.match(proposals,refs,.5)),1)
        self.assertEqual(len(regions.match(proposals*2,refs[:1],.4)),1)
        self.assertEqual(regions.match([dict(proposals[0],page_id='other')],refs,.4),[])

    def test_real_rendering_negative_candidates_and_denominators(self):
        report=regions.run(self.root,self.out)
        self.assertEqual(report['status'],'completed_proposal_baseline');metrics=report['metrics']
        self.assertEqual(metrics['total_positive_occurrences'],3);self.assertEqual(metrics['matched_occurrences'],3)
        self.assertEqual(metrics['negative_page_candidates'],3);self.assertEqual(metrics['negative_pages_rendered'],1)
        self.assertEqual(metrics['unmatched_proposals'],metrics['proposed_regions']-3)
        self.assertFalse(report['proposals_ground_truth_guided']);self.assertFalse(report['equation_classifier'])
        self.assertFalse(report['recognition_evaluated'])
        with self.assertRaises(FileExistsError): regions.run(self.root,self.out)

    def test_proposals_invariant_to_ground_truth_boxes_and_labels(self):
        first=regions.run(self.root,self.out)
        for occurrence in self.manifest['occurrences']:
            occurrence['bbox_pdf']=[400,100,450,150];occurrence['latex']='changed label'
        self.save();second=regions.run(self.root,self.root/'second')
        signature=lambda report:[(p['page_id'],p['bbox_pixels'],p['image_sha256']) for p in report['proposals']]
        self.assertEqual(signature(first),signature(second))
        self.assertEqual(second['metrics']['reference_failures'],3)
        self.assertEqual(second['metrics']['total_positive_occurrences'],3)
        self.assertEqual(second['metrics']['all_positive_proxy_coverage'],0)
        self.assertEqual(second['status'],'failed_proposal_control')
        self.manifest['occurrences']=[];self.save();third=regions.run(self.root,self.root/'third')
        self.assertEqual(signature(first),signature(third))
        self.assertIsNone(third['metrics']['all_positive_proxy_coverage'])

    def test_cropbox_and_rotation_reference_mapping(self):
        path=self.root/'positive.pdf';writer=PdfWriter();writer.append(PdfReader(io.BytesIO(path.read_bytes())))
        writer.pages[0].cropbox=RectangleObject([24,36,588,756]);writer.pages[0].rotate(90)
        writer.write(path);self.manifest['documents'][0]['sha256']=regions.digest(path);self.save()
        report=regions.run(self.root,self.out)
        self.assertEqual(report['metrics']['reference_failures'],0)
        reference=report['references'][0];width,height=report['pages'][0]['image_size']
        left,top,right,bottom=reference['bbox_pixels']
        self.assertTrue(0<=left<right<=width and 0<=top<bottom<=height)

    def test_hash_failure_before_output(self):
        (self.root/'positive.pdf').write_bytes(b'changed')
        with self.assertRaises(ValueError): regions.run(self.root,self.out)
        self.assertFalse(self.out.exists())

    def test_cli_and_invalid_settings(self):
        process=subprocess.run([sys.executable,regions.__file__,str(self.root),str(self.out)],capture_output=True,text=True)
        self.assertEqual(process.returncode,0,process.stderr)
        self.assertTrue((self.out/'proposals.json').exists())
        for dpi,threshold in [(0,.5),(150,float('nan')),(150,0)]:
            with self.assertRaises(ValueError): regions.run(self.root,self.root/'bad',dpi,threshold)

if __name__=='__main__': unittest.main()
