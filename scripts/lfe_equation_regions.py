#!/usr/bin/env python3
"""LFE-4D3: unclassified full-page ink-band proposals; fixture-reference proxies only."""
import argparse
from collections import Counter,defaultdict
from contextlib import closing
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import sys
from PIL import Image
import pypdfium2 as pdfium

BASELINE=dict(version='ink-bands/0.1',threshold=245,max_gap_px=12,padding_px=4,min_ink_pixels=4)
MAX_PIXELS=20_000_000


def digest(path):
    with Path(path).open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()


def local(root,name):
    path=(root/name).resolve(strict=True)
    if not path.is_relative_to(root) or not path.is_file(): raise ValueError('unsafe corpus path')
    return path


def ink(image):
    return image.convert('L').point(lambda value:255 if value<245 else 0)


def propose(image):
    """Input is pixels only: no paths, PDF identity, manifest, boxes or labels."""
    with ink(image) as mask:
        runs=[]
        for y,occupied in enumerate(mask.getprojection()[1]):
            if occupied:
                if runs and y-runs[-1][1]<=BASELINE['max_gap_px']: runs[-1][1]=y+1
                else: runs.append([y,y+1])
        regions=[];padding=BASELINE['padding_px']
        for top,bottom in runs:
            with mask.crop((0,top,image.width,bottom)) as band:
                if band.histogram()[255]<BASELINE['min_ink_pixels']: continue
                left,upper,right,lower=band.getbbox()
            regions.append([max(0,left-padding),max(0,top+upper-padding),
                            min(image.width,right+padding),min(image.height,top+lower+padding)])
        return regions


def iou(a,b):
    area=lambda box:(box[2]-box[0])*(box[3]-box[1])
    intersection=max(0,min(a[2],b[2])-max(a[0],b[0]))*max(0,min(a[3],b[3])-max(a[1],b[1]))
    return intersection/(area(a)+area(b)-intersection)


def match(proposals,references,threshold):
    """Deterministic maximum-cardinality matching, not greedy or max-total-IoU."""
    edges={}
    for index,reference in enumerate(references):
        candidates=[]
        if reference['status']=='ready':
            for j,proposal in enumerate(proposals):
                if proposal['page_id']==reference['page_id']:
                    overlap=iou(proposal['bbox_pixels'],reference['bbox_pixels'])
                    if overlap>=threshold: candidates.append((j,overlap))
        edges[index]=sorted(candidates,key=lambda pair:(-pair[1],pair[0]))
    owners={}
    def augment(index,seen):
        for j,overlap in edges[index]:
            if j in seen: continue
            seen.add(j)
            if j not in owners or augment(owners[j],seen): owners[j]=index;return True
        return False
    for index in range(len(references)): augment(index,set())
    return [dict(reference_id=references[index]['id'],proposal_id=proposals[j]['id'],
                 iou=iou(references[index]['bbox_pixels'],proposals[j]['bbox_pixels'])) for j,index in sorted(owners.items())]


def reference_box(occurrence,converter,image):
    if occurrence['coordinate_space']!='unrotated_pdf_default_user_space' or occurrence['bbox_kind']!='allocated_region_not_ink':
        raise ValueError('unsupported reference coordinates')
    bounds=occurrence['bbox_pdf']
    if len(bounds)!=4 or not all(math.isfinite(v) for v in bounds) or not (bounds[0]<bounds[2] and bounds[1]<bounds[3]):
        raise ValueError('invalid allocated reference region')
    left,bottom,right,top=bounds
    corners=[converter.to_bitmap(x,y) for x in [left,right] for y in [bottom,top]]
    xs,ys=zip(*corners);slot=[min(xs),min(ys),max(xs),max(ys)]
    if not (0<=slot[0]<slot[2]<=image.width and 0<=slot[1]<slot[3]<=image.height):
        raise ValueError('allocated reference region outside viewport; no clamping')
    with image.crop(slot) as crop,ink(crop) as mask:
        bounds=mask.getbbox()
        if bounds is None: raise ValueError('blank allocated reference region')
    return [slot[0]+bounds[0],slot[1]+bounds[1],slot[0]+bounds[2],slot[1]+bounds[3]],slot


