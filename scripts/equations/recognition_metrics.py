#!/usr/bin/env python3
"""Conservative formatting metrics; never a semantic-equivalence test."""
import argparse
import copy
import json
import math
from pathlib import Path
import re
import statistics

POLICY = {
    'version': 'omniattach/formula-formatting/0.1',
    'semantic_equivalence': False,
    'outer_delimiters': [r'\[...\]', r'\(...\)'],
    'rules': ['ignore ASCII math whitespace', 'ignore explicit spacing commands',
              'tfrac/dfrac -> frac', 'remove paired left/right on () or []',
              'unbrace single-token scripts'],
    'unsupported': 'delimiter-stripped literal equality only; no rewriting',
}
COMMANDS = {'frac', 'tfrac', 'dfrac', 'sqrt', 'int', 'sum', 'lim', 'to',
            'sin', 'cos', 'tan', 'log', 'ln', 'alpha', 'beta', 'gamma',
            'theta', 'pi', 'infty', 'cdot', 'times', 'le', 'ge', 'ne'}
SPACING = {r'\!', r'\,', r'\;', r'\:', r'\ ', r'\quad', r'\qquad'}
ATOM = re.compile(r'(?:[A-Za-z0-9]|\\(?:alpha|beta|gamma|theta|pi|infty))\Z')


def strip_outer_math(value):
    """Strip at most one complete backslash-delimited outer math pair."""
    text = value.strip()
    for opening, closing in [(r'\[', r'\]'), (r'\(', r'\)')]:
        if text.startswith(opening) and text.endswith(closing):
            body = text[len(opening):-len(closing)]
            if not any(mark in body for mark in (r'\[', r'\]', r'\(', r'\)')):
                return body.strip()
    return text


def formatting_tokens(value):
    """Tokenize only a restricted math subset. Unknown constructs fail closed."""
    text = strip_outer_math(value)
    tokens = []
    i = 0
    while i < len(text):
        char = text[i]
        if char in ' \t\r\n':
            i += 1
            continue
        if char == '\\':
            match = re.match(r'\\(?:[A-Za-z]+|.)', text[i:])
            if not match:
                return None
            token = match.group()
            i += len(token)
            if token in SPACING:
                continue
            if token in (r'\left', r'\right'):
                while i < len(text) and text[i] in ' \t\r\n':
                    i += 1
                if i == len(text) or text[i] not in ('([' if token == r'\left' else ')]'):
                    return None
                tokens.append(token + text[i])
                i += 1
                continue
            if token[1:] not in COMMANDS:
                return None
            tokens.append(r'\frac' if token in (r'\tfrac', r'\dfrac') else token)
        elif char in 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+-=*/(),.[]<>|^_{}':
            tokens.append(char)
            i += 1
        else:
            return None
    stack = []
    left_stack = []
    for token in tokens:
        if token == '{':
            stack.append(token)
        elif token == '}':
            if not stack:
                return None
            stack.pop()
        elif token.startswith(r'\left'):
            left_stack.append((token[-1], len(stack)))
        elif token.startswith(r'\right'):
            if not left_stack:
                return None
            opening, depth = left_stack.pop()
            if (opening, token[-1]) not in [('(', ')'), ('[', ']')] or depth != len(stack):
                return None
    if stack or left_stack:
        return None
    tokens = [t[-1] if t.startswith((r'\left', r'\right')) else t for t in tokens]
    result = []
    i = 0
    while i < len(tokens):
        if (tokens[i] in ('^', '_') and i + 3 < len(tokens)
                and tokens[i+1] == '{' and ATOM.fullmatch(tokens[i+2])
                and tokens[i+3] == '}'):
            result.extend([tokens[i], tokens[i+2]])
            i += 4
        else:
            result.append(tokens[i])
            i += 1
    return tuple(result) if result else None


def formatting_equal(a, b):
    left, right = strip_outer_math(a), strip_outer_math(b)
    if left and left == right:
        return True
    tokens = formatting_tokens(a)
    return tokens is not None and tokens == formatting_tokens(b)


