#!/usr/bin/env python3
"""D5B: blind candidate labels, independent review comparison and adjudication."""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
from PIL import Image

CATEGORIES={'equation','non_equation','mixed','ambiguous','unreadable'}

def sha(data): return hashlib.sha256(data).hexdigest()
def digest(path): return sha(Path(path).read_bytes())
def write(path,data):
    with Path(path).open('x') as stream: json.dump(data,stream,indent=2,allow_nan=False);stream.write('\n')
def unique(pairs):
    result={}
    for key,value in pairs:
        if key in result: raise ValueError('duplicate JSON key')
        result[key]=value
    return result
def load(path): return json.loads(Path(path).read_text(),object_pairs_hook=unique)
def blank(key): return dict(review_id=key,category=None,clockwise_degrees=None,notes='')

def prepare(region_run,output):
    root=Path(region_run).resolve(strict=True);output=Path(output).absolute()
    if output.resolve().is_relative_to(root): raise ValueError('output must be outside source run')
    source=root/'proposals.json';source_hash=digest(source);items=load(source)['proposals']
    ids=[p['id'] for p in items]
    if any(not isinstance(key,str) or not key for key in ids) or len(set(ids))!=len(ids): raise ValueError('invalid proposal IDs')
    salt=secrets.token_hex(16);items=sorted(items,key=lambda p:sha((salt+p['id']).encode()))
    output.mkdir();blind=output/'blind';blind.mkdir();(blind/'images').mkdir()
    public=[];private=[];snapshots={str(source):source_hash}
    for i,item in enumerate(items):
        path=Path(item['image']).resolve(strict=True)
        if not path.is_relative_to(root) or digest(path)!=item['image_sha256']: raise ValueError('source crop changed/outside run')
        snapshots[str(path)]=item['image_sha256'];key=f'r{i:04d}'
        with Image.open(path) as image:
            if image.format!='PNG' or image.mode!='RGB': raise ValueError('RGB PNG source crops required')
            pixels=image.tobytes();size=image.size
        group=sha(f'RGB:{size[0]}:{size[1]}:'.encode()+pixels)
        target=blind/'images'/f'{key}.png'
        Image.frombytes('RGB',size,pixels).save(target)
        public.append(dict(review_id=key,image=f'images/{key}.png',sha256=digest(target)))
        private.append(dict(review_id=key,proposal_id=item['id'],source_image_sha256=item['image_sha256'],
            pixel_group=group,provisional_split='holdout' if int(group[:8],16)%5==0 else 'development'))
    if any(digest(path)!=expected for path,expected in snapshots.items()): raise ValueError('source changed during preparation')
    write(blind/'pack.json',dict(schema_version='omniattach/blind-pack/0.1',items=public))
    pack_hash=digest(blind/'pack.json')
    (blind/'index.html').write_text('<!doctype html><meta charset="utf-8"><title>Blind crop review</title><h1>Blind crop review</h1><p>Edit a review JSON copy; judge pixels only. Clockwise corrections: 0, 90, 180, 270, or null.</p>'+''.join(f'<section><h2>{p["review_id"]}</h2><img src="{p["image"]}" alt="{p["review_id"]}"></section>' for p in public))
    for name in ['review-a.json','review-b.json']:
        write(blind/name,dict(schema_version='omniattach/candidate-review/0.1',pack_sha256=pack_hash,reviewer_id='UNSET',items=[blank(p['review_id']) for p in public]))
    write(output/'private-map.json',dict(schema_version='omniattach/review-map/0.1',pack_sha256=pack_hash,
        source_proposals_sha256=source_hash,shuffle_salt=salt,items=private,
        warning='Exact-pixel grouping only; formula families and near duplicates may cross provisional splits. Not an independent benchmark.'))
    return dict(candidates=len(public),exact_pixel_groups=len({p['pixel_group'] for p in private}),pack_sha256=pack_hash)

def pack(directory):
    directory=Path(directory).resolve(strict=True);manifest=load(directory/'pack.json')
    if set(manifest)!={'schema_version','items'} or manifest['schema_version']!='omniattach/blind-pack/0.1': raise ValueError('invalid blind pack')
    ids=[];paths=set()
    for item in manifest['items']:
        if set(item)!={'review_id','image','sha256'} or not isinstance(item['review_id'],str) or not item['review_id']: raise ValueError('invalid blind item')
        path=(directory/item['image']).resolve(strict=True)
        if not path.is_relative_to(directory) or path in paths or digest(path)!=item['sha256']: raise ValueError('pack image changed/outside pack')
        with Image.open(path) as image:
            if image.format!='PNG' or image.mode!='RGB': raise ValueError('invalid pack image')
            image.verify()
        ids.append(item['review_id']);paths.add(path)
    if len(set(ids))!=len(ids): raise ValueError('duplicate blind IDs')
    return ids,digest(directory/'pack.json')

