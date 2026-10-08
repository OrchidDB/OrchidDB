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

    for artifact in ['orchiddb-java', 'orchiddb-gremlin']:
        module = ROOT / artifact
        for suffix in ['', '-sources', '-javadoc']:
            filename = f'{artifact}-{version}{suffix}.jar'
            jar = module / 'target' / filename
            add(('docs/' if suffix else 'lib/') + filename, jar)
        dependencies = list((module / 'target/runtime-deps').glob('*.jar'))
        if not dependencies:
            raise ValueError('Missing runtime dependencies for ' + artifact)
        for dependency in dependencies:
            add('lib/' + dependency.name, dependency)
    add('LICENSE.md', ROOT / 'LICENSE.md')
    add('README.md', ROOT / 'README.md')
    for document in sorted((ROOT / 'docs').glob('*.md')):
        add('docs/' + document.name, document)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    revisions = json.loads(native_revisions.read_text()) if native_revisions else {platform: (ROOT / 'native/CORE_REVISION').read_text().strip() for platform in platforms}
    output.mkdir(parents=True, exist_ok=True)
    archives = []
    for platform in platforms:
        entries = dict(files)
        name = f'orchiddb-java-{version}-{platform}.jar'
        entries['lib/' + name] = (ROOT / 'target/native-artifacts' / (platform + '.jar')).read_bytes()
        entries['README.txt'] = f'OrchidDB JVM {version} for {platform}\n\nUse lib/* on your runtime classpath, for example:\njava -cp "lib/*:your-application.jar" your.Main\n\nSupply your own JDBC driver for DuckDB or PostgreSQL.\n'.encode()
        manifest = {'version': version, 'commit': revision, 'distribution': 'github',
                    'platform': platform, 'native_revisions': {platform: revisions[platform]},
                    'files': {name: hashlib.sha256(data).hexdigest() for name, data in sorted(entries.items())}}
        entries['release-manifest.json'] = (json.dumps(manifest, indent=2) + '\n').encode()
        archive = output / f'orchiddb-java-{version}-{platform}.zip'
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as target:
            for name, data in sorted(entries.items()):
                info = zipfile.ZipInfo(name, date_time=zip_time)
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                target.writestr(info, data)
        archives.append(archive)
        print(archive)
    (output / 'SHA256SUMS').write_text(''.join(hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.name + '\n' for path in archives))
    return archives


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source', type=Path, default=ROOT)
    parser.add_argument('--platforms', nargs='+', choices=PLATFORMS)
    parser.add_argument('--native-revisions', type=Path)
    args = parser.parse_args()
    ROOT = args.source.resolve()
    package(args.output, args.platforms, args.native_revisions)
