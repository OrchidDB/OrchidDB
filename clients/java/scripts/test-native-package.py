#!/usr/bin/env python3
"""Test the actual published JAR shape in a fresh JVM, including incompatible artifact rejection."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def run(jar, version, negative=True):
    output = ROOT / 'target/native-smoke-classpath.txt'
    subprocess.run(['mvn.cmd' if os.name == 'nt' else 'mvn', '-q', '-pl', 'orchiddb-java',
                    'dependency:build-classpath', '-DincludeScope=runtime',
                    f'-Dmdep.outputFile={output}'], cwd=ROOT, check=True)
    runtime = output.read_text().strip()
    if 'duckdb_jdbc' in runtime or 'gremlin-core' in runtime:
        raise AssertionError('Core runtime includes an unwanted database/TinkerPop dependency')
    api = ROOT / f'orchiddb-java/target/orchiddb-java-{version}.jar'
    java = str(Path(os.environ['JAVA_HOME']) / 'bin' / ('java.exe' if os.name == 'nt' else 'java'))

    def launch(native):
        cp = os.pathsep.join([str(api), str(native), runtime])
        return subprocess.run([java, '-cp', cp, str(ROOT / 'scripts/NativeSmoke.java')],
                              text=True, capture_output=True, cwd=ROOT)

    result = launch(jar)
    if result.returncode:
        raise AssertionError(result.stderr)
    print(result.stdout.strip())
    if not negative:
        return
    # Change only metadata, so these fail before System.load attempts to load the binary.
    with tempfile.TemporaryDirectory() as folder:
        for field, value, message in [
                ('version', '999.0.0', 'version mismatch'),
                ('coreRevision', '0' * 40, 'revision mismatch'),
                ('sha256', '0' * 64, 'checksum mismatch')]:
            bad = Path(folder) / (field + '.jar')
            with zipfile.ZipFile(jar) as source, zipfile.ZipFile(bad, 'w') as dest:
                for entry in source.infolist():
                    content = source.read(entry)
                    if entry.filename.endswith('/build.properties'):
                        lines = content.decode().splitlines()
                        content = ('\n'.join(f'{field}={value}' if line.startswith(field+'=') else line
                                             for line in lines)+'\n').encode()
                    dest.writestr(entry, content)
            result = launch(bad)
            assert result.returncode and message in result.stderr, result.stderr
            print('Rejected incompatible artifact:', field)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--jar', type=Path, required=True)
    p.add_argument('--version', required=True)
    p.add_argument('--no-negative', action='store_true')
    a = p.parse_args()
    run(a.jar.resolve(), a.version, not a.no_negative)
