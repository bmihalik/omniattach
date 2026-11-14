#!/usr/bin/env python3
"""LFE-4D1: recognize exported embedded formula assets, not reference PNGs."""
import argparse
from collections import Counter
import hashlib
import itertools
import json
import math
from pathlib import Path
import subprocess
import shutil
import sys
from PIL import Image

SUPPORTED = {'image_xobject', 'reused_image_xobject', 'rotated_scaled_image', 'cropbox_rotate'}


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def local(root, name):
    path = (root/name).resolve(strict=True)
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('unsafe asset path: '+name)
    return path


def rgb(path):
    with Image.open(path) as image:
        image = image.convert('RGB')
        return image.size, image.tobytes()


def anchor_key(element):
    anchor = element['anchor']
    forms = '['+', '.join(f'({a}, {b})' for a,b in anchor['form_path'])+']'
    return anchor['page'], f"content-operation {anchor['operation_visit']} forms {forms}"


def exported_images(inspection, document, directory):
    elements = {anchor_key(e): e for e in inspection['elements'] if e['kind']=='image'}
    if len(elements) != sum(e['kind']=='image' for e in inspection['elements']):
        raise ValueError('duplicate inspection anchors')
    assets = {a['id']: a for a in document.get('assets', [])}
    if len(assets) != len(document.get('assets', [])):
        raise ValueError('duplicate asset IDs')
    for asset in assets.values():
        path = local(directory, asset['filename'])
        if digest(path) != asset['sha256']:
            raise ValueError('exported asset hash mismatch')
    found = []
    seen = set()
    for block in document['blocks']:
        if block['kind']['type'] != 'image':
            continue
        span = block['source']; key = (span['page'], span['path'])
        if key not in elements or key in seen:
            raise ValueError('ambiguous projection-to-inspection anchor')
        seen.add(key)
        asset = assets[block['kind']['asset_id']]
        found.append(dict(element=elements[key], asset=asset,
                          path=str(local(directory, asset['filename']))))
    return sorted(found, key=lambda row: row['element']['sequence'])


