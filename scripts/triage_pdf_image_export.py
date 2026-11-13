#!/usr/bin/env python3
"""Content-omitting PDF export triage; Python 3.10+, stdlib only.

Usage: python3 scripts/triage_pdf_image_export.py IMAGE_EXPORT --reference PLAIN_EXPORT
Add --report NEW.json for exclusive report creation. This is not an integrity
checker: run audit_pdf_image_export.py separately. Only paragraph blocks are
compared. Page numbers are source metadata, not inferred visual positions.
Reports omit text, filenames, block IDs, diagnostic messages and section titles;
hashes/counts/page positions still reveal metadata. Trusted local exports only.
"""
import argparse
from collections import Counter, defaultdict
from difflib import SequenceMatcher
import hashlib
import json
from pathlib import Path
import re


KNOWN_REASONS = {
    'image masks, custom Decode or DecodeParms/predictors are unsupported': 'mask_decode_or_predictor',
    'image has no indirect XObject ID': 'no_indirect_id',
    'image XObject did not resolve to a stream': 'not_stream',
    'indirect or invalid image filter is unsupported': 'invalid_filter',
    'image filter chains or indirect filters are unsupported': 'filter_chain_or_indirect',
    'unsupported image filter; expected DCTDecode, JPXDecode, FlateDecode or no filter': 'unsupported_filter',
    'raw raster requires direct BitsPerComponent 8': 'unsupported_bit_depth',
    'raw raster requires direct DeviceGray or DeviceRGB; indexed/ICC/CMYK/indirect color spaces are unsupported': 'unsupported_color_space',
    'raw raster dimensions must be positive direct integers within PNG bounds': 'invalid_dimensions',
    'raw raster byte length disagrees with dimensions/channels': 'raw_length',
    'decoded Flate raster byte length disagrees with dimensions/channels': 'flate_length',
    'Flate raster could not be decoded within the exact expected byte bound': 'flate_decode_bound',
}


def digest(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def page_of(item):
    page = (item.get('source') or {}).get('page')
    return page if type(page) is int and page > 0 else None


def paragraphs(document):
    groups = defaultdict(list)
    for index, block in enumerate(document['blocks']):
        if block['kind']['type'] == 'paragraph':
            text = block['text']
            if not isinstance(text, str):
                raise ValueError('paragraph text type')
            groups[page_of(block)].append((index, text))
    return groups


def summary(rows):
    text = '\n\n'.join(text for _, text in rows)
    alpha = ''.join(c for c in text if c.isalnum())
    return {'paragraphs': len(rows), 'whitespace_tokens': len(text.split()),
            'alphanumeric_characters': len(alpha), 'alphanumeric_sha256': digest(alpha),
            'normalized_sha256': digest(' '.join(text.split()))}


def skipped_summary(document):
    groups = defaultdict(Counter)
    for item in document['diagnostics']:
        if item['code'] != 'PDF_IMAGE_SKIPPED':
            continue
        message = item['message']
        if not isinstance(message, str):
            raise ValueError('diagnostic message type')
        reason = KNOWN_REASONS.get(message)
        if reason is None:
            reason = 'unknown_sha256:' + digest(message)
        groups[reason][page_of(item)] += 1
    return [{'reason': reason, 'diagnostic_entries': sum(pages.values()),
             'pages': [{'page': page, 'entries': count} for page, count in
                       sorted(pages.items(), key=lambda pair: pair[0] or 0)]}
            for reason, pages in sorted(groups.items())]


def compare(image, reference, max_paragraphs=500, max_regions=200):
    if max_paragraphs <= 0 or max_regions <= 0:
        raise ValueError('positive comparison limits required')
    for doc in (image, reference):
        if not re.fullmatch('[0-9a-f]{64}', doc['source']['sha256']):
            raise ValueError('source hash required')
    same = image['source']['sha256'] == reference['source']['sha256']
    report = {'triage_schema': 'omniattach/pdf-image-export-triage/0.1',
              'same_source': same, 'source_sha256': image['source']['sha256'],
              'skipped_image_groups': skipped_summary(image),
              'note': 'No integrity or fidelity verdict. Paragraph alignment is heuristic; page metadata may be absent. Ranges contain original zero-based block indices, not text offsets.'}
    if not same:
        report['comparison_status'] = 'refused_source_mismatch'
        return report
    left, right = paragraphs(image), paragraphs(reference)
    pages = []
    remaining = max_regions
    for page in sorted(left.keys() | right.keys(), key=lambda p: p or 0):
        a, b = left.get(page, []), right.get(page, [])
        sa, sb = summary(a), summary(b)
        entry = {'page': page, 'image': sa, 'reference': sb,
                 'alphanumeric_sequence_equal': sa['alphanumeric_sha256'] == sb['alphanumeric_sha256'],
                 'whitespace_normalized_equal': sa['normalized_sha256'] == sb['normalized_sha256']}
        if max(len(a), len(b)) > max_paragraphs:
            entry['alignment_status'] = 'omitted_paragraph_limit'
        else:
            regions = []
            omitted = 0
            matcher = SequenceMatcher(None, [digest(' '.join(t.split())) for _, t in a],
                                      [digest(' '.join(t.split())) for _, t in b], autojunk=False)
            for tag, i, j, k, l in matcher.get_opcodes():
                if tag == 'equal':
                    continue
                if remaining == 0:
                    omitted += 1
                    continue
                regions.append({'operation': tag,
                                'image_block_indices': [n for n, _ in a[i:j]],
                                'reference_block_indices': [n for n, _ in b[k:l]]})
                remaining -= 1
            entry.update(alignment_status='bounded_paragraph_alignment', regions=regions, omitted_regions=omitted)
        pages.append(entry)
    report.update(comparison_status='completed', pages=pages,
                  differing_alphanumeric_pages=sum(not p['alphanumeric_sequence_equal'] for p in pages),
                  differing_normalized_pages=sum(not p['whitespace_normalized_equal'] for p in pages))
    return report


def load_document(directory, limit):
    if limit <= 0:
        raise ValueError('positive read limit required')
    with (directory / 'document.json').open('rb') as handle:
        raw = handle.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('read limit')
    doc = json.loads(raw)
    if not isinstance(doc, dict) or not isinstance(doc.get('blocks'), list) or not isinstance(doc.get('diagnostics'), list):
        raise ValueError('document shape')
    return doc


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('export', type=Path)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--max-json-bytes', type=int, default=67108864)
    args = parser.parse_args(argv)
    if args.max_json_bytes <= 0:
        parser.error('read limit must be positive')
    stage = 'image_document'
    try:
        image = load_document(args.export, args.max_json_bytes)
        stage = 'reference_document'
        reference = load_document(args.reference, args.max_json_bytes)
        stage = 'comparison'
        result = compare(image, reference)
        status = 0 if result['same_source'] else 1
    except (OSError, ValueError, TypeError, KeyError, AttributeError, IndexError):
        result = {'triage_schema': 'omniattach/pdf-image-export-triage/0.1',
                  'error': 'input_read_or_structure_failed', 'stage': stage}
        status = 1
    rendered = json.dumps(result, indent=2, sort_keys=True) + '\n'
    if args.report:
        try:
            with args.report.open('x', encoding='utf-8') as handle:
                handle.write(rendered)
        except OSError:
            print(json.dumps({'error': 'report_create_or_write_failed', 'stage': 'report'}))
            return 1
    print(rendered, end='')
    return status


if __name__ == '__main__':
    raise SystemExit(main())
