#!/usr/bin/env python3
"""LFE-4D4: recognize every frozen D3 candidate; associate references after inference."""
import argparse
import copy
import itertools
import json
import math
from pathlib import Path
import shutil
import sys
from PIL import Image
import lfe_equation_regions as regions
from lfe_equation_eval import digest,invoke,score
from lfe_equation_metrics import POLICY,formatting_equal


def load_inputs(root):
    root=Path(root).resolve(strict=True)
    paths=[root/'report.json',root/'proposals.json']
    report=json.loads(paths[0].read_text());proposals=json.loads(paths[1].read_text())
    if report['schema_version']!='omniattach/region-proposal-evaluation/0.1' or report['status']!='completed_proposal_baseline':
        raise ValueError('a completed D3 run is required')
    if report['script_sha256']!=digest(regions.__file__): raise ValueError('D3 evaluator fingerprint changed')
    if proposals!=dict(baseline=report['baseline'],dpi=report['dpi'],proposals=report['proposals']):
        raise ValueError('proposal artifacts disagree')
    pages={p['id']:p for p in report['pages']};references=report['references'];candidates=report['proposals']
    if len(pages)!=len(report['pages']) or len({p['id'] for p in candidates})!=len(candidates) or len({r['id'] for r in references})!=len(references):
        raise ValueError('duplicate source IDs')
    if any(p['status']!='ready' for p in pages.values()) or any(r['status']!='ready' for r in references):
        raise ValueError('incomplete page/reference ledger')
    if report['metrics']['total_positive_occurrences']!=len(references) or report['metrics']['proposed_regions']!=len(candidates):
        raise ValueError('inconsistent source denominators')
    for item in candidates+references:
        page=pages[item['page_id']];w,h=page['image_size'];box=item['bbox_pixels']
        if len(box)!=4 or any(type(v) is not int for v in box) or not (0<=box[0]<box[2]<=w and 0<=box[1]<box[3]<=h):
            raise ValueError('invalid source pixel box')
    snapshots={str(p):digest(p) for p in paths}
    candidates=copy.deepcopy(candidates)
    for item in candidates:
        path=Path(item['image']).resolve(strict=True)
        if not path.is_relative_to(root) or not path.is_file(): raise ValueError('crop outside input run')
        if digest(path)!=item['image_sha256']: raise ValueError('crop hash mismatch')
        box=item['bbox_pixels']
        with Image.open(path) as image:
            if image.format!='PNG' or image.size!=(box[2]-box[0],box[3]-box[1]): raise ValueError('unexpected crop format/dimensions')
            image.verify()
        item['image']=str(path);snapshots[str(path)]=item['image_sha256']
    return report,candidates,snapshots


def verify(snapshots):
    for path,expected in snapshots.items():
        if digest(path)!=expected: raise ValueError('input artifact changed: '+path)


def build_cases(candidates,repeats):
    # Empty labels are used only by the shared response validator, never as truth scores.
    return [dict(id=f"{p['id']}:{i}",proposal_id=p['id'],formula_id=p['id'],latex='',
                 image=p['image'],sha256=p['image_sha256']) for p in candidates for i in range(repeats)]


def associate(source):
    matches=regions.match(source['proposals'],source['references'],source['iou_threshold'])
    if matches!=source['matches']: raise ValueError('frozen D3 matching differs on replay')
    return {m['proposal_id']:m['reference_id'] for m in matches}


