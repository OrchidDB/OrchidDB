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
import matrix


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
            with self.assertRaisesRegex(SystemExit, r'(?s)clean.*Blocking changes.*M file'):
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
             patch.object(publishing, 'validated_packages', return_value=[directory]), \
             patch.object(publishing.subprocess, 'run') as run:
            publishing.main()
        command = run.call_args.args[0]
        self.assertEqual(command[:4], ['gh', 'release', 'create', 'extension-v0.1.0'])
        self.assertEqual(command[command.index('--target')+1], self.receipt['revision'])
        self.assertTrue(command[4].endswith('.tar.gz'))
        self.assertTrue(command[5].endswith('.tar.gz.sha256'))
        self.assertNotIn('--clobber', command)
        self.assertTrue(Path(command[command.index('--notes-file')+1]).is_file())

    def test_make_release_propagates_pipeline_failure_without_publishing(self):
        stub = self.root / 'stub.py'
        log = self.root / 'calls'
        stub.write_text('import sys\nfrom pathlib import Path\n'
                        f'Path({str(log)!r}).write_text(" ".join(sys.argv[1:]))\n'
                        'sys.exit(1)\n')
        overrides = self.root / 'steps.mk'
        overrides.write_text('release-prepare release-env:\n\t@:\n')
        result = subprocess.run(['make', '-j4', '-f', 'Makefile', '-f', str(overrides), 'release',
                                 f'MAKE=make -f Makefile -f {overrides}',
                                 'VERSION=0.1.0', f'PYTHON={sys.executable} {stub}'],
                                cwd=packaging.ROOT, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(log.read_text(), 'scripts/release/release.py all --version 0.1.0')

    def test_publication_requires_exactly_three_validated_platforms(self):
        commit = self.receipt['revision']
        directory = matrix.location('0.1.0', commit, self.root)
        matrix.write(directory / 'compiler-validation.json', dict(revision=commit))
        self.assertEqual(set(matrix.PLATFORMS), {'osx_arm64', 'linux_arm64', 'linux_amd64'})
        with self.assertRaisesRegex(SystemExit, 'all three'):
            matrix.validated_packages('0.1.0', commit, self.root)
        import shutil
        for target in matrix.PLATFORMS:
            build = directory / target
            shutil.copytree(self.build, build)
            artifact = build / 'orchid.duckdb_extension'
            payload = artifact.read_bytes()
            # DuckDB metadata stores platform in the seventh 32-byte field.
            payload = payload[:-320] + target.encode().ljust(32, b'\0') + payload[-288:]
            artifact.write_bytes(payload)
            checksum = matrix.digest(artifact)
            record = dict(self.receipt, extension_version='0.1.0', platform=target, sha256=checksum)
            matrix.write(build / 'build-manifest.json', record)
            matrix.write(build / 'validation.json', dict(revision=commit, sha256=checksum,
                                                       platform=target, external_storage=True))
        self.assertEqual(len(matrix.validated_packages('0.1.0', commit, self.root)), 3)
        matrix.write(directory / 'linux_amd64/validation.json', {'sha256': 'stale'})
        with self.assertRaisesRegex(SystemExit, 'Missing validation for linux_amd64'):
            matrix.validated_packages('0.1.0', commit, self.root)

    def test_reuse_requires_the_same_revision_version_platform_and_binary(self):
        self.receipt.update(platform='osx_arm64', extension_version='0.1.0')
        self.write_receipt()
        self.assertTrue(matrix.reusable(self.build, '0.1.0', self.receipt['revision'], 'osx_arm64'))
        for version, commit, target in [('0.2.0', self.receipt['revision'], 'osx_arm64'),
                                       ('0.1.0', 'old', 'osx_arm64'),
                                       ('0.1.0', self.receipt['revision'], 'linux_arm64')]:
            self.assertFalse(matrix.reusable(self.build, version, commit, target))
        (self.build / 'orchid.duckdb_extension').write_bytes(b'changed')
        self.assertFalse(matrix.reusable(self.build, '0.1.0', self.receipt['revision'], 'osx_arm64'))


if __name__ == '__main__':
    unittest.main()
