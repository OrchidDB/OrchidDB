#!/usr/bin/env python3
"""Run pinned upstream assertions against the loaded DuckDB extension only."""
import argparse
from collections import Counter
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'conformance/upstream'))


def identity(path):
    path = Path(path).resolve()
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--suite', choices=['opencypher', 'tinkerpop', 'rdf'], required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--limit', type=int)
    p.add_argument('--filter', default='')
    p.add_argument('--case', action='append', default=[], help='Exact pinned case ID; repeatable')
    args = p.parse_args()
    os.chdir(REPO)
    os.environ['CONFORMANCE_ORCHIDDB_BINARY'] = str(Path(__file__).with_name('adapter.py').resolve())
    os.environ['PATH'] = str(Path(sys.executable).parent) + os.pathsep + os.environ['PATH']
    os.environ['CONFORMANCE_PYTHON'] = sys.executable
    # Required if the upstream Java assertion adapter starts its fixture bridge.
    os.environ['ORCHIDDB_SQL_ENGINE_JSON'] = '{"dialect":"duckdb"}'
    if args.suite == 'tinkerpop':
        kernel_jar = REPO / 'jvm/target/orchiddb-jvm-0.1.0.jar'
        dependencies = (REPO / 'jvm/target/classpath.txt').read_text().strip()
        os.environ.setdefault('ORCHIDDB_JVM_CLASSPATH', str(kernel_jar) + os.pathsep + dependencies)
        os.environ.setdefault('ORCHIDDB_JAVA', os.environ.get('CONFORMANCE_JAVA', 'java'))
    from adapter import Extension, EXTENSION
    import duckdb
    check = Extension()
    try:
        assert check.send({'op': 'cypher', 'query': 'RETURN 1 AS x'})['native_rows'] == [[{'type':'int','value':1}]]
        assert check.send({'op': 'sparql-syntax', 'query': 'SELECT * WHERE {}'}) == {'parsed': True}
        provider = check.send({'op': 'provider-info'})
    finally: check.db.close()
    catalog = json.loads((REPO / 'conformance/upstream/catalog.json').read_text())
    cases = [c for c in catalog['cases'] if c['suite'] == args.suite and args.filter in c['id'] and (not args.case or c['id'] in args.case)]
    if args.case and set(args.case) - {c['id'] for c in cases}:
        p.error('An exact case ID is missing from the selected pinned suite/filter')
    if args.limit: cases = cases[:args.limit]
    build = {'extension': identity(EXTENSION), 'duckdb': duckdb.__version__,
             'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
             'working_tree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True)),
             'adapter_sources': [identity(__file__), identity(Path(__file__).with_name('adapter.py')), identity(Path(__file__).with_name('provider.py'))]}
    if provider['extension_artifact_sha256'] != build['extension']['sha256']:
        raise RuntimeError('Extension artifact changed during provider initialization')
    build['provider'] = provider
    provider_cases = {}
    upstream = REPO / 'conformance/upstream'
    build['assertion_sources'] = [identity(upstream / f) for f in ('run.py', 'cypher.py', 'sparql.py', 'sparql_updates.py')]
    if args.suite == 'opencypher':
        from cypher import Cypher
        adapter = Cypher('orchiddb')
    elif args.suite == 'rdf':
        from sparql import Sparql
        adapter = Sparql('orchiddb')
    else:
        from run import Gremlin
        from provider import original_provider_cases, verify_provider_result
        source = Path(os.environ.get('CONFORMANCE_TINKERPOP_SOURCE',
                      REPO / 'conformance/upstream/cache/tinkerpop')).resolve()
        os.environ['CONFORMANCE_TINKERPOP_SOURCE'] = str(source)
        provider_cases = original_provider_cases(REPO, catalog, source)
        build['provider_assertions'] = identity(REPO / 'conformance/adapters/jvm-provider-tests/placeholders.json')
        adapter = Gremlin('orchiddb')
        kernel_artifacts = []
        for entry in os.environ['ORCHIDDB_JVM_CLASSPATH'].split(os.pathsep):
            path = Path(entry)
            kernel_artifacts.extend(identity(f) for f in sorted(path.rglob('*.class'))) if path.is_dir() else kernel_artifacts.append(identity(path))
        build['jvm_kernel_classpath'] = kernel_artifacts
        build['jvm_kernel_scope'] = 'Existing compiled GraphJvm operators; DuckDB owns scans, scheduling and transactions'
        build['java'] = subprocess.check_output([os.environ.get('CONFORMANCE_JAVA', 'java'), '-version'], stderr=subprocess.STDOUT, text=True).strip()
        artifacts = []
        for entry in adapter.classpath.split(os.pathsep):
            path = Path(entry)
            artifacts.extend(identity(f) for f in sorted(path.rglob('*.class'))) if path.is_dir() else artifacts.append(identity(path))
        build['assertion_classpath'] = artifacts
    args.output.parent.mkdir(parents=True, exist_ok=True)
    results = []
    start = datetime.datetime.now(datetime.timezone.utc).isoformat()
    try:
        with args.output.with_suffix('.jsonl').open('w') as journal:
            for i, case in enumerate(cases):
                before = time.monotonic()
                try:
                    result = adapter.run(case)
                    if case['id'] in provider_cases:
                        result = verify_provider_result(result, provider_cases[case['id']], build['extension']['sha256'])
                except TimeoutError as error: result = {'status': 'timeout', 'reason': str(error)}
                except Exception as error: result = {'status': 'adapter-error', 'reason': str(error)}
                record = {'id': case['id'], 'case_sha256': hashlib.sha256(json.dumps(case, sort_keys=True).encode()).hexdigest(),
                          'elapsed_ms': round((time.monotonic() - before) * 1000, 3), **result}
                results.append(record)
                journal.write(json.dumps(record) + '\n'); journal.flush()
                if (i + 1) % 50 == 0: print(args.suite, i + 1, '/', len(cases), flush=True)
    finally: adapter.close()
    counts = dict(Counter(r['status'] for r in results))
    report = dict(schema_version=3, engine='duckdb-extension', suite=args.suite, source=catalog['sources'][args.suite],
                  started_at=start, finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(), build=build,
                  execution_profile={'executor': 'Host DuckDB only', 'datafusion_execution': False, 'graphengine_execution': False,
                                     'assertions': 'Existing pinned upstream assertions, unchanged'},
                  coverage={'catalog_cases': sum(c['suite'] == args.suite for c in catalog['cases']), 'recorded_cases': len(results),
                            'filtered': bool(args.limit or args.filter or args.case)}, counts=counts, results=results)
    if args.suite == 'tinkerpop':
        instances = {r['engine_instance'] for r in results if r.get('engine_instance')}
        report['execution_profile']['engine_instances'] = sorted(instances)
        report['execution_profile']['single_instance_verified'] = len(instances) == 1 and all(
            r.get('engine_instance') in instances for r in results if r['status'] != 'unsupported')
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(args.suite, counts, flush=True)
    return 0 if len(results) == report['coverage']['catalog_cases'] and counts.get('pass') == len(results) else 1


if __name__ == '__main__': sys.exit(main())
