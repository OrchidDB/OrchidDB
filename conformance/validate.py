#!/usr/bin/env python3
"""Validate committed local evidence. Never invoked by GitHub Actions."""
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent
STATUS = {'pass', 'fail', 'unsupported', 'skipped', 'not-applicable', 'adapter-error', 'timeout'}

def main():
    sys.path.insert(0, str(ROOT / 'upstream'))
    from java_assertions import counterpart, result_for
    selection = json.loads((ROOT / 'adapters/jvm-provider-tests/placeholders.json').read_text())
    catalog = json.loads((ROOT / 'upstream/catalog.json').read_text())
    sources = json.loads((ROOT / 'upstream/sources.json').read_text())
    assert catalog['sources'] == sources, 'Catalog source pins changed'
    cases = {case['id']: case for case in catalog['cases']}
    assert len(cases) == len(catalog['cases']), 'Duplicate catalog IDs'
    for engine in ('crabgraph', 'sqlg', 'puppygraph', 'reference', 'crabgraph-jvm', 'crabgraph-computer'):
        for suite in sources:
            if engine in ('reference', 'crabgraph-jvm', 'crabgraph-computer') and suite != 'tinkerpop':
                continue
            path = ROOT / 'upstream-results' / f'{engine}-{suite}.json'
            run = json.loads(path.read_text())
            expected = {key for key, case in cases.items() if case['suite'] == suite}
            if engine == 'crabgraph-computer' and run['coverage']['filtered']:
                expected = {key for key in expected if '@GraphComputerOnly' in cases[key].get('tags', [])}
            results = run['results']
            assert run['engine'] == engine and run['suite'] == suite, path
            assert run['source'] == sources[suite], f'{path}: stale source'
            assert not run['coverage']['filtered'] or engine == 'crabgraph-computer', f'{path}: subset run'
            assert len(results) == len(expected), f'{path}: missing/duplicate cases'
            assert {r['id'] for r in results} == expected, f'{path}: case coverage'
            for result in results:
                fingerprint = hashlib.sha256(json.dumps(cases[result['id']], sort_keys=True).encode()).hexdigest()
                assert result['case_sha256'] == fingerprint, f'{path}: stale case {result["id"]}'
                assert result['status'] in STATUS, f'{path}: unknown outcome'
                assert result['elapsed_ms'] >= 0, f'{path}: negative time'
                if result.get('assertion_source', {}).get('kind') == 'java-counterpart':
                    assert engine in ('crabgraph', 'crabgraph-jvm') and suite == 'tinkerpop', path
                    case = cases[result['id']]
                    mapping = counterpart(case, selection)
                    assert mapping is not None, f'{path}: unmapped Java counterpart'
                    report = {**result['java_run'], 'cases': [result['java_assertion']]}
                    verified = result_for(case, mapping, report, sources[suite]['revision'])
                    for key in ('status', 'elapsed_ms', 'assertion_source', 'assertion_engine'):
                        assert result[key] == verified[key], f'{path}: inconsistent Java evidence {key}'
            counts = Counter(r['status'] for r in results)
            if engine == 'reference':
                assert set(counts) <= {'pass', 'skipped'} and counts['pass'] > 0, 'Reference assertion check failed'
            print(f'{engine} / {suite}: {len(results)} cases; {dict(counts)}')
    java_root = ROOT / 'upstream-results/java-provider'
    index = json.loads((java_root / 'index.json').read_text())
    for entry in index['entries']:
        path = (java_root / entry['file']).resolve()
        assert path.is_relative_to(java_root.resolve()), 'Java evidence path escapes its directory'
        report = json.loads(path.read_text())
        if 'cases' in report:
            counts = Counter(case['status'] for case in report['cases'])
            if entry['upstream_assertions']:
                assert report['upstream_assertions_modified'] is False, path
                assert all(case.get('source_sha256') for case in report['cases']), path
        elif 'counts' in report:
            counts = report['counts']
        elif 'jvm_tests' in report:
            test = report['jvm_tests']
            counts = {'pass': test['tests'] - test['failures'] - test['errors'] - test['skipped'],
                      'fail': test['failures'], 'error': test['errors'], 'skipped': test['skipped']}
        elif 'native_protocol_tests' in report:
            test = report['native_protocol_tests']
            counts = {'pass': test['passed'], 'fail': test['failed'], 'skipped': test['ignored']}
        else:
            counts = {'pass': report['run'] - report['failed'] - report['ignored'],
                      'fail': report['failed'], 'skipped': report['ignored']}
        assert {k: v for k, v in counts.items() if v} == entry['counts'], f'{path}: summary mismatch'
        assert entry['run_complete'] == report.get('run_complete', True), f'{path}: completion mismatch'
        print(f"Java/provider evidence: {entry['file']}; {entry['counts']}")
    print(f'Validated {len(cases)} upstream cases, execution profiles, and Java/provider evidence summaries.')

if __name__ == '__main__':
    main()
