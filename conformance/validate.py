#!/usr/bin/env python3
"""Validate committed local evidence. Never invoked by GitHub Actions."""
import hashlib
import json
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent
STATUS = {'pass', 'fail', 'unsupported', 'skipped', 'not-applicable', 'adapter-error', 'timeout'}

def main():
    catalog = json.loads((ROOT / 'upstream/catalog.json').read_text())
    sources = json.loads((ROOT / 'upstream/sources.json').read_text())
    assert catalog['sources'] == sources, 'Catalog source pins changed'
    cases = {case['id']: case for case in catalog['cases']}
    assert len(cases) == len(catalog['cases']), 'Duplicate catalog IDs'
    for engine in ('crabgraph', 'sqlg', 'puppygraph', 'reference'):
        for suite in sources:
            if engine == 'reference' and suite != 'tinkerpop':
                continue
            path = ROOT / 'upstream-results' / f'{engine}-{suite}.json'
            run = json.loads(path.read_text())
            expected = {key for key, case in cases.items() if case['suite'] == suite}
            results = run['results']
            assert run['engine'] == engine and run['suite'] == suite, path
            assert run['source'] == sources[suite], f'{path}: stale source'
            assert not run['coverage']['filtered'], f'{path}: subset run'
            assert len(results) == len(expected), f'{path}: missing/duplicate cases'
            assert {r['id'] for r in results} == expected, f'{path}: case coverage'
            for result in results:
                fingerprint = hashlib.sha256(json.dumps(cases[result['id']], sort_keys=True).encode()).hexdigest()
                assert result['case_sha256'] == fingerprint, f'{path}: stale case {result["id"]}'
                assert result['status'] in STATUS, f'{path}: unknown outcome'
                assert result['elapsed_ms'] >= 0, f'{path}: negative time'
            counts = Counter(r['status'] for r in results)
            if engine == 'reference':
                assert set(counts) <= {'pass', 'skipped'} and counts['pass'] > 0, 'Reference assertion check failed'
            print(f'{engine} / {suite}: {len(results)} cases; {dict(counts)}')
    print(f'Validated {len(cases)} upstream cases and every committed run.')

if __name__ == '__main__':
    main()
