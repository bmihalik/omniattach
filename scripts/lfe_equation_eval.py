#!/usr/bin/env python3
"""Opt-in equation recognition comparison. Backends receive paths, never labels."""
import argparse
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import tempfile
import time


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def samples(root):
    root = Path(root).resolve()
    manifest = json.loads((root/'manifest.json').read_text())
    if manifest['schema_version'] != 'omniattach/lfe-equation-fixtures/0.1':
        raise ValueError('unsupported corpus schema')
    assets = {a['path']: a['sha256'] for a in manifest['assets']}
    found = {}
    for o in manifest['occurrences']:
        image = o.get('formula_image')
        if image is None:
            continue
        path = (root/image).resolve()
        if not path.is_relative_to(root) or digest(path) != assets[image]:
            raise ValueError('untrusted or changed image: '+image)
        key = o['formula_id']
        record = dict(id=key, image=str(path), sha256=assets[image], latex=o['latex'])
        if key in found and found[key] != record:
            raise ValueError('conflicting formula/image identity: '+key)
        found[key] = record
    if not found:
        raise ValueError('no labeled formula images')
    return list(found.values())


def invoke(argv, request, timeout):
    start = time.perf_counter()
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr,
                                start_new_session=(os.name == 'posix'))
        try:
            proc.communicate(json.dumps(request).encode(), timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name == 'posix':
                os.killpg(proc.pid, signal.SIGKILL)
            else:
                proc.kill()
            proc.wait()
            raise TimeoutError('backend batch deadline exceeded')
        stdout.seek(0)
        stderr.seek(0)
        data = stdout.read(8*1024*1024+1)
        diagnostic = stderr.read(65536).decode(errors='replace')
        if proc.returncode:
            raise RuntimeError(f'exit {proc.returncode}: {diagnostic}')
        if len(data) > 8*1024*1024:
            raise ValueError('backend JSON exceeds 8 MiB')
        return json.loads(data), (time.perf_counter()-start)*1000, diagnostic


def score(response, cases):
    predictions = response['results']
    by_id = {r['id']: r for r in predictions}
    if len(by_id) != len(predictions) or set(by_id) != {c['id'] for c in cases}:
        raise ValueError('missing, duplicate or unknown result IDs')
    rows = []
    for case in cases:
        pred = by_id[case['id']]
        status = pred['status']
        if status not in ['ok', 'error']:
            raise ValueError('unknown status')
        latex = pred.get('latex')
        if status == 'ok' and not isinstance(latex, str):
            raise ValueError('ok result requires LaTeX string')
        elapsed = pred.get('elapsed_ms')
        if elapsed is not None and (isinstance(elapsed, bool) or not isinstance(elapsed,(int,float)) or not math.isfinite(elapsed) or elapsed < 0):
            raise ValueError('invalid elapsed_ms')
        usable = status == 'ok' and bool(latex.strip())
        rows.append(dict(id=case['id'], formula_id=case['formula_id'], image_sha256=case['sha256'],
            expected=case['latex'], prediction=pred, usable=usable,
            exact=usable and latex == case['latex'],
            trimmed_exact=usable and latex.strip() == case['latex'].strip()))
    times = [r['prediction']['elapsed_ms'] for r in rows if r['usable'] and r['prediction'].get('elapsed_ms') is not None]
    n = len(rows)
    return dict(cases=n, usable=sum(r['usable'] for r in rows),
        exact=sum(r['exact'] for r in rows), trimmed_exact=sum(r['trimmed_exact'] for r in rows),
        exact_rate=sum(r['exact'] for r in rows)/n,
        median_reported_inference_ms=statistics.median(times) if times else None,
        rows=rows)


def evaluate(root, config, repeats=1, timeout=600):
    if repeats < 1 or timeout <= 0:
        raise ValueError('positive repeats and timeout required')
    source = samples(root)
    cases = [dict(s, id=f"{s['id']}:{i}", formula_id=s['id']) for s in source for i in range(repeats)]
    request = dict(schema_version='omniattach/formula-worker/0.1',
                   images=[dict(id=c['id'], path=c['image']) for c in cases])
    backends = config['backends']
    if not backends or len({b['id'] for b in backends}) != len(backends):
        raise ValueError('backend IDs must be nonempty and unique')
    reports = []
    for backend in backends:
        for item in source:
            if digest(item['image']) != item['sha256']:
                raise ValueError('input changed between backends')
        # Provisioning errors are configuration failures, never scored as OCR results.
        if not backend['artifacts']:
            raise ValueError('declare local model/tokenizer artifacts')
        for item in backend['artifacts']:
            if digest(item['path']) != item['sha256']:
                raise ValueError('model artifact hash mismatch: '+item['path'])
        entry = dict(id=backend['id'], config=backend)
        start = time.perf_counter()
        try:
            response, wall, diagnostic = invoke(backend['argv'], request, timeout)
            for item in source:
                if digest(item['image']) != item['sha256']:
                    raise ValueError('backend modified an input image')
            result = score(response, cases)
            entry.update(status='completed', batch_wall_ms=wall, diagnostics=diagnostic,
                         metadata=response.get('metadata',{}), **result)
        except (OSError, ValueError, KeyError, TypeError, RuntimeError, TimeoutError) as error:
            entry.update(status='failed', error=str(error), batch_wall_ms=(time.perf_counter()-start)*1000,
                         cases=len(cases), usable=0, exact=0, exact_rate=0, rows=[])
        reports.append(entry)
    comparisons = []
    for a,b in itertools.combinations(reports,2):
        pairs = list(zip(a['rows'],b['rows']))
        comparisons.append(dict(backends=[a['id'],b['id']], total_cases=len(cases),
            comparable=sum(x['usable'] and y['usable'] for x,y in pairs),
            exact_agreements=sum(x['usable'] and y['usable'] and x['prediction']['latex']==y['prediction']['latex'] for x,y in pairs)))
    return dict(schema_version='omniattach/formula-evaluation/0.1',
        corpus_manifest_sha256=digest(Path(root)/'manifest.json'),
        environment=dict(python=platform.python_version(), platform=platform.platform()),
        unique_formulas=len(source), repeats=repeats, backends=reports, comparisons=comparisons,
        scope='clean image recognition only; string accuracy, not semantic equivalence or PDF detection; completion/truncation and peak memory not measured')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('corpus',type=Path)
    parser.add_argument('config',type=Path)
    parser.add_argument('output',type=Path)
    parser.add_argument('--repeats',type=int,default=1)
    parser.add_argument('--timeout',type=float,default=600)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    with args.output.open('x') as out:
        report = evaluate(args.corpus,config,args.repeats,args.timeout)
        json.dump(report,out,indent=2,allow_nan=False)
        out.write('\n')
    raise SystemExit(1 if any(b['status']=='failed' for b in report['backends']) else 0)
