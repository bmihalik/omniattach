#!/usr/bin/env python3
"""D5 controls: fixed crop transforms and separate imported selector decisions."""
import argparse
import json
import math
from pathlib import Path
import shutil
import sys
from PIL import Image,ImageOps
import lfe_equation_candidate_eval as d4
from lfe_equation_eval import digest,invoke,score

POLICIES={'identity':(0,0),'pad16':(0,16),'ccw90_pad16':(90,16),'cw90_pad16':(270,16)}


def write(path,data):
    with Path(path).open('x') as stream: json.dump(data,stream,indent=2,allow_nan=False);stream.write('\n')


def transform(image,policy):
    angle,padding=POLICIES[policy]
    if image.mode!='RGB': raise ValueError('D3 RGB crops required')
    image=image.copy()
    if angle: image=image.transpose(Image.Transpose.ROTATE_90 if angle==90 else Image.Transpose.ROTATE_270)
    return ImageOps.expand(image,border=padding,fill='white') if padding else image


def outside(output,*inputs):
    output=Path(output).absolute()
    if any(output.resolve().is_relative_to(Path(root).resolve()) for root in inputs): raise ValueError('output must be outside input directories')
    return output


def prepare(region_run,output,repeats=1):
    if type(repeats) is not int or repeats<1: raise ValueError('positive integer repeats required')
    root=Path(region_run).resolve(strict=True);output=outside(output,root)
    source,candidates,snapshots=d4.load_inputs(root)
    output.mkdir();records=[]
    for policy in POLICIES:
        directory=output/policy;directory.mkdir();derived=[]
        for index,item in enumerate(candidates):
            target=(directory/f'{index:04d}.png').resolve()
            if policy=='identity': shutil.copyfile(item['image'],target)
            else:
                with Image.open(item['image']) as image: transform(image,policy).save(target)
            derived.append(dict(id=item['id'],image=str(target),image_sha256=digest(target),source_sha256=item['image_sha256']))
        cases=d4.build_cases(derived,repeats)
        request=dict(schema_version='omniattach/formula-worker/0.1',images=[dict(id=c['id'],path=c['image']) for c in cases])
        write(directory/'request.json',request)
        records.append(dict(policy=policy,candidates=derived))
    d4.verify(snapshots)
    write(output/'selector-input.json',dict(schema_version='omniattach/selector-input/0.1',
        source_proposals_sha256=digest(root/'proposals.json'),
        images=[dict(id=c['id'],path=c['image'],sha256=c['image_sha256']) for c in candidates]))
    write(output/'selector-template.json',dict(schema_version='omniattach/selector-decisions/0.1',
        producer_id='UNCONFIGURED',source_proposals_sha256=digest(root/'proposals.json'),
        decisions=[dict(id=c['id'],decision='abstain') for c in candidates]))
    plan=dict(schema_version='omniattach/crop-controls/0.1',source_root=str(root),
        source_report_sha256=digest(root/'report.json'),source_proposals_sha256=digest(root/'proposals.json'),
        tool_sha256=digest(__file__),repeats=repeats,policies=records,
        proposal_occurrences=len(candidates),requests_per_policy=len(candidates)*repeats,
        requests_per_backend=len(candidates)*repeats*len(POLICIES),
        scope='fixed white-padding/quarter-turn recognition controls; original localization held fixed; no best-of-policy fusion or classifier')
    write(output/'plan.json',plan)
    return plan