def timing_summary(values):
    values = sorted(values)
    if not values:
        return dict(count=0, min=None, median=None, p95=None, max=None)
    return dict(count=len(values), min=values[0], median=statistics.median(values),
                p95=values[math.ceil(.95*len(values))-1], max=values[-1])


def add_metrics(rows):
    """Enrich rows without changing predictions or legacy exact metrics."""
    groups = {}
    for row in rows:
        latex = row['prediction'].get('latex') if row['usable'] else None
        expected = row['expected']
        et = formatting_tokens(expected)
        pt = formatting_tokens(latex) if latex is not None else None
        outer = latex is not None and bool(strip_outer_math(latex)) and strip_outer_math(latex) == strip_outer_math(expected)
        row['outer_math_delimiter_exact'] = bool(outer)
        row['formatting_normalized_exact'] = bool(outer or (row['usable'] and et is not None and et == pt))
        row['formatting_normalization'] = dict(expected_tokens=et, prediction_tokens=pt)
        groups.setdefault(row['formula_id'], []).append(row)
    names = ['exact', 'trimmed_exact', 'outer_math_delimiter_exact', 'formatting_normalized_exact']
    counts = {name: sum(bool(r[name]) for r in rows) for name in names}
    formula_results = []
    for formula, repeated in groups.items():
        usable = sum(r['usable'] for r in repeated)
        predictions = {r['prediction']['latex'] for r in repeated if r['usable']}
        normalized = {formatting_tokens(r['prediction']['latex']) for r in repeated if r['usable']}
        formula_results.append(dict(
            formula_id=formula, cases=len(repeated), usable=usable,
            all_repeats_match={name: all(r[name] for r in repeated) for name in names},
            distinct_usable_raw_predictions=len(predictions),
            raw_repeat_consistent=(usable == len(repeated) and len(predictions) == 1) if len(repeated) > 1 else None,
            normalized_repeat_consistent=(usable == len(repeated) and len(normalized) == 1 and None not in normalized) if len(repeated) > 1 else None))
    unique_counts = {name: sum(f['all_repeats_match'][name] for f in formula_results) for name in names}
    times = [r['prediction']['elapsed_ms'] for r in rows if r['usable'] and r['prediction'].get('elapsed_ms') is not None]
    return dict(
        repeated_cases=dict(cases=len(rows), counts=counts,
                            rates={k: v/len(rows) if rows else None for k,v in counts.items()},
                            formatting_supported_pairs=sum(r['formatting_normalization']['expected_tokens'] is not None and r['formatting_normalization']['prediction_tokens'] is not None for r in rows)),
        unique_formulas=dict(count=len(groups), all_repeats_match_counts=unique_counts,
                             all_repeats_match_rates={k: v/len(groups) if groups else None for k,v in unique_counts.items()}),
        repeat_consistency=dict(eligible_formulas=sum(f['cases'] > 1 for f in formula_results),
                                raw_consistent_formulas=sum(f['raw_repeat_consistent'] is True for f in formula_results),
                                normalized_consistent_formulas=sum(f['normalized_repeat_consistent'] is True for f in formula_results)),
        formula_results=formula_results,
        reported_inference_ms=timing_summary(times),
        timing_scope='worker-reported per-image samples; same-process repeats, no warmup exclusion; p95 nearest-rank; load and batch wall time remain separate')


def rescore_report(report):
    """Offline derived report: do not imply a fresh model execution."""
    result = copy.deepcopy(report)
    if result.get('schema_version') != 'omniattach/formula-evaluation/0.1':
        raise ValueError('unsupported evaluation schema')
    result['scoring_policy'] = copy.deepcopy(POLICY)
    result['rescoring'] = {'mode': 'offline', 'new_inference': False,
                           'source_metrics_preserved': True}
    for backend in result['backends']:
        if backend['status'] == 'completed':
            backend['formatting_metrics'] = add_metrics(backend['rows'])
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Offline rescore; original report and raw metrics stay unchanged.')
    parser.add_argument('input', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    report = rescore_report(json.loads(args.input.read_text()))
    import hashlib
    report['rescoring']['source_report_sha256'] = hashlib.sha256(args.input.read_bytes()).hexdigest()
    report['rescoring']['scoring_script_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    with args.output.open('x') as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
        stream.write('\n')
