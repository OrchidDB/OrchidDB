"""Execute pinned JUnit counterparts for upstream Gherkin placeholders locally."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PREFIX = 'gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/'


def counterpart(case, selection):
    key = case['path'].removeprefix(PREFIX) + ':' + str(case['line'])
    matches = [row for row in selection['cases'] if row['gherkin'] == key]
    if not matches:
        return None
    if len(matches) != 1:
        raise ValueError('Duplicate Java counterpart: ' + key)
    row = matches[0]
    if (case['name'] != row['scenario'] or case['source_sha256'] != row['feature_source_sha256']
            or [step['text'] for step in case['steps']] != ['an unsupported test', 'nothing should happen because']):
        raise ValueError('Stale Java counterpart mapping: ' + key)
    return row


def result_for(case, mapping, report, revision):
    if (report.get('upstream_revision') != revision or report.get('run_complete') is not True
            or report.get('inventory_only') is not False or report.get('upstream_assertions_modified') is not False
            or report.get('profile') != 'jvm-provider' or report.get('process_exit_code') not in (0, 1)):
        raise ValueError('Incomplete or incompatible Java assertion run')
    identity = mapping['java_class'] + '#' + mapping['java_method']
    matches = [row for row in report['cases'] if row['id'] == identity]
    if len(matches) != 1:
        raise ValueError('Missing or duplicate Java assertion: ' + identity)
    row = matches[0]
    if row.get('source_sha256') != mapping['source_sha256'] or row.get('placeholder') != mapping:
        raise ValueError('Stale Java assertion source or mapping: ' + identity)
    if row['status'] not in ('pass', 'fail', 'skipped') or row.get('elapsed_ms', -1) < 0:
        raise ValueError('Unfinished Java assertion: ' + identity)
    provenance = {key: value for key, value in report.items() if key not in ('cases', 'counts')}
    return {'status': row['status'], 'elapsed_ms': row['elapsed_ms'],
            'assertion_engine': 'Apache gremlin-test 3.7.4 JUnit (unmodified)',
            'assertion_source': {'kind': 'java-counterpart', 'id': identity,
                'source': mapping['source'], 'source_sha256': mapping['source_sha256'],
                'url': 'https://github.com/apache/tinkerpop/blob/' + revision + '/' + mapping['source'],
                'gherkin_status': 'placeholder', 'gherkin_case_id': case['id']},
            'java_assertion': row, 'java_run': provenance,
            'reason': row.get('reason', 'Executed original Java assertions for the upstream Gherkin placeholder'),
            **({'error': row['error']} if 'error' in row else {})}


class JavaAssertions:
    def __init__(self, classpath):
        self.classpath = classpath
        self.selection = json.loads((ROOT / 'adapters/jvm-provider-tests/placeholders.json').read_text())
        self.report = None
        self.failure = None

    def run(self, case):
        mapping = counterpart(case, self.selection)
        if mapping is None:
            return None
        if self.failure:
            raise RuntimeError(self.failure)
        if self.report is None:
            try:
                self.report = self.execute()
            except Exception as error:
                self.failure = str(error)
                raise
        return result_for(case, mapping, self.report, self.selection['upstream_revision'])

    def execute(self):
        if os.environ.get('GITHUB_ACTIONS') == 'true':
            raise RuntimeError('Conformance runs locally only')
        upstream = Path(os.environ.get('CONFORMANCE_TINKERPOP_SOURCE',
            str(Path(os.environ.get('CONFORMANCE_UPSTREAM_CACHE', ROOT / 'upstream/cache')) / 'tinkerpop')))
        revision = subprocess.check_output(['git', '-C', str(upstream), 'rev-parse', 'HEAD'], text=True).strip()
        if revision != self.selection['upstream_revision']:
            raise ValueError('Wrong TinkerPop source revision')
        for row in self.selection['cases']:
            for path, expected in ((row['source'], row['source_sha256']),
                                   (PREFIX + row['gherkin'].rsplit(':', 1)[0], row['feature_source_sha256'])):
                if hashlib.sha256((upstream / path).read_bytes()).hexdigest() != expected:
                    raise ValueError('Modified upstream source: ' + path)
        jars = [Path(p) for p in self.classpath.split(os.pathsep) if Path(p).name == 'crabgraph-jvm-0.1.0.jar']
        if len(jars) != 1:
            raise ValueError('Expected one production Crabgraph JVM jar')
        with tempfile.TemporaryDirectory(prefix='gremlin-java-assertions-') as folder:
            work = Path(folder)
            cp = work / 'classpath.txt'; cp.write_text(self.classpath)
            output = work / 'results.json'
            command = [sys.executable, str(ROOT / 'adapters/jvm-provider-tests/run.py'),
                '--upstream', str(upstream), '--store', os.environ.get('CRABGRAPH_JVM_STORE', str(ROOT.parent / 'target/debug/crabgraph-jvm-store')),
                '--jvm-jar', str(jars[0]), '--jvm-classpath', str(cp), '--output', str(output),
                '--java', os.environ.get('CONFORMANCE_JAVA', 'java')]
            with (ROOT / 'upstream-java-assertions.log').open('a') as log:
                completed = subprocess.run(command, stdout=log, stderr=log, timeout=240)
            if completed.returncode not in (0, 1) or not output.exists():
                raise RuntimeError('Java assertion runner failed; see conformance/upstream-java-assertions.log')
            return json.loads(output.read_text())