def read_plan(directory):
    directory=Path(directory).resolve(strict=True);plan=json.loads((directory/'plan.json').read_text())
    if plan['schema_version']!='omniattach/crop-controls/0.1' or plan['tool_sha256']!=digest(__file__): raise ValueError('plan schema/code changed')
    source,original,snapshots=d4.load_inputs(plan['source_root']);root=Path(plan['source_root'])
    if plan['source_report_sha256']!=digest(root/'report.json') or plan['source_proposals_sha256']!=digest(root/'proposals.json'): raise ValueError('source D3 run changed')
    if type(plan['repeats']) is not int or plan['repeats']<1 or [p['policy'] for p in plan['policies']]!=list(POLICIES): raise ValueError('invalid fixed policy grid')
    snapshots[str(directory/'plan.json')]=digest(directory/'plan.json')
    for record in plan['policies']:
        policy=record['policy'];derived=record['candidates']
        if len(derived)!=len(original) or [c['id'] for c in derived]!=[c['id'] for c in original]: raise ValueError('missing/reordered/duplicate proposals')
        for item,parent in zip(derived,original):
            path=Path(item['image']).resolve(strict=True)
            if not path.is_relative_to(directory) or item['source_sha256']!=parent['image_sha256'] or digest(path)!=item['image_sha256']: raise ValueError('derived crop provenance mismatch')
            with Image.open(parent['image']) as image,Image.open(path) as actual:
                expected=transform(image,policy)
                if actual.format!='PNG' or actual.mode!='RGB' or actual.size!=expected.size or actual.tobytes()!=expected.tobytes(): raise ValueError('derived pixels do not implement declared policy')
            if policy=='identity' and digest(path)!=parent['image_sha256']: raise ValueError('identity must preserve PNG bytes')
            snapshots[str(path)]=item['image_sha256']
        cases=d4.build_cases(derived,plan['repeats'])
        request=dict(schema_version='omniattach/formula-worker/0.1',images=[dict(id=c['id'],path=c['image']) for c in cases])
        request_path=directory/policy/'request.json'
        if json.loads(request_path.read_text())!=request: raise ValueError('request changed')
        snapshots[str(request_path)]=digest(request_path)
    expected_count=len(original)*plan['repeats']
    if (plan['proposal_occurrences'],plan['requests_per_policy'],plan['requests_per_backend'])!=(len(original),expected_count,expected_count*len(POLICIES)): raise ValueError('plan denominator changed')
    return plan,source,snapshots


def run(plan_directory,config_path,output,timeout=1800):
    if not math.isfinite(timeout) or timeout<=0: raise ValueError('positive finite timeout required')
    plan,source,snapshots=read_plan(plan_directory);output=outside(output,plan_directory,plan['source_root'])
    config_path=Path(config_path).resolve(strict=True);config=json.loads(config_path.read_text());backends=config['backends']
    if not backends or any(not isinstance(b['id'],str) or not b['id'] for b in backends) or len({b['id'] for b in backends})!=len(backends): raise ValueError('unique nonempty backend IDs required')
    for backend in backends:
        if not backend['argv'] or any(not isinstance(arg,str) for arg in backend['argv']): raise ValueError('invalid worker argv')
        if plan['requests_per_policy']:
            if not backend['artifacts']: raise ValueError('declared worker/model artifacts required')
            for artifact in backend['artifacts']:
                if digest(artifact['path'])!=artifact['sha256']: raise ValueError('model artifact hash mismatch')
    snapshots[str(config_path)]=digest(config_path);output.mkdir();pending=[]
    for record in plan['policies']:
        policy=record['policy'];cases=d4.build_cases(record['candidates'],plan['repeats'])
        request=dict(schema_version='omniattach/formula-worker/0.1',images=[dict(id=c['id'],path=c['image']) for c in cases])
        for index,backend in enumerate(backends):
            entry=dict(policy=policy,id=backend['id'],config=backend,requested_cases=len(cases),usable=0,status='not_run_no_candidates',rows=[])
            if cases:
                try:
                    d4.verify(snapshots)
                    response,wall,diagnostics=invoke(backend['argv'],request,timeout)
                    entry.update(batch_wall_ms=wall,diagnostics=diagnostics)
                    write(output/f'{policy}-backend-{index:03d}.json',response)
                    if not isinstance(response,dict): raise ValueError('worker result must be an object')
                    entry['metadata']=response.get('metadata',{})
                    d4.verify(snapshots);validated=score(response,cases)
                    entry.update(status='completed',usable=validated['usable'],reported_inference_ms=validated['formatting_metrics']['reported_inference_ms'],
                        rows=[dict(id=row['id'],proposal_id=case['proposal_id'],image_sha256=row['image_sha256'],prediction=row['prediction'],usable=row['usable']) for row,case in zip(validated['rows'],cases)])
                except (OSError,ValueError,KeyError,TypeError,RuntimeError,TimeoutError) as error:
                    entry.update(status='failed',error=f'{type(error).__name__}: {error}',rows=[])
            pending.append((entry,cases))
    association=d4.associate(source)
    for entry,cases in pending: d4.summarize(entry,source,cases,association,plan['repeats'])
    report=dict(schema_version='omniattach/crop-control-results/0.1',status='failed' if any(e['status']=='failed' for e,c in pending) else 'completed',
        plan_sha256=digest(Path(plan_directory)/'plan.json'),tool_sha256=digest(__file__),config_sha256=digest(config_path),
        repeats=plan['repeats'],requests_per_backend=plan['requests_per_backend'],
        scope=plan['scope'],equation_classifier=False,ground_truth_policy_selection=False,
        original_positive_occurrences=len(source['references']),original_matched_occurrences=len(association),
        results=[entry for entry,cases in pending])
    write(output/'report.json',report);return report


