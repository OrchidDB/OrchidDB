#!/usr/bin/env python3
"""Create an explicitly selected compiler classifier JAR; never include a database engine."""
import argparse
import hashlib
from pathlib import Path
import re
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = {
    'linux-aarch64': 'liborchiddb_java.so',
    'linux-x86_64': 'liborchiddb_java.so',
    'macos-aarch64': 'liborchiddb_java.dylib',
    'macos-x86_64': 'liborchiddb_java.dylib',
}


def package(platform, library, version, output):
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?', version):
        raise ValueError('Expected a semantic release or snapshot version')
    data = library.read_bytes()
    if not data:
        raise ValueError('Native compiler file is empty')
    revision = (ROOT / 'native/CORE_REVISION').read_text().strip()
    prefix = f'io/orchiddb/native/{platform}/'
    entries = {
        prefix + PLATFORMS[platform]: data,
        prefix + 'build.properties': (
            f'version={version}\ncoreRevision={revision}\n'
            f'sha256={hashlib.sha256(data).hexdigest()}\n').encode(),
        'META-INF/LICENSE.md': (ROOT / 'LICENSE.md').read_text(encoding='utf-8').encode('utf-8'),
        'META-INF/MANIFEST.MF': b'Manifest-Version: 1.0\r\n\r\n',
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as jar:
        for name, content in sorted(entries.items()):
            info = zipfile.ZipInfo(name, date_time=(2026, 9, 25, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            jar.writestr(info, content)
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT)
    parser.add_argument('--platform', choices=PLATFORMS, required=True)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    ROOT = args.source.resolve()
    package(args.platform, args.library, args.version, args.output)
