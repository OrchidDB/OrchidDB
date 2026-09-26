"""Versioned observational cost reporting; never changes conformance outcomes."""
import hashlib
METRIC_VERSION = 1

def collect(adapter, result):
    entries = list(getattr(adapter, 'query_costs', [])) if adapter else []
    if not entries:
        entries = [{'query': t.get('query', ''), 'phase': 'query', 'step': t.get('step'),
                    'cost': t.get('query_cost')} for t in result.get('query_transports', [])]
    # Fixture construction and TCK side-effect observation are retained for
    # inspection, but must not dominate the tested query's ranking.
    selected = [e for e in entries if e.get('phase') in ('query', 'control')]
    measured = [e for e in selected if isinstance(e.get('cost'), dict)
                and e['cost'].get('metric_version') == METRIC_VERSION
                and e['cost'].get('work_units') is not None]
    elapsed = sum(e['cost'].get('request_elapsed_micros', 0) for e in selected
                  if isinstance(e.get('cost'), dict))
    return {'metric_version': METRIC_VERSION, 'queries': entries,
            'query_count': len(selected), 'measured_queries': len(measured),
            'coverage': ('unavailable' if result.get('status') in ('timeout','adapter-error','fail') else 'not_executed') if not selected else
                        'boundary_work' if len(measured) == len(selected) else
                        'partial' if measured else 'elapsed_only',
            'request_elapsed_micros': elapsed,
            'work_units': sum(e['cost']['work_units'] for e in measured) if measured else None}

def signature(cost):
    queries = [(e.get('phase'), e.get('query')) for e in cost.get('queries', [])
               if e.get('phase') in ('query', 'control')]
    return hashlib.sha256(repr(queries).encode()).hexdigest()

def summarize(results, threshold=100000, baseline=None):
    ranked = sorted(({'id': r['id'], 'work_units': c['work_units'],
                      'request_elapsed_micros': c['request_elapsed_micros'], 'coverage': c['coverage']}
                     for r in results if (c := r.get('query_cost', {})).get('work_units') is not None),
                    key=lambda r: (-r['work_units'], r['id']))
    timed = [{'id':r['id'],'work_units':c.get('work_units'),'request_elapsed_micros':c.get('request_elapsed_micros',0),'coverage':c.get('coverage')} for r in results if (c:=r.get('query_cost',{})).get('query_count',0)>0]
    previous = {r['id']: r for r in (baseline or {}).get('results', [])}
    comparisons = []
    for row in results:
        old = previous.get(row['id'], {})
        a, b = old.get('query_cost', {}), row.get('query_cost', {})
        if (a.get('metric_version') != METRIC_VERSION or b.get('metric_version') != METRIC_VERSION
            or a.get('coverage') != 'boundary_work' or b.get('coverage') != 'boundary_work'
            or a.get('work_units') is None or b.get('work_units') is None
            or old.get('case_sha256') != row.get('case_sha256') or signature(a) != signature(b)):
            continue
        comparisons.append({'id': row['id'], 'before': a['work_units'], 'after': b['work_units'],
                            'delta': b['work_units'] - a['work_units'],
                            'ratio': b['work_units'] / max(1, a['work_units'])})
    return {'metric_version': METRIC_VERSION, 'threshold': threshold,
            'cases_with_work_measurements': len(ranked),
            'expensive_queries': [r for r in ranked if r['work_units'] >= threshold],
            'highest_work': ranked[:50],
            'highest_elapsed': sorted(timed, key=lambda r: -r['request_elapsed_micros'])[:50],
            'baseline_comparisons': sorted(comparisons, key=lambda r: -r['delta'])}
