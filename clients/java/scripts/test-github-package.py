"""Local packaging checks using staged fixture JARs; never build native code."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPTS = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), SCRIPTS / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package_github = load('package-github')
package_native = load('package-native')


class GitHubPackageTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        for folder in ['native', 'scripts', 'docs', 'target/native-artifacts']:
            (self.root / folder).mkdir(parents=True)
        (self.root / 'pom.xml').write_text('''<project xmlns="http://maven.apache.org/POM/4.0.0">
          <modelVersion>4.0.0</modelVersion><groupId>com.orchiddb</groupId>
          <artifactId>orchiddb-parent</artifactId><version>1.2.3</version>
          <properties><project.build.outputTimestamp>2026-09-25T00:00:00Z</project.build.outputTimestamp></properties>
          </project>''')
        (self.root / 'native/CORE_REVISION').write_text('a' * 40 + '\n')
        (self.root / 'LICENSE.md').write_text('Fixture license\n')
        (self.root / 'README.md').write_text('Fixture API documentation\n')
        (self.root / 'docs/arrow.md').write_text('Arrow usage\n')
        for script in ['package-native.py', 'verify-native-artifacts.py']:
            shutil.copyfile(SCRIPTS / script, self.root / 'scripts' / script)
        for module in ['orchiddb-java', 'orchiddb-gremlin']:
            path = self.root / module
            (path / 'target/runtime-deps').mkdir(parents=True)
            (path / 'pom.xml').write_text('<project/><!-- ' + module + ' -->')
            for suffix in ['', '-sources', '-javadoc']:
                with zipfile.ZipFile(path / f'target/{module}-1.2.3{suffix}.jar', 'w') as jar:
                    jar.writestr('fixture.txt', module + suffix)
            (path / 'target/runtime-deps/shared-1.0.jar').write_bytes(b'shared dependency')
        shutil.copyfile(self.root / 'orchiddb-java/target/orchiddb-java-1.2.3.jar',
                        self.root / 'orchiddb-gremlin/target/runtime-deps/orchiddb-java-1.2.3.jar')
        library = self.root / 'fixture-native'
        library.write_bytes(b'fixture only, not a real native binary')
        with patch.object(package_native, 'ROOT', self.root):
            for platform in package_github.PLATFORMS:
                package_native.package(platform, library, '1.2.3',
                                       self.root / f'target/native-artifacts/{platform}.jar')

    def package(self, output='dist'):
        with patch.object(package_github, 'ROOT', self.root), \
             patch.object(package_github.subprocess, 'check_output', return_value='b' * 40 + '\n'):
            return package_github.package(self.root / output)

    def test_complete_repository_classpath_checksums_and_reproducible_archive(self):
        archive = self.package()
        self.assertEqual(archive.read_bytes(), self.package('dist-second').read_bytes())
        with zipfile.ZipFile(archive) as jar:
            names = set(jar.namelist())
            manifest = json.loads(jar.read('release-manifest.json'))
            self.assertEqual(set(manifest['files']), names - {'release-manifest.json'})
            self.assertEqual(manifest['commit'], 'b' * 40)
            self.assertEqual(manifest['core_revision'], 'a' * 40)
            self.assertEqual(manifest['distribution'], 'github')
            for name, digest in manifest['files'].items():
                self.assertEqual(digest, hashlib.sha256(jar.read(name)).hexdigest())
            for module in ['orchiddb-java', 'orchiddb-gremlin']:
                base = f'repository/com/orchiddb/{module}/1.2.3/{module}-1.2.3'
                for extension in ['.pom', '.jar', '-sources.jar', '-javadoc.jar']:
                    self.assertIn(base + extension, names)
                    data = jar.read(base + extension)
                    for algorithm in ['sha1', 'sha256']:
                        self.assertEqual(jar.read(base + extension + '.' + algorithm).decode().strip(),
                                         hashlib.new(algorithm, data).hexdigest())
                self.assertIn(f'lib/{module}-1.2.3.jar', names)
            self.assertIn('repository/com/orchiddb/orchiddb-parent/1.2.3/orchiddb-parent-1.2.3.pom', names)
            for platform in package_github.PLATFORMS:
                filename = f'orchiddb-java-1.2.3-{platform}.jar'
                self.assertIn('lib/' + filename, names)
                self.assertIn('repository/com/orchiddb/orchiddb-java/1.2.3/' + filename, names)
            self.assertFalse(any('windows' in name for name in names))
            self.assertIn('lib/shared-1.0.jar', names)
            self.assertIn('README.md', names)
            self.assertIn('docs/arrow.md', names)
            self.assertIn(b'com/orchiddb subtree', jar.read('README.txt'))
        checksum, filename = (archive.parent / 'SHA256SUMS').read_text().split()
        self.assertEqual(filename, archive.name)
        self.assertEqual(checksum, hashlib.sha256(archive.read_bytes()).hexdigest())

    def test_missing_classifier_is_rejected(self):
        (self.root / 'target/native-artifacts/linux-x86_64.jar').unlink()
        with self.assertRaises(subprocess.CalledProcessError):
            self.package()
        self.assertFalse((self.root / 'dist').exists())

    def test_missing_linux_arm64_classifier_is_rejected(self):
        (self.root / 'target/native-artifacts/linux-aarch64.jar').unlink()
        with self.assertRaises(subprocess.CalledProcessError):
            self.package()
        self.assertFalse((self.root / 'dist').exists())

    def test_conflicting_runtime_filename_is_rejected(self):
        (self.root / 'orchiddb-gremlin/target/runtime-deps/shared-1.0.jar').write_bytes(b'incompatible dependency')
        with self.assertRaisesRegex(ValueError, 'Conflicting distribution file'):
            self.package()

    def test_missing_runtime_dependencies_are_rejected(self):
        (self.root / 'orchiddb-java/target/runtime-deps/shared-1.0.jar').unlink()
        with self.assertRaisesRegex(ValueError, 'Missing runtime dependencies for orchiddb-java'):
            self.package()


if __name__ == '__main__':
    unittest.main()
