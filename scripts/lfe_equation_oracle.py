#!/usr/bin/env python3
"""LFE-4D2 oracle crop control. Ground truth guides crops; no detector is evaluated."""
import argparse
from collections import Counter
from contextlib import closing
import hashlib
import importlib.metadata
import itertools
import json
import math
from pathlib import Path
import shutil
import sys
from PIL import ImageChops
from pypdf import PdfReader
import pypdfium2 as pdfium

TARGETS={'native_mathtext','vector_paths','full_page_raster'}
COORDINATES='unrotated_pdf_default_user_space'
MAX_PIXELS=20_000_000


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def local(root,name):
    path=(root/name).resolve(strict=True)
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError('unsafe corpus path')
    return path


def pixel_box(bounds,width,height,dpi):
    if not math.isfinite(dpi) or not 36<=dpi<=600:
        raise ValueError('DPI must be finite and between 36 and 600')
    if not all(math.isfinite(v) and v>0 for v in [width,height]):
        raise ValueError('invalid page dimensions')
    scale=dpi/72
    raster_width,raster_height=math.ceil(width*scale),math.ceil(height*scale)
    if raster_width*raster_height>MAX_PIXELS:
        raise ValueError('page exceeds raster pixel budget')
    if len(bounds)!=4 or not all(math.isfinite(v) for v in bounds):
        raise ValueError('invalid crop bounds')
    left,bottom,right,top=bounds
    if not (0<=left<right<=width and 0<=bottom<top<=height):
        raise ValueError('crop outside page or empty; no silent clamping')
    # PDFium maps the page to its ceil-rounded device rectangle.
    scale_x,scale_y=raster_width/width,raster_height/height
    return [math.floor(left*scale_x),math.floor((height-top)*scale_y),
            math.ceil(right*scale_x),math.ceil((height-bottom)*scale_y)]


def render_crop(path,occurrence,dpi,destination,expected_pages):
    with path.open('rb') as stream:
        reader=PdfReader(stream)
        count=len(reader.pages)
        if count!=expected_pages: raise ValueError('manifest/parser page-count disagreement')
        page=reader.pages[occurrence['page']-1]
        media=list(map(float,page.mediabox));crop=list(map(float,page.cropbox))
        if (int(page.get('/Rotate',0))%360 or float(page.get('/UserUnit',1))!=1
                or media[:2]!=[0,0] or crop!=media):
            raise ValueError('control requires zero-origin, unrotated, uncropped, UserUnit=1 pages')
        width,height=media[2:]
        scale=dpi/72
        expected_size=(math.ceil(width*scale),math.ceil(height*scale))
        box=pixel_box(occurrence['bbox_pdf'],width,height,dpi)
    with closing(pdfium.PdfDocument(str(path))) as document:
        if len(document)!=count:
            raise ValueError('renderer/parser page-count disagreement')
        with closing(document[occurrence['page']-1]) as page:
            if page.get_rotation()!=0 or any(abs(a-b)>.001 for a,b in zip(page.get_bbox(),media)):
                raise ValueError('renderer/parser geometry disagreement')
            with closing(page.render(scale=scale)) as bitmap:
                with bitmap.to_pil() as view, view.convert('RGB') as image:
                    if image.size!=expected_size:
                        raise ValueError(f'unexpected raster dimensions: actual={image.size}, expected={expected_size}, scale={scale!r}')
                    with image.crop(box) as crop:
                        mask=crop.point(lambda v:255 if v<245 else 0)
                        channels=mask.split()
                        ink=ImageChops.lighter(ImageChops.lighter(channels[0],channels[1]),channels[2])
                        histogram=ink.histogram();ink_bbox=ink.getbbox()
                        stats=dict(nonwhite_pixels=sum(histogram[1:]),threshold=245,
                            threshold_ink_bbox=ink_bbox,edge_ink=bool(ink_bbox and
                            (ink_bbox[0]==0 or ink_bbox[1]==0 or ink_bbox[2]==crop.width or ink_bbox[3]==crop.height)))
                        crop.save(destination)
                        return dict(pixel_box=box,page_size_pdf=[width,height],page_raster_size=list(image.size),image_size=list(crop.size),
                            pixel_sha256=hashlib.sha256(crop.tobytes()).hexdigest(),**stats)


