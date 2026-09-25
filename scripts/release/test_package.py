"""Exercise the portable archive contract and failure paths without compiling Rust."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('release_package', Path(__file__).with_name('package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ReleasePackaging(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'Cargo.toml').write_text('[package]\nname="test-crate"\nversion="0.1.0"\n')
        (self.root / 'LICENSE.md').write_text('fixture license\n')
        self.binary = self.root / 'orchiddb'
        self.binary.write_bytes(b'release binary fixture\x00\x01')
        self.out = self.root / 'packages'
        for mocked in [patch.object(package, 'ROOT', self.root),
                       patch.object(package, 'smoke'),
                       patch.object(package, 'revision', return_value='a' * 40),
                       patch.object(package.subprocess, 'check_output', return_value='rustc test')]:
            mocked.start()
            self.addCleanup(mocked.stop)

    def make_archive(self, target):
        args = argparse.Namespace(tag='v0.1.0', target=target, binary=self.binary, output=self.out)
        with contextlib.redirect_stdout(io.StringIO()):
            package.archive(args)
        suffix = '.zip' if target.endswith('windows-msvc') else '.tar.gz'
        return self.out / f'orchiddb-v0.1.0-{target}{suffix}'

    def test_tar_contains_executable_license_and_accurate_provenance(self):
        archive = self.make_archive('aarch64-apple-darwin')
        with tarfile.open(archive) as stream:
            members = {Path(m.name).name: m for m in stream if m.isfile()}
            self.assertEqual(set(members), {'orchiddb', 'LICENSE.md', 'README.txt', 'BUILD.json'})
            self.assertTrue(members['orchiddb'].mode & 0o111)
            info = json.load(stream.extractfile(members['BUILD.json']))
            self.assertEqual(info['binary_sha256'], package.digest(self.binary))
            self.assertEqual(info['cargo_package'], 'test-crate')
            self.assertEqual(stream.extractfile(members['LICENSE.md']).read(), b'fixture license\n')

    def test_windows_zip_preserves_binary_bytes_and_exe_name(self):
        archive = self.make_archive('x86_64-pc-windows-msvc')
        with zipfile.ZipFile(archive) as stream:
            names = stream.namelist()
            executable = next(n for n in names if n.endswith('/orchiddb.exe'))
            self.assertEqual(stream.read(executable), self.binary.read_bytes())
            self.assertTrue(all('..' not in Path(n).parts and not Path(n).is_absolute() for n in names))

    def test_verify_requires_every_target_and_rejects_tampering(self):
        first = self.make_archive('aarch64-apple-darwin')
        args = argparse.Namespace(tag='v0.1.0', directory=self.out)
        with self.assertRaisesRegex(ValueError, 'four targets'):
            package.verify(args)
        for target in package.TARGETS - {'aarch64-apple-darwin'}:
            self.make_archive(target)
        import mock_clients
        mock_clients.generate(self.out)
        with contextlib.redirect_stdout(io.StringIO()):
            package.verify(args)
        self.assertEqual(len((self.out / 'SHA256SUMS').read_text().splitlines()), 7)
        first.write_bytes(b'tampered')
        with self.assertRaisesRegex(ValueError, 'Checksum mismatch'):
            package.verify(args)

    def test_tag_must_match_manifest_and_safe_format(self):
        for tag in ['v0.2.0', 'main', '../v0.1.0', 'v0.1.0\ncommit=bad']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                package.package_metadata(tag)

    def test_tag_must_point_at_checkout(self):
        args = argparse.Namespace(tag='v0.1.0', github_output=None)
        with patch.object(package, 'revision', side_effect=['a' * 40, 'b' * 40]):
            with self.assertRaisesRegex(ValueError, 'checkout'):
                package.metadata(args)


if __name__ == '__main__':
    unittest.main()
