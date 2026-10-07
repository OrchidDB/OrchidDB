"""Release orchestration and archive checks; no uploads, Docker, or cross builds."""
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch, MagicMock
import release


class CompleteReleaseTests(unittest.TestCase):
    def test_archive_is_deterministic_and_preserves_execution(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); program = root / 'program'
            program.write_bytes(b'executable'); program.chmod(0o755)
            output = root / 'package.tar.gz'
            entries = {'bin/orchiddb': program, 'README': b'usage'}
            release.archive(output, entries)
            before = output.read_bytes()
            release.archive(output, entries)
            self.assertEqual(before, output.read_bytes())
            with tarfile.open(output) as archive:
                self.assertEqual(archive.getmember('bin/orchiddb').mode, 0o755)
                self.assertEqual(archive.extractfile('README').read(), b'usage')
            with self.assertRaises(ValueError):
                release.archive(output, {'../escape': b'x'})

    def test_failure_stops_before_build_packaging_or_upload(self):
        instance = MagicMock()
        instance.build.side_effect = RuntimeError('build failed')
        with patch.object(release, 'Release', return_value=instance), patch('sys.argv', ['release', 'all', '--version', '1.2.3']):
            with self.assertRaisesRegex(RuntimeError, 'build failed'): release.main()
        instance.check.assert_called_once()
        instance.build.assert_called_once()
        instance.package.assert_not_called()
        instance.verify.assert_not_called()

    def test_all_stages_run_in_order(self):
        instance = MagicMock()
        with patch.object(release, 'Release', return_value=instance), patch('sys.argv', ['release', 'all', '--version', '1.2.3']):
            release.main()
        self.assertEqual([call[0] for call in instance.mock_calls], ['check', 'build', 'package'])

    def test_reuse_requires_all_hashes_and_the_exact_commit(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(release, 'ROOT', Path(tmp)):
            root = Path(tmp); data = root / 'artifact'; data.write_bytes(b'good')
            instance = object.__new__(release.Release)
            instance.version = '1.2.3'; instance.commit = 'a' * 40; instance.base = root
            state = {'version': instance.version, 'commit': instance.commit, 'files': {'artifact': release.digest(data)}}
            release.write(root / 'build.json', state)
            self.assertTrue(instance.completed('build'))
            data.write_bytes(b'changed'); self.assertFalse(instance.completed('build'))
            data.write_bytes(b'good'); instance.commit = 'b' * 40
            self.assertFalse(instance.completed('build'))

    def test_version_cannot_escape_staging_directory(self):
        for value in ('../escape', '1.2', 'v1.2.3', '1.2.3;echo bad'):
            with self.subTest(value=value), self.assertRaises(ValueError): release.Release(value)

    def test_platforms_and_every_client_are_included(self):
        self.assertEqual(set(release.TARGETS), {'osx_arm64', 'linux_arm64', 'linux_amd64'})
        self.assertEqual(set(release.COMPONENTS), {'extension', 'cli', 'native', 'python', 'javascript', 'java', 'rust', 'cpp', 'elixir'})

    def test_packages_require_build_receipts(self):
        instance = object.__new__(release.Release)
        instance.completed = lambda name: False
        with self.assertRaisesRegex(RuntimeError, 'release-build'): instance.package()


if __name__ == '__main__': unittest.main()