def prepare(root,output,dpi):
    root=Path(root).resolve(strict=True);output=Path(output).absolute()
    if not math.isfinite(dpi) or not 36<=dpi<=600: raise ValueError('invalid DPI')
    manifest_path=root/'manifest.json';manifest_bytes=manifest_path.read_bytes();manifest=json.loads(manifest_bytes)
    if manifest['schema_version']!='omniattach/lfe-equation-fixtures/0.1': raise ValueError('unsupported manifest')
    documents={d['path']:d for d in manifest['documents']};occurrences=manifest['occurrences']
    if len(documents)!=len(manifest['documents']) or len({o['id'] for o in occurrences})!=len(occurrences):
        raise ValueError('duplicate document or occurrence IDs')
    for name,entry in documents.items():
        if digest(local(root,name))!=entry['sha256']: raise ValueError('source PDF hash mismatch')
    for occurrence in occurrences:
        entry=documents[occurrence['pdf']]
        if entry.get('negative') or not 1<=occurrence['page']<=entry['pages']: raise ValueError('invalid occurrence label')
    if not any(o['representation'] in TARGETS for o in occurrences): raise ValueError('no oracle-control targets')
    output.mkdir();(output/'crops').mkdir();rows=[]
    for index,occurrence in enumerate(occurrences):
        row=dict(occurrence,status='out_of_control_scope')
        if occurrence['representation'] in TARGETS:
            path=output/'crops'/f'{index:04d}.png'
            try:
                if occurrence['coordinate_space']!=COORDINATES or occurrence['bbox_kind']!='allocated_region_not_ink':
                    raise ValueError('unsupported ground-truth coordinate convention')
                stats=render_crop(local(root,occurrence['pdf']),occurrence,dpi,path,documents[occurrence['pdf']]['pages'])
                row.update(stats,image=str(path),image_sha256=digest(path),
                           status='ready' if stats['nonwhite_pixels'] else 'blank_crop')
            except Exception as error:
                row.update(status='crop_failed',error=f'{type(error).__name__}: {error}')
        rows.append(row)
    for name,entry in documents.items():
        if digest(local(root,name))!=entry['sha256']: raise ValueError('source PDF changed during rendering')
    return dict(schema_version='omniattach/formula-oracle-control/0.1',ground_truth_guided=True,
        detector_evaluated=False,scope='allocated manifest regions rendered from source PDF pages; not detector recall, localization accuracy, pixel fidelity to reference PNGs, or full extraction support',
        dpi=dpi,source_manifest_sha256=hashlib.sha256(manifest_bytes).hexdigest(),documents=manifest['documents'],occurrences=rows,
        versions={name:importlib.metadata.version(name) for name in ['pypdf','pypdfium2','Pillow']},
        coverage=dict(total_source_occurrences=len(rows),selected_occurrences=sum(o['representation'] in TARGETS for o in rows),
                      statuses=dict(Counter(o['status'] for o in rows))))


