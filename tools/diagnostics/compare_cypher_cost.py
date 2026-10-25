#!/usr/bin/env python3
"""Compare executed Cypher costs, keeping semantic coverage and time separate.

Usage: python tools/diagnostics/compare_cypher_cost.py BEFORE AFTER --output REPORT
Only identical case/query signatures and metric versions are comparable. Volatile
queries can change actual row work between runs; deltas are observations, not proofs.
"""
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'conformance' / 'upstream'))
from query_cost import signature


def measurements(case):
    cost = case.get('query_cost', {})
    entries = [e['cost'] for e in cost.get('queries', [])
               if e.get('phase') in ('query', 'control') and isinstance(e.get('cost'), dict)
               and e['cost'].get('work_units') is not None]
    if not entries or any(any(e.get(k, 0) for k in (
            'unprofiled_sql_executions', 'unprofiled_source_queries',
            'unprofiled_write_queries')) for e in entries):
        return None
    result = dict.fromkeys(('total_work', 'orchid_work', 'engine_work', 'sql_calls',
                            'source_calls', 'write_calls', 'cpu_nanos', 'elapsed_micros',
                            'operators', 'sql_bytes', 'peak_memory_bytes', 'max_plan_depth'), 0)
    for entry in entries:
        result['total_work'] += entry['work_units']
        result['orchid_work'] += entry.get('orchid_work_units', 0)
        result['engine_work'] += entry.get('engine_work_units', 0)
        m = entry['measurements']
        result['sql_calls'] += m['sql_executions']
        result['source_calls'] += max(m['native_source_queries'], m.get('source_engine', {}).get('statements', 0))
        result['write_calls'] += m.get('write_engine', {}).get('statements', 0)
        result['elapsed_micros'] += entry.get('request_elapsed_micros', 0)
        for key in ('engine', 'source_engine', 'write_engine'):
            engine = m.get(key, {})
            for metric in ('cpu_nanos', 'operators', 'sql_bytes'):
                result[metric] += engine.get(metric, 0)
            for metric in ('peak_memory_bytes', 'max_plan_depth'):
                result[metric] = max(result[metric], engine.get(metric, 0))
    return result


def compare(before, after):
    previous = {c['id']: c for c in before['results']}
    rows, skipped = [], []
    for case in after['results']:
        old = previous.get(case['id'])
        a, b = (old or {}).get('query_cost', {}), case.get('query_cost', {})
        if (old is None or old.get('case_sha256') != case.get('case_sha256')
            or a.get('metric_version') != b.get('metric_version')
            or a.get('attribution_version') != b.get('attribution_version')
            or signature(a) != signature(b)
            or old.get('status') != 'pass' or case.get('status') != 'pass'
            or not (x := measurements(old)) or not (y := measurements(case))):
            skipped.append(case['id'])
            continue
        def statements(cost):
            return [e.get('cost', {}).get('execution_plan', {}).get('sql_queries')
                    for e in cost.get('queries', []) if e.get('phase') in ('query', 'control')
                    and isinstance(e.get('cost'), dict)]
        before_sql, after_sql = statements(a), statements(b)
        rows.append({'id': case['id'], 'before': x, 'after': y,
                     'contains_rand_call': any('rand(' in e.get('query', '').lower()
                         for e in b.get('queries', []) if e.get('phase') in ('query', 'control')),
                     'same_sql': before_sql == after_sql if all(v is not None for v in before_sql + after_sql) else None,
                     'delta': {k: y[k] - x[k] for k in x},
                     'complexity_review': y['max_plan_depth'] > 64 or y['sql_bytes'] > 262144})
    sums = {}
    for side in ('before', 'after'):
        sums[side] = {k: sum(r[side][k] for r in rows) for k in rows[0][side]} if rows else {}
        # Maxima across queries must not be mistaken for additive work.
        for k in ('peak_memory_bytes', 'max_plan_depth'):
            sums[side][k] = max((r[side][k] for r in rows), default=0)
    return {'scope': 'tested query/control phases; fixtures and observations excluded',
            'time_warning': 'Profiled elapsed times include profiling overhead and concurrent test contention.',
            'complexity_policy': 'Review deep/large SQL; do not introduce boundaries merely to shorten SQL. Compare extra request/transfer cost and lost pushdown against saved engine work.',
            'before_status': {s: sum(c['status'] == s for c in before['results']) for s in {c['status'] for c in before['results']}},
            'after_status': {s: sum(c['status'] == s for c in after['results']) for s in {c['status'] for c in after['results']}},
            'compared': len(rows), 'skipped': skipped, 'totals': sums,
            'improved': sum(r['delta']['total_work'] < 0 for r in rows),
            'regressed': sum(r['delta']['total_work'] > 0 for r in rows),
            'unchanged': sum(r['delta']['total_work'] == 0 for r in rows),
            'increased_work_review': [{k: r[k] for k in ('id', 'contains_rand_call', 'same_sql')} for r in rows if r['delta']['total_work'] > 0],
            'queries': sorted(rows, key=lambda r: -r['after']['total_work'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    report = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('queries', 'skipped')}, indent=2))


if __name__ == '__main__':
    main()