def run(corpus,output,dpi=150,threshold=.5):
    if not math.isfinite(dpi) or not 36<=dpi<=600 or not math.isfinite(threshold) or not 0<threshold<=1:
        raise ValueError('invalid DPI or IoU threshold')
    root=Path(corpus).resolve(strict=True);output=Path(output).absolute()
    raw=(root/'manifest.json').read_bytes();manifest=json.loads(raw)
    if manifest['schema_version']!='omniattach/lfe-equation-fixtures/0.1': raise ValueError('unsupported manifest')
    docs=manifest['documents'];occurrences=manifest['occurrences'];by_doc={d['path']:d for d in docs}
    if len(by_doc)!=len(docs) or len({o['id'] for o in occurrences})!=len(occurrences): raise ValueError('duplicate IDs')
    groups=defaultdict(list)
    for occurrence in occurrences:
        doc=by_doc[occurrence['pdf']]
        if doc.get('negative') or not 1<=occurrence['page']<=doc['pages']: raise ValueError('invalid positive annotation')
        groups[occurrence['pdf'],occurrence['page']].append(occurrence)
    for doc in docs:
        path=local(root,doc['path'])
        if digest(path)!=doc['sha256']: raise ValueError('source PDF hash mismatch')
        with closing(pdfium.PdfDocument(str(path))) as document:
            if len(document)!=doc['pages']: raise ValueError('manifest/renderer page-count disagreement')
    output.mkdir();(output/'pages').mkdir();(output/'crops').mkdir()
    pages=[];proposals=[];references=[]
    for doc in docs:
        with closing(pdfium.PdfDocument(str(local(root,doc['path'])))) as document:
            for number in range(doc['pages']):
                ident=f'page-{len(pages):04d}'
                page_record=dict(id=ident,pdf=doc['path'],page=number+1,status='failed',negative=bool(doc.get('negative')))
                expected=[dict(o,page_id=ident,status='reference_failed',error='page not rendered') for o in groups[doc['path'],number+1]]
                try:
                    with closing(document[number]) as page:
                        width,height=math.ceil(page.get_width()*(dpi/72)),math.ceil(page.get_height()*(dpi/72))
                        if width<1 or height<1 or width*height>MAX_PIXELS: raise ValueError('invalid raster size or pixel budget exceeded')
                        with closing(page.render(scale=dpi/72,draw_annots=False,may_draw_forms=False)) as bitmap:
                            with bitmap.to_pil() as view,view.convert('RGB') as image:
                                if image.size!=(width,height): raise ValueError('unexpected raster dimensions')
                                image_path=output/'pages'/f'{ident}.png';image.save(image_path)
                                # Proposal generation finishes before inspecting this page's reference boxes.
                                regions=propose(image)
                                page_proposals=[]
                                for ordinal,box in enumerate(regions):
                                    candidate_id=f'{ident}-r{ordinal:03d}';path=output/'crops'/f'{candidate_id}.png'
                                    with image.crop(box) as crop: crop.save(path)
                                    page_proposals.append(dict(id=candidate_id,page_id=ident,bbox_pixels=box,image=str(path),image_sha256=digest(path)))
                                proposals.extend(page_proposals)
                                page_record.update(status='ready',image=str(image_path),image_sha256=digest(image_path),image_size=list(image.size))
                                converter=bitmap.get_posconv(page)
                                for reference in expected:
                                    try:
                                        box,slot=reference_box(reference,converter,image)
                                        reference.update(status='ready',bbox_pixels=box,allocated_slot_pixels=slot)
                                        reference.pop('error',None)
                                    except Exception as error: reference['error']=f'{type(error).__name__}: {error}'
                except Exception as error: page_record.update(status='failed',error=f'{type(error).__name__}: {error}')
                pages.append(page_record);references.extend(expected)
    for doc in docs:
        if digest(local(root,doc['path']))!=doc['sha256']: raise ValueError('source PDF changed during run')
    matches=match(proposals,references,threshold);matched={m['reference_id'] for m in matches}
    negative_ids={p['id'] for p in pages if p['negative']}
    negative_count=sum(p['page_id'] in negative_ids for p in proposals)
    strata={}
    for representation in sorted({r['representation'] for r in references}):
        group=[r for r in references if r['representation']==representation]
        strata[representation]=dict(positives=len(group),matched=sum(r['id'] in matched for r in group))
    metrics=dict(total_positive_occurrences=len(references),reference_failures=sum(r['status']!='ready' for r in references),
        proposed_regions=len(proposals),matched_occurrences=len(matches),missed_occurrences=len(references)-len(matches),
        unmatched_proposals=len(proposals)-len(matches),
        mean_matched_reference_iou=sum(m['iou'] for m in matches)/len(matches) if matches else None,
        all_positive_proxy_coverage=len(matches)/len(references) if references else None,
        reference_proxy_precision=len(matches)/len(proposals) if proposals else None,
        negative_pages=len(negative_ids),negative_pages_rendered=sum(p['negative'] and p['status']=='ready' for p in pages),
        negative_page_candidates=negative_count,by_representation=strata)
    failed=any(p['status']!='ready' for p in pages) or metrics['reference_failures']>0
    report=dict(schema_version='omniattach/region-proposal-evaluation/0.1',status='failed_proposal_control' if failed else 'completed_proposal_baseline',
        proposals_ground_truth_guided=False,equation_classifier=False,recognition_evaluated=False,
        scope='unclassified ink bands; fixture threshold-reference proxies, not semantic equation detection accuracy',
        reference_policy='grayscale <245 ink envelope within allocated manifest slot; not exact semantic ink annotation',
        bbox_convention='top-left pixel coordinates, half-open',iou_threshold=threshold,baseline=BASELINE,dpi=dpi,
        source_manifest_sha256=hashlib.sha256(raw).hexdigest(),documents=docs,pages=pages,proposals=proposals,references=references,
        matches=matches,metrics=metrics,script_sha256=digest(__file__),
        versions={name:importlib.metadata.version(name) for name in ['pypdfium2','Pillow']})
    for name,value in [('proposals.json',dict(baseline=BASELINE,dpi=dpi,proposals=proposals)),('report.json',report)]:
        with (output/name).open('x') as stream: json.dump(value,stream,indent=2,allow_nan=False);stream.write('\n')
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('corpus',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--dpi',type=float,default=150);parser.add_argument('--iou',type=float,default=.5)
    args=parser.parse_args();report=run(args.corpus,args.output,args.dpi,args.iou)
    print(json.dumps(dict(status=report['status'],metrics=report['metrics'],equation_classifier=False)))
    sys.exit(int(report['status']!='completed_proposal_baseline'))