def recognize(report,config,repeats,timeout):
    from lfe_equation_eval import invoke,score
    from lfe_equation_metrics import POLICY
    if repeats<1 or not math.isfinite(timeout) or timeout<=0: raise ValueError('invalid repeats or timeout')
    backends=config['backends']
    if not backends or len({b['id'] for b in backends})!=len(backends): raise ValueError('nonempty unique backend IDs required')
    for backend in backends:
        if not backend['artifacts']: raise ValueError('declared model artifacts required')
        for artifact in backend['artifacts']:
            if digest(artifact['path'])!=artifact['sha256']: raise ValueError('model artifact hash mismatch')
    ready=[o for o in report['occurrences'] if o['status']=='ready'];by_id={o['id']:o for o in ready}
    cases=[dict(id=f"{o['id']}:{i}",formula_id=o['formula_id'],sha256=o['image_sha256'],
        latex=o['latex'],image=o['image'],occurrence_id=o['id']) for o in ready for i in range(repeats)]
    request=dict(schema_version='omniattach/formula-worker/0.1',images=[dict(id=c['id'],path=c['image']) for c in cases])
    results=[]
    for backend in backends:
        entry=dict(id=backend['id'],config=backend,status='not_run_no_ready_crops')
        if cases:
            try:
                for case in cases:
                    if digest(case['image'])!=case['sha256']: raise ValueError('crop hash mismatch before inference')
                executable=shutil.which(backend['argv'][0])
                entry['executable_artifact']=dict(path=str(Path(executable).resolve()),sha256=digest(executable)) if executable else None
                response,wall,diagnostics=invoke(backend['argv'],request,timeout)
                for case in cases:
                    if digest(case['image'])!=case['sha256']: raise ValueError('crop changed during inference')
                scored=score(response,cases)
                for row,case in zip(scored['rows'],cases): row['occurrence_id']=case['occurrence_id']
                strata={}
                for representation in sorted(TARGETS):
                    selected=[o for o in report['occurrences'] if o['representation']==representation]
                    evaluated=[r for r in scored['rows'] if by_id[r['occurrence_id']]['representation']==representation]
                    hits=sum(r['formatting_normalized_exact'] for r in evaluated)
                    strata[representation]=dict(selected_occurrences=len(selected),submitted_cases=len(evaluated),normalized_matches=hits,
                        selected_repeat_cases=len(selected)*repeats,oracle_selected_match_rate=hits/(len(selected)*repeats) if selected else None)
                consistency=[]
                for occurrence in ready:
                    repeated=[r for r in scored['rows'] if r['occurrence_id']==occurrence['id']]
                    consistency.append(dict(occurrence_id=occurrence['id'],cases=len(repeated),
                        raw_consistent=(all(r['usable'] for r in repeated) and len({r['prediction']['latex'] for r in repeated if r['usable']})==1) if repeats>1 else None))
                entry.update(status='completed',conditional_recognition=scored,by_representation=strata,per_occurrence_repeat_consistency=consistency,

                    normalized_matches=sum(r['formatting_normalized_exact'] for r in scored['rows']),
                    batch_wall_ms=wall,diagnostics=diagnostics,metadata=response.get('metadata',{}))
            except Exception as error: entry.update(status='failed',error=f'{type(error).__name__}: {error}')
        denominator=report['coverage']['selected_occurrences']*repeats
        entry.update(selected_repeat_cases=denominator,oracle_selected_match_rate=entry.get('normalized_matches',0)/denominator)
        results.append(entry)
    comparisons=[]
    for a,b in itertools.combinations(results,2):
        left={r['id']:r for r in a.get('conditional_recognition',{}).get('rows',[])}
        right={r['id']:r for r in b.get('conditional_recognition',{}).get('rows',[])}
        pairs=[(left[k],right[k]) for k in left.keys()&right.keys() if left[k]['usable'] and right[k]['usable']]
        comparisons.append(dict(backends=[a['id'],b['id']],comparable=len(pairs),exact_agreements=sum(u['prediction']['latex']==v['prediction']['latex'] for u,v in pairs)))
    report.update(scoring_policy=POLICY,repeats=repeats,requested_cases_per_backend=len(cases),backends=results,comparisons=comparisons)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('corpus',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--dpi',type=float,default=150);parser.add_argument('--config',type=Path)
    parser.add_argument('--repeats',type=int,default=3);parser.add_argument('--timeout',type=float,default=600)
    args=parser.parse_args()
    if args.repeats<1 or not math.isfinite(args.timeout) or args.timeout<=0: parser.error('invalid repeats or timeout')
    report=prepare(args.corpus,args.output,args.dpi)
    tools=[Path(__file__).resolve()]
    if args.config:
        report['config_sha256']=digest(args.config)
        recognize(report,json.loads(args.config.read_text()),args.repeats,args.timeout)
        tools.extend(Path(__file__).with_name(n) for n in ['lfe_equation_eval.py','lfe_equation_metrics.py'])
    report['tool_artifacts']=[dict(path=str(p),sha256=digest(p)) for p in tools]
    bad=any(o['status'] in {'crop_failed','blank_crop'} for o in report['occurrences'])
    bad=bad or any(b['status']!='completed' or b['conditional_recognition']['usable']!=b['conditional_recognition']['cases'] for b in report.get('backends',[]))
    report['status']='failed_control' if bad else ('completed_oracle_control' if args.config else 'prepared_oracle_control')
    with (args.output/'report.json').open('x') as stream:
        json.dump(report,stream,indent=2,allow_nan=False);stream.write('\n')
    print(json.dumps(dict(status=report['status'],coverage=report['coverage'],detector_evaluated=False)))
    return int(bad)


if __name__=='__main__': sys.exit(main())
