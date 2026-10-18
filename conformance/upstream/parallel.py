"""Bounded process workers for independently reset native scenarios."""
import copy
import hashlib
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def merge(reports, cases, catalog_size, shards):
    expected = {case['id'] for case in cases}
    rows = [row for report in reports for row in report['results']]
    if len(rows) != len(expected) or {row['id'] for row in rows} != expected:
        raise ValueError('Worker results must cover every selected case exactly once')
    fingerprints = {case['id']: hashlib.sha256(json.dumps(case, sort_keys=True).encode()).hexdigest() for case in cases}
    if any(row['case_sha256'] != fingerprints[row['id']] for row in rows):
        raise ValueError('Catalog changed while workers were running')
    if len({report['build']['binary_sha256'] for report in reports}) != 1:
        raise ValueError('Workers executed different binaries; refusing to combine results')
    first = reports[0]
    if any((r['engine'], r['suite'], r['source']) != (first['engine'], first['suite'], first['source']) for r in reports):
        raise ValueError('Worker suite or upstream source mismatch')
    order = {case['id']: index for index, case in enumerate(cases)}
    rows.sort(key=lambda row: order[row['id']])
    result = copy.deepcopy({key: value for key, value in first.items() if key != 'results'})
    result['results'] = rows
    result['started_at'] = min(r['started_at'] for r in reports)
    result['finished_at'] = max(r['finished_at'] for r in reports)
    result['coverage'] = {'catalog_cases': catalog_size, 'recorded_cases': len(rows), 'filtered': len(rows) != catalog_size}
    result.setdefault('execution_profile', {}).update(parallel_workers=len(reports), scenario_isolation='Each worker owns its engine; every scenario resets its fixture')
    result['build']['workers'] = shards
    return result


def run(args, cases, catalog, output):
    from query_cost import summarize
    directory = output.parent / (output.stem + '.workers')
    directory.mkdir(parents=True, exist_ok=True)
    jobs = min(args.jobs, len(cases))
    if not jobs:
        raise ValueError('No scenarios selected')

    def worker(index):
        selected = directory / f'{index}.cases.json'
        selected.write_text(json.dumps([case['id'] for case in cases[index::jobs]]))
        destination = directory / f'{index}.json'
        command = [sys.executable, str(Path(__file__).with_name('run.py')),
                   '--engine', args.engine, '--suite', args.suite, '--jobs', '1',
                   '--case-file', str(selected), '--output', str(destination),
                   '--cost-work-threshold', str(args.cost_work_threshold)]
        with (directory / f'{index}.log').open('w') as log:
            subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
        data = destination.read_bytes()
        report = json.loads(data)
        print(f'{args.suite}: worker {index + 1}/{jobs} finished ({len(report["results"])} cases)', flush=True)
        return report, {'file': str(destination), 'sha256': hashlib.sha256(data).hexdigest(), 'captured_at': report['build']['captured_at']}

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        completed = list(pool.map(worker, range(jobs)))
    reports, shards = zip(*completed)
    result = merge(reports, cases, sum(case['suite'] == args.suite for case in catalog['cases']), list(shards))
    baseline = json.loads(args.cost_baseline.read_text()) if args.cost_baseline else None
    result['query_cost_summary'] = summarize(result['results'], args.cost_work_threshold, baseline)
    with output.with_suffix('.jsonl').open('w') as journal:
        for row in result['results']:
            journal.write(json.dumps(row) + '\n')
    temporary = output.with_suffix('.json.tmp')
    with temporary.open('w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    temporary.replace(output)
    from collections import Counter
    print(args.engine, args.suite, dict(Counter(row['status'] for row in result['results'])), flush=True)
