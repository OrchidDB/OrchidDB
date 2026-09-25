"""Run the actual Bash installer against deterministic GitHub/curl fixtures."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]

class Installer(unittest.TestCase):
    def run_install(self, system='Darwin', arch='arm64', corrupt=False, version='v0.1.0', empty=False, glibc='2.35'):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tools = root / 'tools'; tools.mkdir()
            target = {'Darwin/arm64': 'aarch64-apple-darwin', 'Darwin/x86_64': 'x86_64-apple-darwin', 'Linux/x86_64': 'x86_64-unknown-linux-gnu'}.get(f'{system}/{arch}', 'unsupported')
            stem = f'orchiddb-v0.1.0-{target}'
            asset = root / (stem + '.tar.gz')
            with tarfile.open(asset, 'w:gz') as archive:
                data = b'#!/bin/sh\necho installed-fixture\n'
                entry = tarfile.TarInfo(stem + '/orchiddb'); entry.size = len(data); entry.mode = 0o755
                archive.addfile(entry, io.BytesIO(data))
            checksum = '0' * 64 if corrupt else hashlib.sha256(asset.read_bytes()).hexdigest()
            (root / 'SHA256SUMS').write_text(f'{checksum}  {asset.name}\n')
            (tools / 'uname').write_text(f'#!/bin/sh\nif [ "$1" = -s ]; then echo {system}; else echo {arch}; fi\n')
            (tools / 'ldd').write_text(f'#!/bin/sh\necho "ldd (GNU libc) {glibc}"\n')
            (tools / 'curl').write_text('''#!/usr/bin/env python3
import os, pathlib, shutil, sys
args=sys.argv[1:]
if any('api.github.com' in x for x in args):
 print('[]' if os.environ.get('EMPTY') else '[{"tag_name": "v0.1.0"}]')
else:
 url=next(x for x in args if x.startswith('https://'))
 source=pathlib.Path(os.environ['FIXTURES']) / url.rsplit('/',1)[1]
 shutil.copyfile(source,args[args.index('-o')+1])
''')
            for file in tools.iterdir(): file.chmod(0o755)
            destination = root / 'my bin'; destination.mkdir()
            original = destination / 'orchiddb'; original.write_text('original')
            env = dict(os.environ, PATH=f'{tools}:{os.environ["PATH"]}', FIXTURES=temp, ORCHIDDB_INSTALL_DIR=str(destination), ORCHIDDB_VERSION=version)
            if empty: env['EMPTY'] = '1'
            result = subprocess.run(['bash', str(ROOT / 'website/install/install.sh')], env=env, capture_output=True, text=True)
            return result, original.read_text(), bool(original.stat().st_mode & 0o111)

    def test_supported_platforms_install_to_directory_with_spaces(self):
        for system, arch in [('Darwin','arm64'),('Darwin','x86_64'),('Linux','x86_64')]:
            result, content, executable = self.run_install(system,arch)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('installed-fixture',content)
            self.assertTrue(executable)

    def test_tampering_preserves_existing_install(self):
        result, content, _ = self.run_install(corrupt=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('Checksum mismatch',result.stderr)
        self.assertEqual(content,'original')

    def test_old_glibc_preserves_existing_install(self):
        result, content, _ = self.run_install('Linux', 'x86_64', glibc='2.31')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('glibc 2.35 or newer', result.stderr)
        self.assertEqual(content, 'original')

    def test_unknown_platform_is_rejected(self):
        result, content, _ = self.run_install('Linux','aarch64')
        self.assertNotEqual(result.returncode,0)
        self.assertIn('Unsupported platform',result.stderr)
        self.assertEqual(content,'original')

    def test_discovers_published_version(self):
        result, _, _ = self.run_install(version='latest')
        self.assertEqual(result.returncode,0,result.stderr)

    def test_empty_releases_have_actionable_error(self):
        result, content, _ = self.run_install(version='latest',empty=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('No published release yet',result.stderr)
        self.assertEqual(content,'original')

    def test_version_cannot_inject_a_path(self):
        result, content, _ = self.run_install(version='../bad')
        self.assertNotEqual(result.returncode,0)
        self.assertIn('ORCHIDDB_VERSION must',result.stderr)
        self.assertEqual(content,'original')

if __name__ == '__main__': unittest.main()