def summarize(entry,source,cases,association,repeats):
    references={r['id']:r for r in source['references']};props={p['id']:p for p in source['proposals']}
    negative_pages={p['id'] for p in source['pages'] if p['negative']}
    negative_ids={p['id'] for p in props.values() if p['page_id'] in negative_pages}
    rows=entry.get('rows',[]);predictions={r['id']:r['prediction'] for r in rows}
    matched_cases=[]
    for case in cases:
        if case['proposal_id'] in association:
            reference=references[association[case['proposal_id']]]
            matched_cases.append(dict(case,formula_id=reference['formula_id'],latex=reference['latex']))
    conditional=None
    if entry['status']=='completed' and matched_cases:
        conditional=score(dict(results=[predictions[c['id']] for c in matched_cases]),matched_cases)
        for row,case in zip(conditional['rows'],matched_cases):
            row.update(proposal_id=case['proposal_id'],reference_id=association[case['proposal_id']])
    hits=sum(r['formatting_normalized_exact'] for r in conditional['rows']) if conditional else 0
    exact=sum(r['exact'] for r in conditional['rows']) if conditional else 0
    denominator=len(references)*repeats
    diagnostics={}
    groups=dict(unmatched={p for p in props if p not in association},negative_page=negative_ids)
    for name,ids in groups.items():
        group=[r for r in rows if r['proposal_id'] in ids]
        diagnostics[name]=dict(proposal_occurrences=len(ids),requested_cases=len(ids)*repeats,
            returned_cases=len(group),usable_nonempty=sum(r['usable'] for r in group),
            error_results=sum(r['prediction']['status']=='error' for r in group),
            empty_ok_results=sum(r['prediction']['status']=='ok' and not r['usable'] for r in group))
    strata={}
    for representation in sorted({r['representation'] for r in references.values()}):
        ids={r['id'] for r in references.values() if r['representation']==representation}
        group=[r for r in conditional['rows'] if r['reference_id'] in ids] if conditional else []
        strata[representation]=dict(all_positive_repeat_cases=len(ids)*repeats,
            localized_repeat_cases=sum(v in ids for v in association.values())*repeats,
            normalized_matches=sum(r['formatting_normalized_exact'] for r in group))
    consistency=[]
    for proposal in props:
        group=[r for r in rows if r['proposal_id']==proposal]
        consistency.append(dict(proposal_id=proposal,returned_cases=len(group),
            raw_consistent=(len(group)==repeats and all(r['usable'] for r in group) and len({r['prediction']['latex'] for r in group})==1) if repeats>1 else None))
    entry.update(conditional_localized_recognition=conditional,
        localized_recognition=dict(all_positive_repeat_cases=denominator,localized_repeat_cases=len(matched_cases),
            exact_matches=exact,formatting_normalized_matches=hits,
            all_positive_normalized_rate=hits/denominator if denominator else None,by_representation=strata),
        unclassified_output_diagnostics=diagnostics,per_proposal_repeat_consistency=consistency)


