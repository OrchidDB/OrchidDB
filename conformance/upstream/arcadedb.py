"""Pinned embedded ArcadeDB transports; never supplies expected answers."""
import os
import subprocess
import tempfile
from pathlib import Path
from run import ROOT, REPO, Process, file_identity


def classpath(directory, filename, override):
    value = os.environ.get(override)
    if value:
        return value
    path = directory / filename
    if not path.is_file():
        raise RuntimeError('Build ArcadeDB adapters first: bash conformance/build-arcadedb.sh')
    return str(directory / Path(filename).parent / 'classes') + os.pathsep + path.read_text().strip()


def gremlin_classpath():
    cp = classpath(ROOT / 'adapters/sqlg', 'target-arcadedb/classpath.txt',
                   'CONFORMANCE_ARCADEDB_GREMLIN_CLASSPATH')
    # Keep the original upstream StepDefinition's ANTLR ABI. ArcadeDB's parser
    # has a different ABI and is loaded independently by ArcadeProviderLoader.
    runtime = ROOT / 'adapters/sqlg/target-arcadedb/antlr4-runtime-4.9.1.jar'
    if not runtime.is_file():
        raise RuntimeError('Build ArcadeDB adapters first: bash conformance/build-arcadedb.sh')
    return str(runtime) + os.pathsep + cp + os.pathsep + str(ROOT / 'adapters/sqlg/target-arcadedb/gremlin-core-3.7.4.jar')


def cypher_classpath():
    return classpath(ROOT / 'adapters/arcadedb', 'target/classpath.txt',
                     'CONFORMANCE_ARCADEDB_CYPHER_CLASSPATH')


class ArcadeProcess(Process):
    def __init__(self):
        self.fixture_directory = tempfile.TemporaryDirectory(prefix='orchiddb-arcadedb-cypher-')
        try:
            super().__init__([os.environ.get('CONFORMANCE_JAVA', 'java'),
                              '-Djava.util.logging.config.file=' + str(ROOT / 'adapters/arcadedb/logging.properties'), '-cp',
                              cypher_classpath(), 'ArcadeCypher', self.fixture_directory.name],
                             ROOT / 'upstream-arcadedb-cypher.log', ready=True)
        except Exception:
            self.fixture_directory.cleanup()
            raise

    def close(self):
        try:
            super().close()
        finally:
            self.fixture_directory.cleanup()


def cypher_process():
    return ArcadeProcess()


def build_identity(suite, adapter):
    cp = adapter.classpath if suite == 'tinkerpop' else cypher_classpath()
    artifacts = []
    for entry in cp.split(os.pathsep):
        path = Path(entry)
        if path.is_dir():
            artifacts.extend(file_identity(p) for p in sorted(path.rglob('*.class')))
        else:
            artifacts.append(file_identity(path))
    return {
        'adapter_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
        'adapter_working_tree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True).strip()),
        'classpath': artifacts,
        'java': subprocess.check_output([os.environ.get('CONFORMANCE_JAVA', 'java'), '-version'], stderr=subprocess.STDOUT, text=True).strip(),
        'storage': 'fresh temporary embedded database per scenario',
        'gremlin_runtime': '3.8.1' if suite == 'tinkerpop' else None,
        'adapter_sources': [file_identity(ROOT / 'upstream' / name) for name in ('arcadedb.py', 'arcadedb_errors.py', 'run.py', 'cypher.py')] + [file_identity(ROOT / 'adapters' / name) for name in ('arcadedb/src/main/java/ArcadeCypher.java', 'sqlg/src/main/java/UpstreamGremlin.java', 'sqlg/src/main/java/ArcadeProviderLoader.java', 'sqlg/src/main/java/ArcadeGremlinBootstrap.java', 'sqlg/src/main/java/NativeJUnitAssertions.java', 'arcadedb/pom.xml', 'arcadedb/logging.properties', 'sqlg/pom.xml')],
    }
