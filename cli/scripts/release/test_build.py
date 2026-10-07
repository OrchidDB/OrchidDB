import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('release_build', Path(__file__).with_name('build.py'))
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class ArchiveTests(unittest.TestCase):
    def test_checksum_required_before_extracting(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'archive.zip'
            archive.write_bytes(b'untrusted')
            destination = Path(directory) / 'output'
            with self.assertRaisesRegex(ValueError, 'checksum'):
                build.unpack(archive, 'incorrect', destination)
            self.assertFalse(destination.exists())

    def test_flat_static_archive_required(self):
        for name, accepted in [('libduckdb_static.a', True), ('../escape.a', False), ('nested/library.a', False), ('libduckdb.so', False)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                archive = Path(directory) / 'archive.zip'
                with zipfile.ZipFile(archive, 'w') as stream:
                    stream.writestr(name, b'fixture')
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                if accepted:
                    build.unpack(archive, digest, Path(directory) / 'out')
                else:
                    with self.assertRaisesRegex(ValueError, 'Unexpected'):
                        build.unpack(archive, digest, Path(directory) / 'out')

    def test_archive_must_contain_main_library(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'archive.zip'
            with zipfile.ZipFile(archive, 'w') as stream:
                stream.writestr('duckdb.h', b'header')
            with self.assertRaisesRegex(ValueError, 'missing'):
                build.unpack(archive, hashlib.sha256(archive.read_bytes()).hexdigest(), Path(directory) / 'out')


if __name__ == '__main__':
    unittest.main()
