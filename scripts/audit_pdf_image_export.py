#!/usr/bin/env python3
"""Audit a native pdf-images CLI export using only Python 3.10+ stdlib.

No extracted prose is printed. Exact comparisons are not semantic fidelity scores.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote


def load_json(path, limit):
    with path.open('rb') as handle:
        raw = handle.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('JSON exceeds configured read limit')
    return json.loads(raw)


def contained_file(root, relative):
    if not isinstance(relative, str) or '\\' in relative or ':' in relative:
        raise ValueError('unsafe artifact path')
    parts = relative.split('/')
    if any(part in ('', '.', '..') for part in parts):
        raise ValueError('unsafe artifact path')
    path = root
    for part in parts:
        path = path / part
        if path.is_symlink():
            raise ValueError('symlink artifact')
    path.resolve().relative_to(root)
    if not path.is_file():
        raise ValueError('missing artifact file')
    return path


def file_hash(path, remaining):
    digest = hashlib.sha256()
    length = 0
    with path.open('rb') as handle:
        while True:
            chunk = handle.read(min(65536, max(remaining - length + 1, 1)))
            if not chunk:
                break
            length += len(chunk)
            if length > remaining:
                raise ValueError('image read byte budget exceeded')
            digest.update(chunk)
    return length, digest.hexdigest()


def prose(document):
    return '\n\n'.join(block.get('text', '') for block in document.get('blocks', [])
                       if block.get('kind', {}).get('type') == 'paragraph')


def prose_summary(text):
    normalized = ' '.join(text.split())
    alphanumeric = ''.join(c for c in text if c.isalnum())
    return {
        'whitespace_tokens': len(text.split()),
        'normalized_sha256': hashlib.sha256(normalized.encode()).hexdigest(),
        'alphanumeric_characters': len(alphanumeric),
        'alphanumeric_sha256': hashlib.sha256(alphanumeric.encode()).hexdigest(),
    }


def audit(root, reference=None, json_limit=67108864, image_limit=536870912):
    root = root.resolve(strict=True)
    document = load_json(contained_file(root, 'document.json'), json_limit)
    manifest = load_json(contained_file(root, 'manifest.json'), json_limit)
    markdown_path = contained_file(root, 'document.md')
    if markdown_path.stat().st_size > json_limit:
        raise ValueError('Markdown exceeds configured read limit')
    markdown = markdown_path.read_text(encoding='utf-8')
    issues = []
    artifacts = {}
    for artifact in manifest.get('artifacts', []):
        name = artifact.get('path')
        if name in artifacts:
            issues.append('duplicate manifest artifact')
        artifacts[name] = artifact
        path = contained_file(root, name)
        if path.stat().st_size != artifact.get('bytes'):
            issues.append('artifact size disagrees with manifest')
    for name in ('document.md', 'document.json'):
        if name not in artifacts:
            issues.append('document output missing from manifest artifacts')
    assets = {}
    names = set()
    total = 0
    for asset in document.get('assets', []):
        asset_id, name = asset.get('id'), asset.get('filename')
        if not isinstance(asset_id, str) or not isinstance(name, str) or not name.startswith('images/'):
            raise ValueError('invalid image asset metadata')
        if asset_id in assets or name in names:
            issues.append('duplicate image asset ID or path')
        names.add(name)
        assets[asset_id] = name
        path = contained_file(root, name)
        length, digest = file_hash(path, image_limit - total)
        total += length
        if asset.get('byte_length') != length or asset.get('sha256') != digest:
            issues.append('saved image length/hash disagrees with document')
        embedded = asset.get('bytes')
        if not isinstance(embedded, list) or any(type(n) is not int or not 0 <= n <= 255 for n in embedded):
            issues.append('invalid embedded image byte array')
        elif len(embedded) != length or hashlib.sha256(bytes(embedded)).hexdigest() != digest:
            issues.append('saved and embedded image bytes disagree')
        declared = artifacts.get(name)
        if declared is None or declared.get('media_type') != asset.get('media_type'):
            issues.append('image missing or media type differs in manifest')
        else:
            location = declared.get('location') or {}
            if location.get('sha256') != digest or location.get('asset_id') != asset_id:
                issues.append('image artifact location hash/ID disagrees')
    expected = Counter()
    image_blocks = 0
    for block in document.get('blocks', []):
        kind = block.get('kind', {})
        if kind.get('type') == 'image':
            image_blocks += 1
            name = assets.get(kind.get('asset_id'))
            if name is None:
                issues.append('image block references a missing asset')
            else:
                expected[name] += 1
    links = re.findall(r'(?<!\\)!\[(?:\\.|[^\]\\])*\]\(([^)\s]+)\)', markdown)
    actual = Counter()
    for target in links:
        name = unquote(target)
        contained_file(root, name)
        actual[name] += 1
        if name not in names or name not in artifacts:
            issues.append('Markdown image target is not a declared image asset')
    if actual != expected:
        issues.append('Markdown image occurrence counts disagree with typed blocks')
    if set(actual) != names:
        issues.append('unreferenced or missing image assets')
    native_source = document.get('source', {}).get('sha256')
    if native_source != manifest.get('source', {}).get('sha256'):
        issues.append('document and facade source hashes disagree')
    result = {
        'audit_schema': 'omniattach/pdf-image-export-audit/0.1',
        'ok': not issues, 'issues': sorted(set(issues)),
        'source_sha256': native_source, 'image_assets': len(assets),
        'image_blocks': image_blocks, 'markdown_image_links': sum(actual.values()),
        'saved_image_bytes': total, 'paragraph_text': prose_summary(prose(document)),
        'diagnostic_codes': dict(Counter(d.get('code', 'UNKNOWN') for d in document.get('diagnostics', []))),
    }
    if reference is not None:
        reference = reference.resolve(strict=True)
        other = load_json(contained_file(reference, 'document.json'), json_limit)
        summary = prose_summary(prose(other))
        same_source = native_source == other.get('source', {}).get('sha256')
        result['reference_comparison'] = {
            'same_source': same_source, 'paragraph_text': summary,
            'whitespace_normalized_equal': result['paragraph_text']['normalized_sha256'] == summary['normalized_sha256'],
            'alphanumeric_sequence_equal': result['paragraph_text']['alphanumeric_sha256'] == summary['alphanumeric_sha256'],
            'note': 'Paragraph-only comparison; differences may include furniture/normalization. Equality is not a visual-order or semantic-fidelity proof.',
        }
        if not same_source:
            result['ok'] = False
            result['issues'].append('reference is not an export of the same source bytes')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('export', type=Path)
    parser.add_argument('--reference', type=Path)
    parser.add_argument('--report', type=Path, help='NEW JSON report file; never overwrite')
    parser.add_argument('--max-json-bytes', type=int, default=67108864)
    parser.add_argument('--max-image-bytes', type=int, default=536870912)
    args = parser.parse_args()
    if args.max_json_bytes <= 0 or args.max_image_bytes <= 0:
        parser.error('byte budgets must be positive')
    try:
        result = audit(args.export, args.reference, args.max_json_bytes, args.max_image_bytes)
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        result = {'audit_schema': 'omniattach/pdf-image-export-audit/0.1', 'ok': False, 'issues': [type(error).__name__ + ': export structure/read validation failed']}
    rendered = json.dumps(result, indent=2, sort_keys=True) + '\n'
    if args.report:
        with args.report.open('x', encoding='utf-8') as handle:
            handle.write(rendered)
    sys.stdout.write(rendered)
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