def prepare(root, output, export):
    root = Path(root).resolve(strict=True)
    manifest_path = root/'manifest.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest['schema_version'] != 'omniattach/lfe-equation-fixtures/0.1':
        raise ValueError('unsupported corpus schema')
    documents = {d['path']: d for d in manifest['documents']}
    if len(documents) != len(manifest['documents']):
        raise ValueError('duplicate source PDFs')
    assets = {a['path']: a['sha256'] for a in manifest['assets']}
    for name, sha in [(d['path'],d['sha256']) for d in documents.values()]+list(assets.items()):
        if digest(local(root,name)) != sha:
            raise ValueError('corpus hash mismatch: '+name)
    occurrences = manifest['occurrences']
    if len({o['id'] for o in occurrences}) != len(occurrences):
        raise ValueError('duplicate occurrence IDs')
    for occurrence in occurrences:
        entry=documents[occurrence['pdf']]
        if entry.get('negative') or not 1 <= occurrence['page'] <= entry['pages']:
            raise ValueError('invalid occurrence page or negative-document label')
    output = Path(output).absolute()
    output.mkdir()
    (output/'exports').mkdir()
    rows = []; inventory = []
    for index, (name, entry) in enumerate(documents.items()):
        directory = output/'exports'/f'{index:04d}'
        inspection, document = export(local(root,name), directory)
        if inspection['schema_version'] != 'omniattach/pdf-inspection/0.1' or document['schema_version'] != 'omniattach-documents/0.2':
            raise ValueError('unsupported extraction output schema')
        if inspection['page_count'] != entry['pages'] or any(d['source']['sha256'] != entry['sha256'] for d in [inspection,document]):
            raise ValueError('source identity or page count mismatch')
        found = exported_images(inspection, document, directory)
        expected = [o for o in occurrences if o['pdf']==name]
        selected = [o for o in expected if o['representation'] in SUPPORTED]
        inventory.append(dict(pdf=name, negative=entry.get('negative',False),
            inspected_images=sum(e['kind']=='image' for e in inspection['elements']),
            exported_image_occurrences=len(found), assets=len(document.get('assets',[])),
            equation_hints=sum(e['kind']=='equation_candidate' for e in inspection['elements']),
            negative_image_free=not entry.get('negative',False) or not found,
            diagnostics=document.get('diagnostics',[])))
        for occurrence in expected:
            row=dict(occurrence, status='unsupported_representation', submitted=False)
            if occurrence['representation'] in SUPPORTED:
                page_expected=[o for o in selected if o['page']==occurrence['page']]
                page_found=[f for f in found if f['element']['anchor']['page']==occurrence['page']]
                row['status']='missing_or_ambiguous_image_occurrence'
                if len(page_expected)==len(page_found):
                    candidate=page_found[page_expected.index(occurrence)]
                    row.update(status='ready', image=candidate['path'], image_sha256=candidate['asset']['sha256'],
                        sequence=candidate['element']['sequence'], anchor=candidate['element']['anchor'])
                    bounds=candidate['element']['bounds']
                    actual=[bounds[k] for k in ['left','bottom','right','top']] if bounds else []
                    target=occurrence['bbox_pdf']
                    row['placement_ok']=bool(bounds and not bounds['estimated'] and len(target)==4 and
                        all(math.isfinite(x) and abs(x-y)<.01 for x,y in zip(actual,target)))
                    reference=occurrence['formula_image']
                    if reference not in assets:
                        raise ValueError('missing declared reference image')
                    row['pixel_exact']=rgb(row['image'])==rgb(local(root,reference))
            rows.append(row)
        if digest(local(root,name)) != entry['sha256']:
            raise ValueError('source PDF changed during export')
    for name, sha in assets.items():
        if digest(local(root,name)) != sha:
            raise ValueError('reference image changed during export')
    return dict(schema_version='omniattach/formula-pdf-evaluation/0.1',
        source_manifest_sha256=digest(manifest_path), occurrences=rows, documents=inventory,
        coverage=dict(total_positive_occurrences=len(rows), supported_occurrences=sum(o['representation'] in SUPPORTED for o in rows),
                      statuses=dict(Counter(o['status'] for o in rows))),
        scope='embedded XObject formula assets only; fixture page/traversal-ordinal association, not formula detection; original asset orientation; PDF default user-space bounds, not display-normalized placement; reference pixels used only for fidelity scoring')


