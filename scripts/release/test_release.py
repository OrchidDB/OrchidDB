"""Exercise packaging and release boundaries without publishing to GitHub."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import package_extension as packaging
import publish_extension as publishing


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.build = self.root / 'extension/build'
        self.build.mkdir(parents=True)
        (self.root / 'LICENSE.md').write_text('test license')
        fields = ['4', 'osx_arm64', 'v1.5.6', '0.1.0', 'CPP', '', '', '']
        payload = b'fixture' + b''.join(f.encode().ljust(32, b'\0') for f in reversed(fields)) + bytes(256)
        (self.build / 'orchid.duckdb_extension').write_bytes(payload)
        self.receipt = dict(revision='a' * 40, working_tree_modified=False, profile='release',
                            reused_rust=False, sha256=hashlib.sha256(payload).hexdigest())
        self.write_receipt()

    def write_receipt(self):
        (self.build / 'build-manifest.json').write_text(json.dumps(self.receipt))

    def test_archive_contains_only_extension_and_metadata_and_is_repeatable(self):
        directory = packaging.package(self.root)
        archive, checksum, manifest = publishing.bundle(directory, '0.1.0', self.receipt['revision'])
        expected_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assertEqual(checksum.read_text(), f'{expected_hash}  {archive.name}\n')
        self.assertEqual(manifest['revision'], self.receipt['revision'])
        with tarfile.open(archive) as tar:
            self.assertEqual({Path(n).name for n in tar.getnames()},
                             {'orchid.duckdb_extension', 'LICENSE.md', 'manifest.json', 'SHA256SUMS'})
            binary = next(n for n in tar.getnames() if n.endswith('.duckdb_extension'))
            self.assertEqual(hashlib.sha256(tar.extractfile(binary).read()).hexdigest(), manifest['sha256'])
        publishing.bundle(directory, '0.1.0', self.receipt['revision'])
        self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), expected_hash)

    def test_rejects_mismatched_receipt(self):
        self.receipt['sha256'] = 'wrong'
        self.write_receipt()
        with self.assertRaisesRegex(SystemExit, 'does not match'):
            packaging.package(self.root)

    def test_rejects_unpublishable_provenance_and_version(self):
        for changes in ({'revision': 'old'}, {'working_tree_modified': True},
                        {'profile': 'debug'}, {'reused_rust': True}):
            with self.subTest(changes=changes):
                receipt = dict(self.receipt, **changes)
                (self.build / 'build-manifest.json').write_text(json.dumps(receipt))
                with self.assertRaisesRegex(SystemExit, 'clean revision'):
                    publishing.bundle(packaging.package(self.root), '0.1.0', self.receipt['revision'])
        self.write_receipt()
        with self.assertRaisesRegex(SystemExit, 'version differs'):
            publishing.bundle(packaging.package(self.root), '0.2.0', self.receipt['revision'])
        (self.build / 'build-manifest.json').unlink()
        with self.assertRaisesRegex(SystemExit, 'clean revision'):
            publishing.bundle(packaging.package(self.root), '0.1.0', self.receipt['revision'])

    def test_preflight_rejects_missing_version_dirty_source_and_existing_tag(self):
        with patch.object(publishing, 'output') as output:
            with self.assertRaisesRegex(SystemExit, 'VERSION'):
                publishing.preflight('', 'OrchidDB/OrchidDB')
            output.assert_not_called()
            output.return_value = ' M file'
            with self.assertRaisesRegex(SystemExit, 'clean'):
                publishing.preflight('0.1.0', 'OrchidDB/OrchidDB')
        with patch.object(publishing, 'output', side_effect=['', 'a'*40, 'a'*40,
                   '[{"ref":"refs/tags/extension-v0.1.0"}]']):
            with self.assertRaisesRegex(SystemExit, 'already exists'):
                publishing.preflight('0.1.0', 'OrchidDB/OrchidDB')
        with patch.object(publishing, 'output', side_effect=['', 'a'*40, subprocess.CalledProcessError(1, 'gh')]):
            with self.assertRaises(subprocess.CalledProcessError):
                publishing.preflight('0.1.0', 'OrchidDB/OrchidDB')

    def test_publish_uses_exact_revision_and_only_extension_assets(self):
        directory = packaging.package(self.root)
        with patch.object(sys, 'argv', ['publish', '--version', '0.1.0']), \
             patch.object(publishing, 'preflight', return_value=(self.receipt['revision'], 'extension-v0.1.0')), \
             patch.object(publishing, 'package', return_value=directory), \
             patch.object(publishing.subprocess, 'run') as run:
            publishing.main()
        command = run.call_args.args[0]
        self.assertEqual(command[:4], ['gh', 'release', 'create', 'extension-v0.1.0'])
        self.assertEqual(command[command.index('--target')+1], self.receipt['revision'])
        self.assertTrue(command[4].endswith('.tar.gz'))
        self.assertTrue(command[5].endswith('.tar.gz.sha256'))
        self.assertNotIn('--clobber', command)
        self.assertTrue(Path(command[command.index('--notes-file')+1]).is_file())

    def test_make_release_stops_before_publication_when_tests_fail(self):
        # Replace only the executables, retaining real make recursion and -j.
        stub = self.root / 'stub.py'
        log = self.root / 'calls'
        stub.write_text('import sys\nfrom pathlib import Path\n'
                        f'with Path({str(log)!r}).open("a") as f: f.write(" ".join(sys.argv[1:])+"\\n")\n'
                        'sys.exit(1 if "discover" in sys.argv else 0)\n')
        result = subprocess.run(['make', '-j4', '-f', 'scripts/release/Makefile', 'release',
                                 'VERSION=0.1.0', f'PYTHON={sys.executable} {stub}'],
                                cwd=packaging.ROOT, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        calls = log.read_text().splitlines()
        self.assertIn('--check', calls[0])
        self.assertIn('build.py --release --extension-version 0.1.0', calls[1])
        self.assertEqual(len(calls), 3, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
