#!/usr/bin/env python3
"""Join reviewed labels to frozen proposals; audit pixels, groups and proxy roles."""
import argparse
import collections
import copy
import csv
import hashlib
import json
from pathlib import Path
from PIL import Image

CATEGORIES={'equation','non_equation','mixed','ambiguous','unreadable'}
def sha(data): return hashlib.sha256(data).hexdigest()
def digest(path): return sha(Path(path).read_bytes())
def unique(pairs):
    result={}
    for key,value in pairs:
        if key in result: raise ValueError('duplicate JSON key')
        result[key]=value
    return result
def indexed(rows,key):
    result={row[key]:row for row in rows}
    if len(result)!=len(rows): raise ValueError('duplicate IDs')
    return result

def audit(region_run,kit,labels_path,output,amendment=None):
    root=Path(region_run).resolve(strict=True);kit=Path(kit).resolve(strict=True);blind=kit/'blind';snapshots={}
    def read(path):
        path=Path(path).resolve(strict=True);raw=path.read_bytes();snapshots[str(path)]=sha(raw)
        return json.loads(raw,object_pairs_hook=unique)
    proposals=read(root/'proposals.json');source=read(root/'report.json');public=read(blind/'pack.json');mapping=read(kit/'private-map.json');labels=read(labels_path)
    if source['schema_version']!='omniattach/region-proposal-evaluation/0.1' or source['status']!='completed_proposal_baseline' or source['proposals']!=proposals['proposals']: raise ValueError('source D3 ledger mismatch')
    if public['schema_version']!='omniattach/blind-pack/0.1' or mapping['schema_version']!='omniattach/review-map/0.1' or labels['schema_version']!='omniattach/reviewed-labels/0.1': raise ValueError('unsupported input schema; use original labels plus explicit amendment')
    pack_hash=snapshots[str((blind/'pack.json').resolve())];labels_hash=snapshots[str(Path(labels_path).resolve())]
    if labels['pack_sha256']!=pack_hash or mapping['pack_sha256']!=pack_hash or mapping['source_proposals_sha256']!=snapshots[str((root/'proposals.json').resolve())]: raise ValueError('pack/source fingerprint mismatch')
    p=indexed(proposals['proposals'],'id');m=indexed(mapping['items'],'review_id');v=indexed(public['items'],'review_id');original=indexed(labels['items'],'review_id');effective=copy.deepcopy(original)
    if set(m)!=set(v) or set(m)!=set(original) or len(m)!=len(p) or {x['proposal_id'] for x in m.values()}!=set(p): raise ValueError('mapping/annotation coverage mismatch')
    changed=set()
    if amendment:
        supplied=read(amendment)
        if supplied['schema_version']!='omniattach/upright-amendment/0.1' or supplied['source_labels_sha256']!=labels_hash or supplied['pack_sha256']!=pack_hash or not isinstance(supplied['producer_id'],str) or not supplied['producer_id'].strip(): raise ValueError('invalid amendment provenance')
        changes=indexed(supplied['changes'],'review_id')
        for key,change in changes.items():
            if key not in effective or effective[key]['category']!='equation' or effective[key]['clockwise_degrees'] is not None or change['old'] is not None or type(change['new']) is not int or change['new']!=0: raise ValueError('amendment must explicitly change an unknown equation orientation to upright')
            effective[key]['clockwise_degrees']=0;changed.add(key)
    refs=indexed(source['references'],'id');matched={x['proposal_id']:x['reference_id'] for x in source['matches']}
    if len(matched)!=len(source['matches']) or len(set(matched.values()))!=len(matched) or not set(matched)<=set(p) or not set(matched.values())<=set(refs): raise ValueError('invalid frozen reference associations')
    pages=indexed(source['pages'],'id');rows=[];groups=collections.defaultdict(list)
    for key in v:
        label=effective[key];private=m[key];parent=p[private['proposal_id']];angle=label['clockwise_degrees'];category=label['category']
        if category not in CATEGORIES or (angle is not None and (type(angle) is not int or angle not in [0,90,180,270])) or (category!='equation' and angle is not None): raise ValueError('invalid label')
        source_image=Path(parent['image']).resolve(strict=True);review_image=(blind/v[key]['image']).resolve(strict=True)
        if not source_image.is_relative_to(root) or not review_image.is_relative_to(blind): raise ValueError('image outside input roots')
        if digest(source_image)!=parent['image_sha256'] or private['source_image_sha256']!=parent['image_sha256'] or digest(review_image)!=v[key]['sha256']: raise ValueError('image fingerprint mismatch')
        snapshots[str(source_image)]=parent['image_sha256'];snapshots[str(review_image)]=v[key]['sha256']
        with Image.open(source_image) as a,Image.open(review_image) as b:
            if a.format!='PNG' or b.format!='PNG' or a.mode!=b.mode or a.mode!='RGB' or a.size!=b.size or a.tobytes()!=b.tobytes(): raise ValueError('review/source pixels differ')
            group=sha(f'RGB:{a.width}:{a.height}:'.encode()+a.tobytes())
        if private['pixel_group']!=group: raise ValueError('pixel group mismatch')
        split='holdout' if int(group[:8],16)%5==0 else 'development'
        if private['provisional_split']!=split: raise ValueError('split assignment mismatch')
        role='matched_reference_proxy' if parent['id'] in matched else 'negative_page' if pages[parent['page_id']]['negative'] else 'unmatched_positive_page'
        row=dict(review_id=key,proposal_id=parent['id'],category=category,clockwise_degrees=angle,
            orientation_basis='user_upright_amendment' if key in changed else 'original_annotation',pixel_group=group,provisional_split=split,
            proxy_role=role,reference_id=matched.get(parent['id']),image_sha256=parent['image_sha256'])
        rows.append(row);groups[group].append(row)
    conflicts=[dict(pixel_group=group,review_ids=[x['review_id'] for x in members]) for group,members in groups.items() if len({(x['category'],x['clockwise_degrees']) for x in members})>1]
    unknown=[x['review_id'] for x in rows if x['category']=='equation' and x['clockwise_degrees'] is None]
    proxy_conflicts=[x['review_id'] for x in rows if (x['proxy_role']=='negative_page' and x['category']=='equation') or (x['proxy_role']=='matched_reference_proxy' and x['category']=='non_equation')]
    report=dict(schema_version='omniattach/mapped-label-audit/0.1',status='needs_review' if conflicts or unknown or proxy_conflicts else 'checked',
        source_labels_sha256=labels_hash,pack_sha256=pack_hash,source_report_sha256=snapshots[str((root/'report.json').resolve())],
        amendment_sha256=snapshots[str(Path(amendment).resolve())] if amendment else None,amended_review_ids=sorted(changed),
        candidates=len(rows),original_positive_occurrences=len(refs),matched_reference_proxies=len(matched),exact_pixel_groups=len(groups),categories=dict(collections.Counter(x['category'] for x in rows)),
        equation_orientations=dict(collections.Counter(str(x['clockwise_degrees']) for x in rows if x['category']=='equation')),
        duplicate_label_conflicts=conflicts,unknown_equation_orientation_ids=unknown,proxy_label_conflict_ids=proxy_conflicts,
        unmatched_equation_candidates=[x for x in rows if x['category']=='equation' and x['proxy_role']=='unmatched_positive_page'],
        categories_by_proxy_role={role:dict(collections.Counter(x['category'] for x in rows if x['proxy_role']==role)) for role in ['matched_reference_proxy','negative_page','unmatched_positive_page']},
        benchmark_validated=False,reviewer_independence_verified=False,selector_trained=False,
        scope='internal mapped annotation QA only; proxy disagreements require inspection, not automatic relabeling; provisional splits do not establish held-out generalization')
    if any(digest(path)!=expected for path,expected in snapshots.items()): raise ValueError('inputs changed during audit')
    output=Path(output).absolute()
    if any(output.resolve().is_relative_to(directory) for directory in [root,kit]): raise ValueError('output must be outside input directories')
    output.mkdir()
    with (output/'report.json').open('x') as stream: json.dump(report,stream,indent=2,allow_nan=False);stream.write('\n')
    with (output/'mapped-labels.csv').open('x',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=['review_id','proposal_id','category','clockwise_degrees','orientation_basis','pixel_group','provisional_split','proxy_role','reference_id','image_sha256']);writer.writeheader();writer.writerows(rows)
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['region_run','review_kit','labels','output']: parser.add_argument(name)
    parser.add_argument('--orientation-amendment');args=parser.parse_args()
    result=audit(args.region_run,args.review_kit,args.labels,args.output,args.orientation_amendment)
    print(json.dumps({k:result[k] for k in ['status','candidates','exact_pixel_groups','categories','equation_orientations']}))
