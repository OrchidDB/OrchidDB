#!/usr/bin/env python3
"""Package locally built, pinned release sources; never dispatch a remote build.

Native code is compiled once by local_build.py. This command packages that same
code for all clients. Cross-target libraries are inspected without loading them.
Completed packaging steps are resumed only when their input hashes still match.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib

from local_build import outputs

PLATFORMS = {
    'linux-aarch64': ('aarch64-unknown-linux-gnu', 'liborchiddb_compiler.so', 'liborchiddb_java.so', 'linux_aarch64', 'linux-arm64'),
    'macos-aarch64': ('aarch64-apple-darwin', 'liborchiddb_compiler.dylib', 'liborchiddb_java.dylib', 'macosx_11_0_arm64', 'darwin-arm64'),
    'macos-x86_64': ('x86_64-apple-darwin', 'liborchiddb_compiler.dylib', 'liborchiddb_java.dylib', 'macosx_11_0_x86_64', 'darwin-x64'),
    'linux-x86_64': ('x86_64-unknown-linux-gnu', 'liborchiddb_compiler.so', 'liborchiddb_java.so', 'linux_x86_64', 'linux-x64'),
}
REPOS = {'engine': 'orchiddb', 'native': 'orchiddb-native', 'rust': 'orchiddb-rust', 'cli': 'orchiddb-cli', 'java': 'orchiddb-java', 'python': 'orchiddb-python', 'javascript': 'orchiddb-js', 'elixir': 'orchiddb-elixir', 'cpp': 'orchiddb-cpp'}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(command, cwd=None, env=None):
    print('+', ' '.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=cwd, env=env, check=True)


def capture(command, cwd=None):
    return subprocess.check_output(list(map(str, command)), cwd=cwd, text=True).strip()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def copy_source(source, destination):
    destination.mkdir(parents=True, exist_ok=True)
    data = subprocess.check_output(['git', 'archive', 'HEAD'], cwd=source)
    import io
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        archive.extractall(destination, filter='data')


def check_binary(path, platform, core=None):
    data = path.read_bytes()
    if platform.startswith('macos'):
        expected = 0x0100000c if platform.endswith('aarch64') else 0x01000007
        assert data[:4] == b'\xcf\xfa\xed\xfe' and struct.unpack_from('<I', data, 4)[0] == expected, f'Wrong Mach-O architecture: {path}'
    elif platform.startswith('linux'):
        assert data[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H', data, 18)[0] == (183 if platform.endswith('aarch64') else 62), f'Wrong ELF architecture: {path}'
    else:
        assert data[:2] == b'MZ', f'Expected PE: {path}'
        offset = struct.unpack_from('<I', data, 60)[0]
        assert data[offset:offset+4] == b'PE\0\0' and struct.unpack_from('<H', data, offset+4)[0] == 0x8664, f'Wrong PE architecture: {path}'
    if core:
        assert core.encode() in data, f'Pinned core revision is absent: {path}'
        assert (core + '-dirty').encode() not in data, f'Dirty release source: {path}'


class Package:
    def __init__(self, args):
        self.version = args.version
        self.workspace = args.workspace.resolve()
        self.release = self.workspace / '.releases' / self.version
        self.sources = self.release / 'local-source'
        self.target = args.target.resolve() if args.target else self.workspace / 'target/native-release'
        self.explicit_target = args.target is not None
        self.output = self.release / 'local-artifacts'
        self.work = self.release / 'local-package-work'
        self.revisions = {}
        for kind, name in REPOS.items():
            source = self.sources / name
            revision = capture(['git', 'rev-parse', 'HEAD'], source)
            assert revision == capture(['git', 'rev-parse', f'v{self.version}^{{commit}}'], source), f'{name}: source does not match release tag'
            assert not capture(['git', 'status', '--porcelain', '--untracked-files=no'], source), f'{name}: modified tracked source'
            self.revisions[kind] = revision
        self.core = self.revisions['engine']
        for kind in REPOS:
            self.dest(kind).mkdir(parents=True, exist_ok=True)
        self.work.mkdir(parents=True, exist_ok=True)

    def source(self, kind):
        return self.sources / REPOS[kind]

    def dest(self, kind):
        return self.output / kind

    def binary(self, platform, filename):
        if not self.explicit_target:
            return outputs(self.workspace, platform)[1]['native'].parent / filename
        triple = PLATFORMS[platform][0]
        directory = self.target / 'release' if platform == 'macos-aarch64' else self.target / triple / 'release'
        return directory / filename

    def staged(self, kind):
        destination = self.work / REPOS[kind]
        if destination.exists():
            shutil.rmtree(destination)
        copy_source(self.source(kind), destination)
        return destination

    def native(self):
        source = self.source('native')
        assert (source / 'CORE_REVISION').read_text().strip() == self.core
        for platform, (triple, filename, _, _, _) in PLATFORMS.items():
            binary = self.binary(platform, filename)
            check_binary(binary, platform, self.core)
            metadata = {'abi_version': 1, 'version': self.version, 'core_revision': self.core,
                        'target': triple, 'library': 'lib/' + filename, 'sha256': digest(binary)}
            manifest = self.work / ('native-' + platform + '.json')
            write_json(manifest, metadata)
            archive = self.dest('native') / f'orchiddb-compiler-v{self.version}-{triple}.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                output.add(binary, arcname=metadata['library'])
                output.add(source / 'include/orchiddb.h', arcname='include/orchiddb.h')
                output.add(source / 'LICENSE.md', arcname='LICENSE.md')
                output.add(manifest, arcname='manifest.json')

    def java(self):
        source = self.source('java')
        assert (source / 'native/CORE_REVISION').read_text().strip() == self.core
        for platform, (_, _, filename, _, _) in PLATFORMS.items():
            binary = self.binary(platform, filename)
            check_binary(binary, platform)
            jar = source / 'target/native-artifacts' / (platform + '.jar')
            run([sys.executable, self.workspace / 'orchiddb-java/scripts/package-native.py', '--source', source, '--platform', platform,
                 '--library', binary, '--version', self.version, '--output', jar])
        self.java_classes()
        run([sys.executable, self.workspace / 'orchiddb-java/scripts/package-github.py', '--source', source, '--output', self.dest('java')], source)

    def java_classes(self):
        source = self.source('java')
        stamp = self.work / 'java-classes.json'
        if stamp.exists():
            previous = json.loads(stamp.read_text())
            if previous['source_commit'] == self.revisions['java'] and all(
                    (source / name).is_file() and digest(source / name) == value
                    for name, value in previous['files'].items()):
                print('Reusing locally packaged JVM modules', flush=True)
                return
        run(['mvn', '-B', '-Pgremlin', '-DskipTests', 'package', 'dependency:copy-dependencies',
             '-DincludeScope=runtime', '-DoutputDirectory=target/runtime-deps'], source)
        files = {}
        for artifact in ['orchiddb-java', 'orchiddb-gremlin']:
            for path in (source / artifact / 'target').glob('*.jar'):
                files[str(path.relative_to(source))] = digest(path)
            for path in (source / artifact / 'target/runtime-deps').glob('*.jar'):
                files[str(path.relative_to(source))] = digest(path)
        assert files, 'Missing JVM packages'
        write_json(stamp, {'source_commit': self.revisions['java'], 'files': files})

    def python(self):
        interpreter = self.workspace / 'orchiddb-python/.venv/bin/python'
        # Reuse the local packaging environment; never rebuild the Rust library.
        run([interpreter, '-m', 'pip', 'install', 'setuptools>=77', 'wheel', 'build'])
        for platform, (_, filename, _, wheel_platform, _) in PLATFORMS.items():
            stage = self.staged('python')
            binary = self.binary(platform, filename)
            check_binary(binary, platform, self.core)
            native = stage / 'src/orchiddb/native'
            native.mkdir(parents=True, exist_ok=True)
            shutil.copy2(binary, native / filename)
            run([interpreter, 'setup.py', 'bdist_wheel', '--plat-name', wheel_platform,
                 '--dist-dir', self.dest('python')], stage)
        # Audit the actual ELF symbol requirements and bundle required libraries
        # locally before declaring a manylinux tag. Never just rename a wheel.
        for architecture, docker_arch in [('x86_64', 'amd64'), ('aarch64', 'arm64')]:
            linux_wheel = next(self.dest('python').glob(f'*-linux_{architecture}.whl'))
            run(['docker', 'run', '--rm', '--platform', 'linux/' + docker_arch,
                 '-v', str(self.dest('python')) + ':/dist', 'python:3.12-slim-bookworm@sha256:54c85f3c47607a77f32adec749d3c81d1348bf25833671f512b26a9b6d778cb3',
                 'sh', '-ec', 'apt-get update -qq && apt-get install -y -qq patchelf >/dev/null && '
                 'pip install -q auditwheel && auditwheel repair --plat manylinux_2_34_' + architecture +
                 ' --wheel-dir /dist/repaired /dist/' + linux_wheel.name])
            repaired = list((self.dest('python') / 'repaired').glob('*.whl'))
            assert len(repaired) == 1, 'Auditwheel did not produce exactly one Linux wheel'
            shutil.move(repaired[0], self.dest('python') / repaired[0].name)
            linux_wheel.unlink()
            shutil.rmtree(self.dest('python') / 'repaired')
        stage = self.staged('python')
        run([interpreter, '-m', 'build', '--sdist', '--outdir', self.dest('python')], stage)

    def javascript(self):
        stage = self.staged('javascript')
        modules = self.workspace / 'orchiddb-js/node_modules'
        if modules.is_dir():
            (stage / 'node_modules').symlink_to(modules)
        else:
            run(['npm', 'ci'], stage)
        for platform, (_, filename, _, _, js_platform) in PLATFORMS.items():
            binary = self.binary(platform, filename)
            check_binary(binary, platform, self.core)
            destination = stage / 'native' / js_platform
            destination.mkdir(parents=True, exist_ok=True)
            shutil.copy2(binary, destination / filename)
            write_json(destination / 'manifest.json', {'abi_version': 1, 'version': self.version,
                       'core_revision': self.core, 'sha256': digest(binary)})
        run(['npm', 'pack', '--pack-destination', self.dest('javascript')], stage)

    def cpp(self):
        source = self.source('cpp')
        json_source = self.workspace / 'orchiddb-cpp/build/_deps/json-src'
        for platform, (_, filename, _, _, _) in PLATFORMS.items():
            binary = self.binary(platform, filename)
            check_binary(binary, platform, self.core)
            build = self.work / ('cpp-' + platform)
            system = 'Linux' if platform.startswith('linux') else 'Darwin'
            processor = 'arm64' if platform.endswith('aarch64') else 'x86_64'
            command = ['cmake', '-S', source, '-B', build, '-DORCHIDDB_BUILD_TESTS=OFF',
                       '-DORCHIDDB_NATIVE_LIBRARY=' + str(binary), '-DCMAKE_SYSTEM_NAME=' + system,
                       '-DCMAKE_SYSTEM_PROCESSOR=' + processor]
            if json_source.is_dir():
                command.append('-DFETCHCONTENT_SOURCE_DIR_JSON=' + str(json_source))
            run(command)
            run(['cpack', '--config', build / 'CPackConfig.cmake', '-B', self.dest('cpp')])

    def cli(self):
        source = self.source('cli')
        for platform in PLATFORMS:
            binary = self.binary(platform, 'orchiddb')
            check_binary(binary, platform)
            metadata = binary.parent / 'BUILD.json'
            info = json.loads(metadata.read_text())
            assert info['version'] == self.version and info['platform'] == platform
            assert info['source_commit'] == self.revisions['cli'] and info['binary_sha256'] == digest(binary)
            archive = self.dest('cli') / f'orchiddb-v{self.version}-{platform}.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                for path in [binary, metadata, source / 'LICENSE.md', source / 'README.md', source / 'examples']:
                    output.add(path, arcname=path.name)

    def elixir(self):
        stage = self.staged('elixir')
        env = dict(os.environ)
        env['PATH'] = '/opt/homebrew/opt/erlang/bin:/opt/homebrew/opt/elixir/bin:' + env['PATH']
        deps = self.workspace / 'orchiddb-elixir/deps'
        if deps.is_dir():
            (stage / 'deps').symlink_to(deps)
        else:
            run(['mix', 'deps.get'], stage, env)
        run(['mix', 'hex.build', '--output', self.dest('elixir') / f'orchiddb-{self.version}.tar'], stage, env)

    def rust(self):
        # Package both crates together so Cargo resolves the unpublished engine
        # from its local workspace index. Verification already ran before tagging.
        root = self.work / 'crates'
        if root.exists():
            shutil.rmtree(root)
        root.mkdir()
        for kind in ['engine', 'rust']:
            copy_source(self.source(kind), root / REPOS[kind])
        manifest = root / 'orchiddb/Cargo.toml'
        manifest.write_text(re.sub(r'\n\[workspace\]\n.*?(?=\n\[)', '\n', manifest.read_text(), flags=re.S))
        manifest = root / 'orchiddb-rust/Cargo.toml'
        manifest.write_text(re.sub(r'^orchiddb = \{[^\n]+', 'orchiddb = { version = "=' + self.version + '", path = "../orchiddb", default-features = false }', manifest.read_text(), flags=re.M))
        shutil.copy2(self.source('engine') / 'Cargo.lock', root / 'Cargo.lock')
        (root / 'Cargo.toml').write_text('[workspace]\nresolver = "2"\nmembers = ["orchiddb", "orchiddb-rust"]\n')
        env = dict(os.environ, CARGO_TARGET_DIR=str(self.work / 'cargo-package'))
        run(['cargo', 'metadata', '--format-version', '1', '--no-deps'], root, env)
        run(['cargo', 'package', '--workspace', '--no-verify', '--registry', 'crates-io'], root, env)
        for kind, name in [('engine', 'orchiddb'), ('rust', 'orchiddb-client')]:
            filename = f'{name}-{self.version}.crate'
            shutil.copy2(self.work / 'cargo-package/package' / filename, self.dest(kind) / filename)

    def package_family(self, family):
        if family == 'java-classes':
            self.java_classes()
            return
        sources = dict(self.revisions)
        inputs = {}
        if family in ['native', 'java', 'python', 'javascript', 'cpp', 'cli']:
            for platform, (_, native, jni, _, _) in PLATFORMS.items():
                filename = jni if family == 'java' else 'orchiddb' if family == 'cli' else native
                path = self.binary(platform, filename)
                inputs[str(path)] = digest(path)
                if family == 'cli':
                    inputs[str(path.parent / 'BUILD.json')] = digest(path.parent / 'BUILD.json')
        signature = {'sources': sources, 'inputs': inputs, 'platforms': list(PLATFORMS),
                     'packager': digest(Path(__file__)),
                     'java_packager': digest(self.workspace / 'orchiddb-java/scripts/package-github.py') if family == 'java' else None}
        stamp = self.work / ('completed-' + family + '.json')
        if stamp.is_file():
            previous = json.loads(stamp.read_text())
            if previous['signature'] == signature and all(
                    (self.output / name).is_file() and digest(self.output / name) == value
                    for name, value in previous['files'].items()):
                print('Reusing completed local package:', family, flush=True)
                return
        getattr(self, family.replace('-', '_'))()
        self.finish()
        kinds = ['engine', 'rust'] if family == 'rust' else ([] if family == 'java-classes' else [family])
        files = {str(path.relative_to(self.output)): digest(path) for kind in kinds
                 for path in self.dest(kind).iterdir() if path.is_file()}
        write_json(stamp, {'signature': signature, 'files': files})

    def finish(self):
        for kind in REPOS:
            destination = self.dest(kind)
            if not any(destination.iterdir()):
                continue
            (destination / 'SOURCE_COMMIT').write_text(self.revisions[kind] + '\n')
            validation = self.release / 'validation.json'
            if validation.is_file():
                shutil.copy2(validation, destination / 'validation.json')
            files = {p.name: digest(p) for p in sorted(destination.iterdir()) if p.is_file() and p.name not in ['SHA256SUMS', 'local-provenance.json']}
            write_json(destination / 'local-provenance.json', {'version': self.version, 'build_location': 'local',
                       'source_commit': self.revisions[kind], 'core_revision': self.core,
                       'native_revision': self.revisions['native'], 'files': files})
            files['local-provenance.json'] = digest(destination / 'local-provenance.json')
            (destination / 'SHA256SUMS').write_text(''.join(f'{value}  {name}\n' for name, value in sorted(files.items())))

    def verify(self):
        expected = {'engine': ('.crate', 1), 'rust': ('.crate', 1), 'native': ('.tar.gz', len(PLATFORMS)),
                    'cli': ('.tar.gz', len(PLATFORMS)), 'java': ('.zip', 1), 'python': ('.whl', len(PLATFORMS)),
                    'javascript': ('.tgz', 1), 'elixir': ('.tar', 1), 'cpp': ('.tar.gz', len(PLATFORMS))}
        for kind, (suffix, count) in expected.items():
            destination = self.dest(kind)
            assert len(list(destination.glob('*' + suffix))) == count, f'{kind}: missing or unexpected package count'
            provenance = json.loads((destination / 'local-provenance.json').read_text())
            assert provenance['source_commit'] == self.revisions[kind]
            for line in (destination / 'SHA256SUMS').read_text().splitlines():
                expected_hash, name = line.split('  ', 1)
                assert digest(destination / name) == expected_hash, f'Checksum mismatch: {kind}/{name}'
            print('Verified local packages:', kind)
        write_json(self.release / 'local-verified.json', {'version': self.version, 'sources': self.revisions,
                   'artifacts': {kind: {p.name: digest(p) for p in sorted(self.dest(kind).iterdir()) if p.is_file()} for kind in REPOS}})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['package', 'verify'], nargs='?', default='package')
    parser.add_argument('--version', required=True)
    parser.add_argument('--workspace', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--target', type=Path)
    parser.add_argument('--only', choices=['native', 'java', 'python', 'javascript', 'cpp', 'cli', 'elixir', 'rust', 'java-classes'])
    args = parser.parse_args()
    release = Package(args)
    if args.command == 'verify':
        release.verify()
        return
    for family in ([args.only] if args.only else ['rust', 'elixir', 'native', 'java', 'python', 'javascript', 'cpp', 'cli']):
        release.package_family(family)
    if not args.only:
        release.verify()


if __name__ == '__main__':
    main()