def labels(document,expected_ids):
    rows=document['items'];result={}
    for row in rows:
        if set(row)!={'review_id','category','clockwise_degrees','notes'}: raise ValueError('invalid annotation fields')
        key=row['review_id'];category=row['category'];angle=row['clockwise_degrees']
        if key in result or category not in CATEGORIES or not isinstance(row['notes'],str): raise ValueError('incomplete/duplicate annotation')
        if angle is not None and (type(angle) is not int or angle not in [0,90,180,270]): raise ValueError('invalid clockwise correction')
        if category!='equation' and angle is not None: raise ValueError('only equation crops receive orientation labels')
        result[key]=row
    if set(result)!=set(expected_ids): raise ValueError('missing/unknown annotation IDs')
    if not isinstance(document['reviewer_id'],str) or not document['reviewer_id'].strip() or document['reviewer_id']=='UNSET': raise ValueError('named reviewer required')
    return result

def compare(directory,review_a,review_b,output,adjudication=None):
    ids,pack_hash=pack(directory);review_hashes=[digest(review_a),digest(review_b)];a=load(review_a);b=load(review_b)
    for doc in [a,b]:
        if doc['schema_version']!='omniattach/candidate-review/0.1' or doc['pack_sha256']!=pack_hash: raise ValueError('review belongs to another pack')
    left=labels(a,ids);right=labels(b,ids)
    if a['reviewer_id'].strip().casefold()==b['reviewer_id'].strip().casefold(): raise ValueError('two distinct reviewer identities required')
    same=lambda x,y:(x['category'],x['clockwise_degrees'])==(y['category'],y['clockwise_degrees'])
    conflicts=[key for key in ids if not same(left[key],right[key])]
    pair_hash=sha(json.dumps([pack_hash,*review_hashes],separators=(',',':')).encode())
    resolved={};adjudication_hash=None
    if adjudication:
        adjudication_hash=digest(adjudication);doc=load(adjudication)
        if doc['schema_version']!='omniattach/candidate-adjudication/0.1' or doc['review_pair_sha256']!=pair_hash: raise ValueError('adjudication belongs to another review pair')
        resolved=labels(doc,conflicts)
        if digest(adjudication)!=adjudication_hash: raise ValueError('adjudication changed')
    if [digest(review_a),digest(review_b)]!=review_hashes or pack(directory)!=(ids,pack_hash): raise ValueError('review inputs changed')
    output=Path(output).absolute()
    if output.resolve().is_relative_to(Path(directory).resolve()): raise ValueError('comparison output must be outside blind pack')
    output.mkdir()
    report=dict(schema_version='omniattach/review-comparison/0.1',pack_sha256=pack_hash,review_pair_sha256=pair_hash,
        reviewers=[a['reviewer_id'],b['reviewer_id']],candidates=len(ids),type_agreements=sum(left[k]['category']==right[k]['category'] for k in ids),
        joint_label_agreements=len(ids)-len(conflicts),disagreements=len(conflicts),adjudicated=len(resolved),
        status='pending_adjudication' if conflicts and not adjudication else 'complete',
        rows=[dict(review_id=k,review_a=left[k],review_b=right[k],agreed=same(left[k],right[k])) for k in ids],
        scope='declared reviewer identities; independence not technically verified; no classifier/orientation inference or accuracy measured')
    write(output/'report.json',report)
    write(output/'adjudication-template.json',dict(schema_version='omniattach/candidate-adjudication/0.1',review_pair_sha256=pair_hash,reviewer_id='UNSET',items=[blank(k) for k in conflicts]))
    if report['status']=='complete':
        write(output/'labels.json',dict(schema_version='omniattach/reviewed-labels/0.1',pack_sha256=pack_hash,review_pair_sha256=pair_hash,
            adjudication_sha256=adjudication_hash,items=[dict(resolved[k] if k in resolved else left[k],basis='adjudicated' if k in resolved else 'reviewer_agreement') for k in ids],
            scope='reviewed annotations, not selector predictions; ambiguous/mixed/unreadable and unknown orientations remain explicit'))
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    p=commands.add_parser('prepare');p.add_argument('region_run');p.add_argument('output')
    p=commands.add_parser('compare');p.add_argument('blind_directory');p.add_argument('review_a');p.add_argument('review_b');p.add_argument('output');p.add_argument('--adjudication')
    args=parser.parse_args()
    result=prepare(args.region_run,args.output) if args.command=='prepare' else compare(args.blind_directory,args.review_a,args.review_b,args.output,args.adjudication)
    print(json.dumps({k:v for k,v in result.items() if k!='rows'}))
