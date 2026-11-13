#!/usr/bin/env python3
"""Compare paragraph text globally, not by source.page; Python 3.10+ stdlib.

Usage: python3 scripts/compare_pdf_export_text.py IMAGE_EXPORT --reference PLAIN_EXPORT
Optional --report NEW.json never overwrites. This supersedes page-bucket text
comparison in triage_pdf_image_export.py, not its skipped-image summary.
Offsets count Unicode alphanumeric code points in paragraph-only document order,
case sensitively, without Unicode normalization. They are not source byte offsets.
Alignment uses bounded, greedy exact anchors; it is not a minimal edit script or
fidelity verdict. Unresolved tails can include unchanged content. Page metadata
means paragraph START page only. No text or raw exception messages are emitted;
hashes, counts, positions and page metadata remain potentially identifying.
Trusted local exports only; read/character limits are not peak-memory guarantees.
"""
import argparse
from bisect import bisect_right, bisect_left
import hashlib
import json
from pathlib import Path
import re


def fingerprint(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def project(doc, max_chars):
    chunks, texts, spans = [], [], []
    size = 0
    for index, block in enumerate(doc['blocks']):
        if block['kind']['type'] != 'paragraph':
            continue
        text = block['text']
        if not isinstance(text, str):
            raise ValueError('text type')
        size += len(text)
        if size > max_chars:
            raise ValueError('paragraph character budget')
        alpha = ''.join(c for c in text if c.isalnum())
        start = spans[-1][1] if spans else 0
        page = (block.get('source') or {}).get('page')
        page = page if type(page) is int and page > 0 else None
        if alpha:
            spans.append((start, start + len(alpha), index, page))
        chunks.append(alpha)
        texts.append(text)
    joined = '\n\n'.join(texts)
    alpha = ''.join(chunks)
    return alpha, spans, {
        'paragraphs': len(texts), 'alphanumeric_characters': len(alpha),
        'whitespace_tokens': len(joined.split()), 'alphanumeric_sha256': fingerprint(alpha),
        'normalized_sha256': fingerprint(' '.join(joined.split()))}


def locations(spans, lo, hi):
    ends = [s[1] for s in spans]
    starts = [s[0] for s in spans]
    first, last = bisect_right(ends, lo), bisect_left(starts, hi)
    selected = spans[first:last] if lo < hi else spans[max(0, first - 1):first + 1]
    return {'alphanumeric_range': [lo, hi],
            'mapping_role': 'overlapping_blocks' if lo < hi else 'insertion_context_only',
            'blocks': [{'block_index': s[2], 'paragraph_start_page': s[3]} for s in selected[:20]],
            'omitted_block_mappings': max(0, len(selected) - 20)}


def align(a, b, max_regions, work_limit):
    i = j = 0
    end_a, end_b = len(a), len(b)
    while i < end_a and j < end_b and a[i] == b[j]:
        i += 1
        j += 1
    while end_a > i and end_b > j and a[end_a - 1] == b[end_b - 1]:
        end_a -= 1
        end_b -= 1
    regions, work = [], 0
    while i < end_a or j < end_b:
        if len(regions) >= max_regions - 1:
            regions.append((i, end_a, j, end_b, 'unresolved_region_limit'))
            break
        if i == end_a or j == end_b:
            regions.append((i, end_a, j, end_b, 'one_sided'))
            break
        x, y = a[i:min(i + 1024, end_a)], b[j:min(j + 1024, end_b)]
        cost = len(x) + len(y)
        if work + cost > work_limit:
            regions.append((i, end_a, j, end_b, 'unresolved_work_limit'))
            break
        work += cost
        left, right = {}, {}
        for text, anchors in ((x, left), (y, right)):
            for offset in range(max(0, len(text) - 15)):
                key = text[offset:offset + 16]
                anchors[key] = None if key in anchors else offset
        candidates = [(u + right[key], u, right[key]) for key, u in left.items()
                      if u is not None and right.get(key) is not None]
        if not candidates:
            status = 'bounded_short_core' if max(end_a - i, end_b - j) <= 1024 else 'unresolved_no_unique_anchor'
            regions.append((i, end_a, j, end_b, status))
            break
        _, u, v = min(candidates)
        regions.append((i, i + u, j, j + v, 'greedy_anchor'))
        i += u
        j += v
        while i < end_a and j < end_b and a[i] == b[j]:
            i += 1
            j += 1
    return regions, work


def compare(image, reference, max_chars=2000000, max_regions=200, work_limit=1000000):
    if min(max_chars, max_regions, work_limit) <= 0:
        raise ValueError('positive budgets required')
    for doc in (image, reference):
        if not re.fullmatch('[0-9a-f]{64}', doc['source']['sha256']):
            raise ValueError('source hash required')
    report = {'comparison_schema': 'omniattach/pdf-export-text-comparison/0.1',
              'same_source': image['source']['sha256'] == reference['source']['sha256'],
              'source_sha256': image['source']['sha256'],
              'location_semantics': 'Zero-based half-open alphanumeric code-point offsets; original zero-based block indices; page is paragraph-start metadata, never exact text-page attribution.'}
    if not report['same_source']:
        report['comparison_status'] = 'refused_source_mismatch'
        return report
    a, sa, ma = project(image, max_chars)
    b, sb, mb = project(reference, max_chars)
    rows, work = align(a, b, max_regions, work_limit)
    report.update(comparison_status='completed', image=ma, reference=mb,
                  alphanumeric_sequence_equal=(a == b),
                  whitespace_normalized_equal=(ma['normalized_sha256'] == mb['normalized_sha256']),
                  paragraph_count_equal=(ma['paragraphs'] == mb['paragraphs']),
                  alignment_method='bounded_greedy_unique_16_character_anchors_1024_lookahead',
                  anchor_window_characters_examined=work,
                  unresolved_regions=sum(row[4].startswith('unresolved') for row in rows),
                  regions=[{'image': locations(sa, i, j), 'reference': locations(sb, k, l),
                            'localization': status} for i, j, k, l, status in rows])
    return report


def load(directory, limit):
    with (directory / 'document.json').open('rb') as handle:
        raw = handle.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('read budget')
    return json.loads(raw)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('export', type=Path)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--max-json-bytes', type=int, default=67108864)
    args = parser.parse_args(argv)
    if args.max_json_bytes <= 0:
        parser.error('positive read budget required')
    stage = 'image_document'
    try:
        image = load(args.export, args.max_json_bytes)
        stage = 'reference_document'
        reference = load(args.reference, args.max_json_bytes)
        stage = 'comparison'
        report = compare(image, reference)
        code = 0 if report['same_source'] else 1
    except (OSError, ValueError, TypeError, KeyError, AttributeError, IndexError):
        report, code = {'error': 'input_read_or_structure_failed', 'stage': stage}, 1
    rendered = json.dumps(report, indent=2, sort_keys=True) + '\n'
    if args.report:
        try:
            with args.report.open('x', encoding='utf-8') as handle:
                handle.write(rendered)
        except OSError:
            print(json.dumps({'error': 'report_create_or_write_failed', 'stage': 'report'}))
            return 1
    print(rendered, end='')
    return code


if __name__ == '__main__':
    raise SystemExit(main())