def select_audit(region_run,decisions_path,output):
    source,candidates,snapshots=d4.load_inputs(region_run);decisions_path=Path(decisions_path)
    snapshots[str(decisions_path.resolve())]=digest(decisions_path)
    supplied=json.loads(decisions_path.read_text())
    if supplied['schema_version']!='omniattach/selector-decisions/0.1' or not isinstance(supplied['producer_id'],str) or supplied['producer_id'] in ('','UNCONFIGURED'): raise ValueError('named decision producer required')
    if supplied['source_proposals_sha256']!=digest(Path(region_run)/'proposals.json'): raise ValueError('decisions belong to another proposal run')
    items=supplied['decisions'];decisions={item['id']:item['decision'] for item in items}
    if len(items)!=len(decisions) or set(decisions)!={c['id'] for c in candidates} or not set(decisions.values())<={'accept','reject','abstain'}: raise ValueError('one explicit decision per original proposal required')
    association=d4.associate(source);negative_pages={p['id'] for p in source['pages'] if p['negative']}
    negative={p['id'] for p in candidates if p['page_id'] in negative_pages};positive=set(association)
    if positive&negative: raise ValueError('contradictory source roles')
    groups=dict(matched_reference_proxy=positive,known_negative_page=negative,unlabeled=set(decisions)-positive-negative)
    counts={name:{value:sum(decisions[k]==value for k in ids) for value in ['accept','reject','abstain']} for name,ids in groups.items()}
    accepted=counts['matched_reference_proxy']['accept'];total=len(source['references']);d4.verify(snapshots)
    report=dict(schema_version='omniattach/selector-audit/0.1',producer_id=supplied['producer_id'],selector_executed=False,
        decisions_sha256=digest(decisions_path),source_proposals_sha256=supplied['source_proposals_sha256'],
        proposal_occurrences=len(candidates),all_positive_occurrences=total,matched_proxy_occurrences=len(positive),counts=counts,
        conditional_proxy_retention=accepted/len(positive) if positive else None,
        all_positive_proxy_retention=accepted/total if total else None,
        scope='imported decisions only; producer leakage/provenance not independently verified; unmatched positive-page candidates remain unlabeled; no semantic precision/F1')
    output=Path(output).absolute()
    if output.resolve().is_relative_to(Path(region_run).resolve()) or output.resolve()==decisions_path.resolve(): raise ValueError('audit output must not replace inputs')
    write(output,report);return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    p=commands.add_parser('prepare');p.add_argument('region_run');p.add_argument('output');p.add_argument('--repeats',type=int,default=1)
    p=commands.add_parser('run');p.add_argument('plan_directory');p.add_argument('config');p.add_argument('output');p.add_argument('--timeout',type=float,default=1800)
    p=commands.add_parser('selector-audit');p.add_argument('region_run');p.add_argument('decisions');p.add_argument('output')
    args=parser.parse_args()
    if args.command=='prepare':
        result=prepare(args.region_run,args.output,args.repeats);print(json.dumps({k:result[k] for k in ['proposal_occurrences','requests_per_policy','requests_per_backend','repeats']}))
    elif args.command=='run':
        result=run(args.plan_directory,args.config,args.output,args.timeout);print(json.dumps(dict(status=result['status'],requests_per_backend=result['requests_per_backend'])));sys.exit(int(result['status']!='completed'))
    else:
        result=select_audit(args.region_run,args.decisions,args.output);print(json.dumps(dict(proposal_occurrences=result['proposal_occurrences'],counts=result['counts'])))