def evaluate(region_run,config_path,output,repeats=3,timeout=600):
    if type(repeats) is not int or repeats<1 or not math.isfinite(timeout) or timeout<=0: raise ValueError('invalid repeats/timeout')
    root=Path(region_run).resolve(strict=True);output=Path(output).absolute()
    if output.resolve().is_relative_to(root): raise ValueError('output must be outside the input run')
    source,candidates,snapshots=load_inputs(root);config_path=Path(config_path).resolve(strict=True)
    config=json.loads(config_path.read_text());backends=config['backends'];cases=build_cases(candidates,repeats)
    snapshots[str(config_path)]=digest(config_path)
    if not backends or any(not isinstance(b['id'],str) or not b['id'] for b in backends) or len({b['id'] for b in backends})!=len(backends):
        raise ValueError('nonempty unique backend IDs required')
    for backend in backends:
        if not backend['argv'] or any(not isinstance(arg,str) for arg in backend['argv']): raise ValueError('invalid worker argv')
        if cases:
            if not backend['artifacts']: raise ValueError('declared worker/model artifacts required')
            for artifact in backend['artifacts']:
                if digest(artifact['path'])!=artifact['sha256']: raise ValueError('model artifact hash mismatch')
    output.mkdir()
    request=dict(schema_version='omniattach/formula-worker/0.1',images=[dict(id=c['id'],path=c['image']) for c in cases])
    (output/'request.json').write_text(json.dumps(request,indent=2)+'\n')
    entries=[]
    for ordinal,backend in enumerate(backends):
        entry=dict(id=backend['id'],config=backend,status='not_run_no_candidates',requested_cases=len(cases),usable=0,rows=[])
        if cases:
            try:
                verify(snapshots);executable=shutil.which(backend['argv'][0])
                entry['executable_artifact']=dict(path=str(Path(executable).resolve()),sha256=digest(executable)) if executable else None
                response,wall,diagnostics=invoke(backend['argv'],request,timeout)
                entry.update(batch_wall_ms=wall,diagnostics=diagnostics)
                serialized=json.dumps(response,indent=2,allow_nan=False)
                (output/f'backend-{ordinal:03d}-response.json').write_text(serialized+'\n')
                if not isinstance(response,dict): raise ValueError('worker result must be an object')
                entry['metadata']=response.get('metadata',{})
                verify(snapshots);validated=score(response,cases)
                entry['rows']=[dict(id=r['id'],proposal_id=c['proposal_id'],image_sha256=r['image_sha256'],prediction=r['prediction'],usable=r['usable']) for r,c in zip(validated['rows'],cases)]
                entry.update(status='completed',usable=validated['usable'],reported_inference_ms=validated['formatting_metrics']['reported_inference_ms'])
            except (OSError,ValueError,KeyError,TypeError,RuntimeError,TimeoutError) as error:
                entry.update(status='failed',error=f'{type(error).__name__}: {error}',rows=[])
        entries.append(entry)
    # Only now are reference associations used for recognition scoring.
    association=associate(source)
    for entry in entries: summarize(entry,source,cases,association,repeats)
    comparisons=[]
    for a,b in itertools.combinations(entries,2):
        left={r['id']:r for r in a['rows']};right={r['id']:r for r in b['rows']}
        pairs=[(left[k]['prediction']['latex'],right[k]['prediction']['latex']) for k in left.keys()&right.keys() if left[k]['usable'] and right[k]['usable']]
        comparisons.append(dict(backends=[a['id'],b['id']],requested_cases=len(cases),comparable=len(pairs),
            exact_agreements=sum(x==y for x,y in pairs),formatting_policy_agreements=sum(formatting_equal(x,y) for x,y in pairs)))
    report=dict(schema_version='omniattach/proposed-formula-recognition/0.1',
        status='failed_candidate_control' if any(e['status']=='failed' for e in entries) else 'completed_candidate_control',
        ground_truth_prefiltered=False,equation_classifier=False,scoring_policy=POLICY,
        scope='all frozen proposals recognized; post-hoc fixture-localization-qualified string matching, not semantic detection accuracy; empty/nonempty outputs are not classification decisions',
        source_region_report_sha256=snapshots[str(root/'report.json')],source_proposals_sha256=snapshots[str(root/'proposals.json')],
        source_manifest_sha256=source['source_manifest_sha256'],config_sha256=snapshots[str(config_path)],
        repeats=repeats,proposal_occurrences=len(candidates),requested_cases_per_backend=len(cases),
        localization=dict(iou_threshold=source['iou_threshold'],reference_policy=source['reference_policy'],
            all_positive_occurrences=len(source['references']),matched_occurrences=len(association),
            missed_occurrences=len(source['references'])-len(association),matches=source['matches']),
        backends=entries,comparisons=comparisons,
        code_artifacts=[dict(path=str(path),sha256=digest(path)) for path in [Path(__file__),Path(regions.__file__),Path(__file__).with_name('lfe_equation_eval.py'),Path(__file__).with_name('lfe_equation_metrics.py')]])
    with (output/'report.json').open('x') as stream: json.dump(report,stream,indent=2,allow_nan=False);stream.write('\n')
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('region_run',type=Path);parser.add_argument('config',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--repeats',type=int,default=3);parser.add_argument('--timeout',type=float,default=600)
    args=parser.parse_args();report=evaluate(args.region_run,args.config,args.output,args.repeats,args.timeout)
    print(json.dumps(dict(status=report['status'],proposal_occurrences=report['proposal_occurrences'],requested_cases_per_backend=report['requested_cases_per_backend'])))
    sys.exit(int(report['status']!='completed_candidate_control'))
