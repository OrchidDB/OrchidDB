#!/usr/bin/env python3
"""Run the pinned ProcessComputerSuite in bounded, isolated JVMs.

Build the existing provider harness first, then supply a frozen production runtime
jar, its dependency classpath, the native store, and the pinned upstream checkout.
An optional --exclude-algorithms selection leaves the 31 algorithm tests to a
parallel runner. Original suite occurrences (including duplicate ProfileTest
entries), assertions, source hashes, assumptions and ignores are preserved.
"""
import argparse
import collections
import concurrent.futures
import datetime
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import zipfile
import time

ROOT = Path(__file__).resolve().parent
REVISION = 'fa698ba2aba8967dcd17eb61cb13648b934fab5b'
ALGORITHMS = {'ConnectedComponentTest$Traversals', 'PageRankTest$Traversals',
              'PeerPressureTest$Traversals', 'ShortestPathTest$Traversals'}


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def hashes(directory):
    return {str(p.relative_to(directory)): digest(p)
            for p in sorted(directory.rglob('*')) if p.is_file()}


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--store', type=Path, required=True)
    parser.add_argument('--store-source-commit', required=True)
    parser.add_argument('--runtime-jar', type=Path, required=True)
    parser.add_argument('--runtime-classpath', type=Path, required=True)
    parser.add_argument('--runtime-manifest', type=Path, required=True)
    parser.add_argument('--upstream', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New report directory')
    parser.add_argument('--inventory', type=Path, help='Reuse a pinned harness inventory')
    parser.add_argument('--inventory-only', action='store_true', help='Validate selection without executing tests')
    parser.add_argument('--harness-classes', type=Path, default=ROOT / 'target/classes')
    parser.add_argument('--harness-classpath', type=Path, default=ROOT / 'target/classpath.txt')
    parser.add_argument('--java', default=os.environ.get('CONFORMANCE_JAVA', 'java'))
    parser.add_argument('--jstack', help='Optional jstack executable for timeout diagnostics')
    parser.add_argument('--timeout', type=int, default=180, help='Deadline per class JVM, seconds')
    parser.add_argument('--workers', type=int, default=2, help='Concurrent independent JVMs')
    parser.add_argument('--exclude-algorithms', action='store_true')
    args = parser.parse_args()
    if args.timeout < 1 or args.workers < 1:
        parser.error('--timeout and --workers must be positive')
    for path in (args.store, args.runtime_jar, args.runtime_classpath,
                 args.runtime_manifest, args.harness_classpath):
        if not path.is_file():
            parser.error(f'Missing input file: {path}')
    upstream = args.upstream.resolve()
    revision = subprocess.check_output(['git', '-C', str(upstream), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != REVISION:
        parser.error(f'Expected pinned upstream {REVISION}; found {revision}')
    manifest = json.loads(args.runtime_manifest.read_text())
    expected_jar = manifest.get('jar_sha256') or manifest.get('files', {}).get('jvm/target/orchiddb-jvm-0.1.0.jar')
    if expected_jar != digest(args.runtime_jar):
        parser.error('Frozen runtime jar does not match its source manifest')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    production = out / 'production-classes'
    with zipfile.ZipFile(args.runtime_jar) as jar:
        jar.extractall(production)
    shutil.copytree(args.harness_classes, out / 'harness-classes')
    shutil.copy(args.runtime_manifest, out / 'runtime-source-manifest.json')
    shutil.copy(__file__, out / 'orchestrator.py')
    # Production classes precede all installed/transitive dependencies.
    cp = os.pathsep.join([str(production), str(out / 'harness-classes'),
                          args.harness_classpath.read_text().strip(),
                          args.runtime_classpath.read_text().strip()])
    java = shutil.which(args.java) or args.java
    jstack = args.jstack or str(Path(java).with_name('jstack'))
    flags = ['-Dis.testing=true', '-DassertNonDeterministic=true',
             '-Dorchiddb.test.computer=true',
             '--add-opens=java.base/java.util=ALL-UNNAMED',
             '--add-opens=java.base/java.lang=ALL-UNNAMED',
             '--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED']
    base = [java, *flags, '-cp', cp, 'io.orchiddb.conformance.ProviderSuite']
    environment = dict(os.environ, ORCHIDDB_JVM_STORE=str(args.store.resolve()))
    if args.inventory:
        inventory = json.loads(args.inventory.read_text())
    else:
        inventory_path = out / 'inventory.json'
        subprocess.run([*base, 'ProcessComputerSuite', str(upstream), str(inventory_path), 'inventory'],
                       env=environment, check=True, timeout=args.timeout)
        inventory = json.loads(inventory_path.read_text())
    if inventory['upstream_revision'] != REVISION or not inventory['inventory_only']:
        raise ValueError('Expected an original pinned ProcessComputerSuite inventory')
    write_json(out / 'inventory.json', inventory)
    groups, occurrences = [], collections.Counter()
    for index, (name, rows) in enumerate(itertools.groupby(
            inventory['cases'], lambda row: row['id'].split('#')[0])):
        rows = list(rows)
        occurrences[name] += 1
        if not args.exclude_algorithms or name.rsplit('.', 1)[1] not in ALGORITHMS:
            groups.append((index, name, occurrences[name], rows))
    report = {
        'started_at': now(), 'profile': 'jvm-graphcomputer',
        'selection': 'ProcessComputerSuite', 'upstream_revision': REVISION,
        'suite_declared_occurrences': len(inventory['cases']),
        'suite_unique_original_ids': len({row['id'] for row in inventory['cases']}),
        'selected_occurrences': sum(len(group[3]) for group in groups),
        'excluded_classes_owned_by_parallel_original31_run': sorted(ALGORITHMS) if args.exclude_algorithms else [],
        'upstream_assertions_modified': False, 'native_rust_traversal_evidence': False,
        'java_system_properties': {'is.testing': 'true', 'assertNonDeterministic': 'true',
                                   'orchiddb.test.computer': 'true', 'build.dir': 'isolated per class'},
        'store_path': str(args.store.resolve()), 'store_sha256': digest(args.store),
        'store_source_commit': args.store_source_commit,
        'runtime_jar_sha256': digest(args.runtime_jar),
        'runtime_source_commit': manifest['source_revision'],
        'runtime_source_manifest': manifest,
        'provider_source_sha256': manifest['jvm_sources'],
        'production_class_sha256': hashes(production),
        'harness_class_sha256': hashes(out / 'harness-classes'),
        'harness_source_sha256': hashes(ROOT / 'src'),
        'orchestrator_sha256': digest(__file__), 'class_timeout_seconds': args.timeout,
        'effective_classpath': cp, 'java_flags': flags,
        'java_version': subprocess.run([java, '-version'], capture_output=True, text=True).stderr,
        'cases': [], 'class_processes': [], 'run_complete': False,
        'inventory_only': args.inventory_only,
    }
    write_json(out / 'provenance.json', report)
    if args.inventory_only:
        for index, name, occurrence, rows in groups:
            report['cases'].extend(dict(row, suite_class_index=index,
                                        suite_class_occurrence=occurrence) for row in rows)
        report['counts'] = {'not-run': len(report['cases'])}
        report['run_complete'] = True
        report['finished_at'] = now()
        write_json(out / 'aggregate.json', report)
        print(json.dumps(report['counts']), flush=True)
        return 0

    def run_class(item):
        index, name, occurrence, expected = item
        filename = f'{index:02d}-{name.rsplit(".", 1)[1].replace("$", "_")}-{occurrence}'
        result_path = out / (filename + '.json')
        command = [java, '-Dbuild.dir=' + str(out / (filename + '-test-data')),
                   *flags, '-cp', cp, 'io.orchiddb.conformance.ProviderSuite',
                   name, str(upstream), str(result_path)]
        start = time.monotonic()
        with (out / (filename + '.log')).open('w') as log:
            process = subprocess.Popen(command, stdout=log, stderr=log,
                                       env=environment, start_new_session=True)
            try:
                code = process.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                try:
                    with (out / (filename + '.stack')).open('w') as stack:
                        subprocess.run([jstack, str(process.pid)], stdout=stack,
                                       stderr=subprocess.DEVNULL, timeout=10)
                except (subprocess.TimeoutExpired, OSError):
                    pass
                finally:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    code = 124
        try:
            data = json.loads(result_path.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            data = {'cases': [], 'run_complete': False}
        seen = {row['id'] for row in data['cases']}
        for row in expected:
            if row['id'] not in seen:
                data['cases'].append(dict(row, status='not-run', reason=(
                    'Class process timed out' if code == 124 else 'Class runner did not report this case')))
        for row in data['cases']:
            if row['status'] == 'running':
                row['status'] = 'timeout' if code == 124 else 'interrupted'
                row['reason'] = 'Class process ended before this test completed'
            row.update(suite_class_index=index, suite_class_occurrence=occurrence,
                       class_report=result_path.name)
        info = {'suite_class_index': index, 'class': name, 'suite_class_occurrence': occurrence,
                'exit_code': code, 'duration_seconds': round(time.monotonic() - start, 3),
                'complete': data.get('run_complete', False) and code in (0, 1),
                'counts': dict(collections.Counter(row['status'] for row in data['cases']))}
        print(json.dumps(info), flush=True)
        return info, data['cases']

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = [pool.submit(run_class, group) for group in groups]
        for future in concurrent.futures.as_completed(futures):
            info, rows = future.result()
            report['class_processes'].append(info)
            report['cases'].extend(rows)
            report['cases'].sort(key=lambda row: (row['suite_class_index'], row['id']))
            report['class_processes'].sort(key=lambda row: row['suite_class_index'])
            report['counts'] = dict(collections.Counter(row['status'] for row in report['cases']))
            write_json(out / 'aggregate.json', report)
    report['finished_at'] = now()
    report['run_complete'] = all(row['complete'] for row in report['class_processes'])
    write_json(out / 'aggregate.json', report)
    print('FINAL ' + json.dumps(report['counts']), flush=True)
    return 0 if report['run_complete'] and not report['counts'].get('fail') else 1


if __name__ == '__main__':
    raise SystemExit(main())
