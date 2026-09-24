#!/usr/bin/env python3
"""Local-only runner for original TinkerPop Java test classes on native CrabGraph."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--selection', default=str(ROOT / 'placeholders.json'))
    parser.add_argument('--upstream', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--jvm', type=Path, default=REPO / 'jvm')
    parser.add_argument('--store', type=Path)
    parser.add_argument('--jvm-jar', type=Path, help='Frozen production provider artifact')
    parser.add_argument('--jvm-classpath', type=Path, help='Frozen dependency classpath file')
    parser.add_argument('--provider-source-commit', help='Source commit supplied with the frozen provider jar')
    parser.add_argument('--java', default=os.environ.get('CONFORMANCE_JAVA', 'java'))
    parser.add_argument('--inventory', action='store_true')
    parser.add_argument('--computer', action='store_true', help='Use GraphComputer for class/method selections')
    parser.add_argument('--timeout', type=int, default=3600)
    args = parser.parse_args()
    subprocess.run(['mvn', '-q', '-f', str(ROOT / 'pom.xml'), 'package',
                    'dependency:build-classpath', '-Dmdep.outputFile=target/classpath.txt'], check=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    harness_jar = args.output.with_suffix('.harness.jar').resolve()
    shutil.copyfile(ROOT / 'target/crabgraph-provider-tests-0.1.0.jar', harness_jar)
    cp = [str(harness_jar), (ROOT / 'target/classpath.txt').read_text().strip()]
    if not args.inventory:
        if not args.store or not args.store.is_file():
            parser.error('--store must identify the actual production crabgraph-jvm-store binary')
        cp.append(str(args.jvm_jar) if args.jvm_jar else str(args.jvm / 'target/classes'))
        for dependency_file in ([args.jvm_classpath] if args.jvm_classpath else [args.jvm / 'classpath.txt', args.jvm / 'target/classpath.txt']):
            if dependency_file.exists():
                cp.append(dependency_file.read_text().strip())
    command = [args.java, '-Dis.testing=true', '-Dcrabgraph.test.computer=' + str(args.computer).lower(), '-Dbuild.dir=' + str(ROOT / 'target/upstream-data'), '--add-opens=java.base/java.util=ALL-UNNAMED',
               '--add-opens=java.base/java.lang=ALL-UNNAMED',
               '--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED',
               '-cp', os.pathsep.join(cp), 'io.crabgraph.conformance.ProviderSuite',
               args.selection, str(args.upstream), str(args.output)]
    if args.inventory:
        command.append('inventory')
    environment = dict(os.environ)
    if args.store:
        environment['CRABGRAPH_JVM_STORE'] = str(args.store.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # A failed/terminated run never inherits an older successful report.
    args.output.unlink(missing_ok=True)
    provenance = {
        'source_commit': subprocess.check_output(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], text=True).strip(),
        'working_tree_modified': bool(subprocess.check_output(['git', '-C', str(REPO), 'status', '--porcelain'], text=True).strip()),
        'harness_working_tree_modified': bool(subprocess.check_output(['git', '-C', str(REPO), 'status', '--porcelain', '--', str(ROOT)], text=True).strip()),
        'harness_source_sha256': {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT / 'src').rglob('*.java'))},
        'harness_jar_sha256': digest(harness_jar),
        'provider_source_commit': args.provider_source_commit,
        'provider_jar_sha256': digest(args.jvm_jar) if args.jvm_jar else None,
        'store_sha256': digest(args.store) if args.store else None,
    }
    try:
        completed = subprocess.run(command, env=environment, timeout=args.timeout)
        exit_code = completed.returncode
    except subprocess.TimeoutExpired:
        exit_code = 124
    if args.output.exists():
        report = json.loads(args.output.read_text())
        report['run_complete'] = report.get('run_complete', False) and exit_code in (0, 1)
        report['process_exit_code'] = exit_code
        report.update(provenance)
        report['jvm_flags'] = [arg for arg in command[1:] if arg.startswith('-D') or arg.startswith('--add-opens')]
        report['java_version'] = subprocess.run([args.java, '-version'], capture_output=True, text=True).stderr
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    return exit_code


if __name__ == '__main__':
    sys.exit(main())
