#!/usr/bin/env python3
"""Fail before signing/uploading if any platform artifact is missing or incompatible."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native_package', ROOT / 'scripts/package-native.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--version', required=True)
p.add_argument('--source', type=Path, default=ROOT)
p.add_argument('--platforms', nargs='+', choices=module.PLATFORMS, default=['linux-x86_64', 'linux-aarch64', 'macos-aarch64', 'macos-x86_64'])
p.add_argument('--directory', type=Path, default=None)
a = p.parse_args()
ROOT = a.source.resolve()
a.directory = a.directory or ROOT / 'target/native-artifacts'
for platform in a.platforms:
    library = module.PLATFORMS[platform]
    with zipfile.ZipFile(a.directory / (platform + '.jar')) as jar:
        prefix = f'io/orchiddb/native/{platform}/'
        props = dict(line.split('=', 1) for line in jar.read(prefix+'build.properties').decode().splitlines())
        assert props['version'] == a.version, platform + ': wrong version'
        assert props['coreRevision'] == (ROOT / 'native/CORE_REVISION').read_text().strip(), platform + ': wrong compiler'
        assert props['sha256'] == hashlib.sha256(jar.read(prefix+library)).hexdigest(), platform + ': wrong checksum'
        assert jar.read('META-INF/LICENSE.md') == (ROOT / 'LICENSE.md').read_text(encoding='utf-8').encode('utf-8')
    print('Verified release artifact:', platform)
