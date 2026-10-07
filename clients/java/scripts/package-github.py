#!/usr/bin/env python3
"""Package completed JVM modules and native classifiers for GitHub Releases."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TOOLS = Path(__file__).resolve().parent
PLATFORMS = ['linux-x86_64', 'linux-aarch64', 'macos-aarch64', 'macos-x86_64']
NS = {'m': 'http://maven.apache.org/POM/4.0.0'}


def package(output, platforms=None, native_revisions=None):
    platforms = platforms or PLATFORMS
    pom = ET.parse(ROOT / 'pom.xml').getroot()
    version = pom.findtext('m:version', namespaces=NS)
    group = pom.findtext('m:groupId', namespaces=NS)
    parent = pom.findtext('m:artifactId', namespaces=NS)
    if not all([version, group, parent]):
        raise ValueError('Root POM must declare version, groupId, and artifactId')
    timestamp = pom.findtext('m:properties/m:project.build.outputTimestamp', namespaces=NS)
    if timestamp is None:
        raise ValueError('Root POM must declare project.build.outputTimestamp')
    zip_time = datetime.fromisoformat(timestamp.replace('Z', '+00:00')).astimezone(timezone.utc).timetuple()[:6]
    verification = ['python3', str(TOOLS / 'verify-native-artifacts.py'), '--source', str(ROOT), '--version', version, '--platforms', *platforms]
    if native_revisions:
        verification += ['--native-revisions', str(native_revisions)]
    subprocess.run(verification, cwd=ROOT, check=True)
    files = {}

    def add(name, path):
        data = path.read_bytes()
        if name in files and files[name] != data:
            raise ValueError('Conflicting distribution file: ' + name)
        files[name] = data

    def repository(artifact, name, path):
        destination = f'repository/{group.replace(".", "/")}/{artifact}/{version}/{name}'
        add(destination, path)
        for algorithm in ['sha1', 'sha256']:
            files[destination + '.' + algorithm] = (hashlib.new(algorithm, files[destination]).hexdigest() + '\n').encode()

    repository(parent, f'{parent}-{version}.pom', ROOT / 'pom.xml')
    for artifact in ['orchiddb-java', 'orchiddb-gremlin']:
        module = ROOT / artifact
        repository(artifact, f'{artifact}-{version}.pom', module / 'pom.xml')
        for suffix in ['', '-sources', '-javadoc']:
            filename = f'{artifact}-{version}{suffix}.jar'
            jar = module / 'target' / filename
            repository(artifact, filename, jar)
            if not suffix:
                add('lib/' + filename, jar)
        dependencies = list((module / 'target/runtime-deps').glob('*.jar'))
        if not dependencies:
            raise ValueError('Missing runtime dependencies for ' + artifact)
        for dependency in dependencies:
            add('lib/' + dependency.name, dependency)
    for platform in platforms:
        path = ROOT / 'target/native-artifacts' / (platform + '.jar')
        name = f'orchiddb-java-{version}-{platform}.jar'
        repository('orchiddb-java', name, path)
        add('lib/' + name, path)
    add('LICENSE.md', ROOT / 'LICENSE.md')
    add('README.md', ROOT / 'README.md')
    for document in sorted((ROOT / 'docs').glob('*.md')):
        add('docs/' + document.name, document)
    files['README.txt'] = f'''OrchidDB JVM {version}

This distribution is published on GitHub Releases, not Sonatype/Maven Central.

Use lib/* on your application's runtime classpath. It contains the JVM and
Gremlin clients, runtime dependencies, and the selected native classifiers.
Included native classifiers: {", ".join(platforms)}.
The loader selects the classifier for the current OS and architecture.
Example: java -cp "lib/*:your-application.jar" your.Main.
Supply your own JDBC driver for your database, such as DuckDB or PostgreSQL.

For Maven, the repository/ directory contains the OrchidDB POMs, main JARs,
sources, Javadocs, and native classifiers in standard repository layout. Add
its absolute file:/// URL as a repository in your application, or copy its
{group.replace('.', '/')} subtree to your local Maven repository. Ordinary third-party
dependencies are still resolved through your normal Maven repositories.
Declare {group}:orchiddb-java:{version} and one runtime dependency with the
same coordinates and the classifier matching your OS:
{", ".join(platforms)}.
For Gremlin, also declare {group}:orchiddb-gremlin:{version}.

See README.md and the client documentation for API usage. Native classifiers
were reused from the recorded release builds; this package rebuilds none.
'''.encode()
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    manifest = {'version': version, 'commit': revision, 'distribution': 'github',
                'native_revisions': json.loads(native_revisions.read_text()) if native_revisions else {platform: (ROOT / 'native/CORE_REVISION').read_text().strip() for platform in platforms},
                'files': {name: hashlib.sha256(data).hexdigest() for name, data in sorted(files.items())}}
    files['release-manifest.json'] = (json.dumps(manifest, indent=2) + '\n').encode()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'orchiddb-java-{version}.zip'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as target:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, date_time=zip_time)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            target.writestr(info, data)
    (output / 'SHA256SUMS').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
    print(archive)
    return archive


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source', type=Path, default=ROOT)
    parser.add_argument('--platforms', nargs='+', choices=PLATFORMS)
    parser.add_argument('--native-revisions', type=Path)
    args = parser.parse_args()
    ROOT = args.source.resolve()
    package(args.output, args.platforms, args.native_revisions)