def recognize(report, config, repeats, timeout):
    from lfe_equation_eval import digest as model_digest, invoke, score
    from lfe_equation_metrics import POLICY, formatting_equal
    if repeats < 1 or timeout <= 0:
        raise ValueError('positive repeats and timeout required')
    ready=[o for o in report['occurrences'] if o['status']=='ready']
    cases=[dict(id=f"{o['id']}:{i}", formula_id=o['formula_id'], sha256=o['image_sha256'],
                latex=o['latex'], image=o['image'], occurrence_id=o['id']) for o in ready for i in range(repeats)]
    request=dict(schema_version='omniattach/formula-worker/0.1', images=[dict(id=c['id'],path=c['image']) for c in cases])
    backends=config['backends']
    if not backends or len({b['id'] for b in backends}) != len(backends):
        raise ValueError('nonempty unique backend IDs required')
    results=[]
    strict={o['id']: o.get('pixel_exact',False) and o.get('placement_ok',False) for o in ready}
    for backend in backends:
        if not backend['artifacts']:
            raise ValueError('declared model artifacts required')
        for artifact in backend['artifacts']:
            if model_digest(artifact['path']) != artifact['sha256']:
                raise ValueError('model artifact hash mismatch: '+artifact['path'])
        for case in cases:
            if digest(case['image']) != case['sha256']:
                raise ValueError('extracted image changed before inference')
        entry=dict(id=backend['id'],config=backend,status='not_run_no_eligible_assets')
        if cases:
            try:
                executable=shutil.which(backend['argv'][0])
                entry['executable_artifact']=dict(path=str(Path(executable).resolve()),sha256=digest(executable)) if executable else None
                response, wall, diagnostic=invoke(backend['argv'],request,timeout)
                for case in cases:
                    if digest(case['image']) != case['sha256']:
                        raise ValueError('backend changed extracted image')
                scored=score(response,cases)
                for row, case in zip(scored['rows'],cases):
                    row['occurrence_id']=case['occurrence_id']
                hits=sum(r['formatting_normalized_exact'] for r in scored['rows'])
                strict_hits=sum(r['formatting_normalized_exact'] and strict[r['occurrence_id']] for r in scored['rows'])
                entry.update(status='completed',batch_wall_ms=wall,diagnostics=diagnostic,metadata=response.get('metadata',{}),
                    conditional_recognition=scored, normalized_matches=hits, strict_matches=strict_hits)
            except (OSError,ValueError,KeyError,TypeError,RuntimeError,TimeoutError) as error:
                entry.update(status='failed',error=str(error))
        total=len(report['occurrences'])*repeats
        entry['all_positive_repeat_cases']=total
        entry['all_positive_normalized_rate']=entry.get('normalized_matches',0)/total if total else None
        entry['all_positive_strict_rate']=entry.get('strict_matches',0)/total if total else None
        results.append(entry)
    comparisons=[]
    for a,b in itertools.combinations(results,2):
        x=a.get('conditional_recognition',{}).get('rows',[])
        y=b.get('conditional_recognition',{}).get('rows',[])
        pairs=[(u['prediction']['latex'],v['prediction']['latex']) for u,v in zip(x,y) if u['usable'] and v['usable']]
        comparisons.append(dict(backends=[a['id'],b['id']],requested_cases=len(cases),comparable=len(pairs),
            exact_agreements=sum(u==v for u,v in pairs),formatting_normalized_agreements=sum(formatting_equal(u,v) for u,v in pairs)))
    report.update(scoring_policy=POLICY,repeats=repeats,submitted_occurrences=len(ready),
        requested_cases_per_backend=len(cases),backends=results,comparisons=comparisons)
    for occurrence in ready:
        occurrence['submitted']=bool(cases)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('corpus',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--pdf-images',type=Path,required=True)
    parser.add_argument('--pdf-inspect',type=Path,required=True)
    parser.add_argument('--config',type=Path,help='omit for extraction/fidelity checks without models')
    parser.add_argument('--repeats',type=int,default=3);parser.add_argument('--timeout',type=float,default=600)
    args=parser.parse_args()
    if args.repeats<1 or args.timeout<=0: parser.error('positive repeats and timeout required')
    images=args.pdf_images.resolve(strict=True); inspect=args.pdf_inspect.resolve(strict=True)
    def export(pdf,directory):
        proc=subprocess.run([str(images),str(pdf),str(directory)],capture_output=True,timeout=args.timeout)
        if not directory.exists(): directory.mkdir()
        (directory/'export.stderr').write_bytes(proc.stderr)
        proc.check_returncode()
        proc=subprocess.run([str(inspect),str(pdf)],capture_output=True,timeout=args.timeout)
        (directory/'inspection.stderr').write_bytes(proc.stderr)
        proc.check_returncode()
        (directory/'inspection.json').write_bytes(proc.stdout)
        return json.loads(proc.stdout),json.loads((directory/'document.json').read_text())
    report=prepare(args.corpus,args.output,export)
    report['tool_artifacts']=[dict(path=str(p),sha256=digest(p)) for p in [images,inspect,Path(__file__).resolve(),Path(__file__).with_name('lfe_equation_eval.py').resolve(),Path(__file__).with_name('lfe_equation_metrics.py').resolve()]]
    if args.config:
        report['recognizer_config_sha256']=digest(args.config)
        recognize(report,json.loads(args.config.read_text()),args.repeats,args.timeout)
    bad=any(o['representation'] in SUPPORTED and (o['status']!='ready' or not o.get('pixel_exact') or not o.get('placement_ok')) for o in report['occurrences'])
    bad=bad or any(not d['negative_image_free'] for d in report['documents'])
    bad=bad or any(b['status']=='failed' for b in report.get('backends',[]))
    report['status']='failed' if bad else 'completed_scoped'
    with (args.output/'report.json').open('x') as stream:
        json.dump(report,stream,indent=2,allow_nan=False);stream.write('\n')
    print(json.dumps(dict(report=str(args.output/'report.json'),coverage=report['coverage'],status=report['status'])))
    return int(bad)


if __name__=='__main__': sys.exit(main())
