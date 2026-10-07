#!/usr/bin/env python3
"""Build a complete, resumable GitHub release locally. Never upload or publish."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

import matrix
from package_extension import package as package_extension
from publish_extension import bundle as extension_bundle

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    'osx_arm64': ('aarch64-apple-darwin', 'macos-aarch64', 'darwin-arm64', 'macosx_11_0_arm64'),
    'linux_arm64': ('aarch64-unknown-linux-gnu', 'linux-aarch64', 'linux-arm64', 'manylinux_2_28_aarch64'),
    'linux_amd64': ('x86_64-unknown-linux-gnu', 'linux-x86_64', 'linux-x64', 'manylinux_2_28_x86_64'),
}
COMPONENTS = ('extension', 'cli', 'native', 'python', 'javascript', 'java', 'rust', 'cpp', 'elixir')


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args, *, cwd=ROOT, env=None, capture=False):
    args = list(map(str, args))
    print('+ ' + ' '.join(args), flush=True)
    return subprocess.run(args, cwd=cwd, env=env or matrix.environment(), check=True,
                          text=True, stdout=subprocess.PIPE if capture else None).stdout


def write(path, data):
    matrix.write(Path(path), data)


def read(path):
    return matrix.read(Path(path))


def archive(path, entries):
    """Stable, safe tar archives; executable modes are retained."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as gz:
        with tarfile.open(fileobj=gz, mode='w') as tar:
            for name, source in sorted(entries.items()):
                if Path(name).is_absolute() or '..' in Path(name).parts:
                    raise ValueError('Unsafe archive member: ' + name)
                data = source if isinstance(source, bytes) else Path(source).read_bytes()
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if not isinstance(source, bytes) and os.access(source, os.X_OK) else 0o644
                tar.addfile(info, io.BytesIO(data))


def source_files(relative):
    names = subprocess.check_output(['git', 'ls-files', '-z', '--', str(relative)], cwd=ROOT).decode().split('\0')
    base = ROOT / relative
    return {str((ROOT / name).relative_to(base)): ROOT / name for name in names if name and (ROOT / name).is_file()}


class Release:
    def __init__(self, version):
        if len(version) > 31 or not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version):
            raise ValueError('VERSION must be a semantic version')
        self.version = version
        self.commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        self.base = ROOT / 'target/releases' / f'orchiddb-v{version}' / self.commit
        self.assets = self.base / 'upload'
        self.env = matrix.environment()
        self.env['CARGO_TARGET_DIR'] = str(ROOT / 'target')
        self.env.pop('ORCHID_BUILD_DIR', None)
        self.env.pop('DUCKDB_LIB_DIR', None)
        self.env.pop('DUCKDB_INCLUDE_DIR', None)
        erlang = Path('/opt/homebrew/opt/erlang/bin')
        if erlang.is_dir(): self.env['PATH'] = str(erlang) + ':' + self.env['PATH']
        self.python = os.path.abspath(os.environ.get('RELEASE_PYTHON', ROOT / 'target/release-env/bin/python'))

    def check(self):
        if subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT):
            raise RuntimeError('Commit the source changes before a release; the release must identify one clean commit')
        versions = {}
        for name in ('Cargo.toml', 'cli/Cargo.toml', 'clients/rust/Cargo.toml', 'clients/native/Cargo.toml', 'clients/java/native/Cargo.toml', 'clients/python/pyproject.toml'):
            config = tomllib.loads((ROOT / name).read_text())
            versions[name] = (config.get('package') or config['project'])['version']
        versions['clients/js/package.json'] = read(ROOT / 'clients/js/package.json')['version']
        for name, pattern in [('clients/java/pom.xml', r'<version>([^<]+)</version>'),
                              ('clients/elixir/mix.exs', r'version: "([^"]+)"'),
                              ('clients/cpp/CMakeLists.txt', r'project\(OrchidDB VERSION ([^ ]+)')]:
            versions[name] = re.search(pattern, (ROOT / name).read_text())[1]
        wrong = {name: value for name, value in versions.items() if value != self.version}
        if wrong:
            raise RuntimeError(f'Update and commit component versions to {self.version} first: {wrong}')
        matrix.prerequisites()
        for tool in ('npm', 'node', 'mvn', 'java', 'cmake', 'c++'):
            if not shutil.which(tool, path=self.env['PATH']):
                raise RuntimeError('Missing release prerequisite: ' + tool)
        if not Path(self.python).is_file():
            raise RuntimeError('Run make release-env to prepare the packaging environment')
        run([self.python, '-c', 'import build, wheel, setuptools'], env=self.env)
        run([sys.executable, 'scripts/clients.py', 'metadata'], env=self.env)
        self.base.mkdir(parents=True, exist_ok=True)

    def stamp(self, stage, files):
        if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() != self.commit or subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT):
            raise RuntimeError('Source changed during the release')
        write(self.base / (stage + '.json'), {'version': self.version, 'commit': self.commit,
              'files': {str(Path(p).relative_to(ROOT)): digest(p) for p in files}})

    def completed(self, stage):
        state = read(self.base / (stage + '.json'))
        return state.get('version') == self.version and state.get('commit') == self.commit and bool(state.get('files')) and all(
            (ROOT / name).is_file() and digest(ROOT / name) == sha for name, sha in state['files'].items())

    def build(self):
        if self.completed("build"):
            print("Reusing completed builds for this commit")
            return
        matrix.prepare_sources()
        matrix.parallel_build(self.build_platform, list(TARGETS))
        run(['npm', 'ci'], cwd=ROOT / 'clients/js', env=self.env)
        run(['npm', 'run', 'build'], cwd=ROOT / 'clients/js', env=self.env)
        java_env = dict(self.env)
        if not java_env.get('JAVA_HOME'):
            java_env['JAVA_HOME'] = subprocess.check_output(['/usr/libexec/java_home', '-v', '21'], text=True).strip()
        for module in ('jvm-codecs', 'jvm'):
            run(['mvn', '-q', '-f', module + '/pom.xml', '-DskipTests', 'install', 'dependency:build-classpath', '-Dmdep.outputFile=target/classpath.txt'], env=java_env)
        run(['mvn', '-Pgremlin', '-DskipTests', 'package', 'dependency:copy-dependencies', '-DincludeScope=runtime', '-DoutputDirectory=target/runtime-deps'], cwd=ROOT / 'clients/java', env=java_env)
        inputs = [ROOT / path for t in TARGETS for path in read(self.base / ('build-' + t + '.json'))['files']]
        self.stamp('build', inputs + list((ROOT / 'clients/js/dist').glob('*')) + list((ROOT / 'clients/java').glob('*/target/*.jar')) + list((ROOT / 'clients/java').glob('*/target/runtime-deps/*.jar')) + [ROOT / 'jvm/target/orchiddb-jvm-0.1.0.jar', ROOT / 'jvm/target/classpath.txt'])

    def build_platform(self, target):
        triple, classifier, _, _ = TARGETS[target]
        stage = 'build-' + target
        if self.completed(stage):
            print('Reusing completed client binaries:', target)
            return
        env = dict(self.env)
        env['DUCKDB_DOWNLOAD_LIB'] = '1'
        env['DUCKDB_STATIC'] = '0'
        command = ['cargo', 'build' if target == 'osx_arm64' else 'zigbuild', '--locked', '--release']
        cache = ROOT / 'target' if target == 'osx_arm64' else ROOT / 'target' / ('release-' + target)
        env['CARGO_TARGET_DIR'] = str(cache)
        if target == 'osx_arm64': env['MACOSX_DEPLOYMENT_TARGET'] = '11.0'
        if target != 'osx_arm64':
            run(['rustup', 'target', 'add', triple], env=env)
            command += ['--target', triple + '.2.28']
            rustc = subprocess.check_output(['rustup', 'which', 'rustc'], text=True).strip()
            env['PATH'] = str(Path(rustc).parent) + ':' + env['PATH']
        for package in ('orchid-duckdb-compiler', 'orchiddb-compiler-native', 'orchiddb-java-native', 'orchiddb-cli'):
            command += ['-p', package]
        run(command, env=env)
        extension_build = matrix.location(self.version, self.commit) / target
        extension_env = dict(env, ORCHID_BUILD_DIR=str(extension_build))
        wrapper = [sys.executable, 'extension/scripts/build.py', '--release', '--skip-rust',
                   '--skip-load-check', '--extension-version', self.version, '--duckdb-platform', target]
        if target != 'osx_arm64':
            wrapper += ['--rust-target', triple]
        run(wrapper, env=extension_env)
        manifest = read(extension_build / 'build-manifest.json')
        manifest.update(reused_rust=False, rust_build=command)
        write(extension_build / 'build-manifest.json', manifest)
        binaries = cache / ('' if target == 'osx_arm64' else triple) / 'release'
        out = self.base / 'binaries' / target
        out.mkdir(parents=True, exist_ok=True)
        suffix = 'dylib' if target == 'osx_arm64' else 'so'
        for name in ('orchiddb', 'liborchiddb_compiler.' + suffix, 'liborchiddb_java.' + suffix):
            shutil.copy2(binaries / name, out / name)
        run([sys.executable, 'clients/java/scripts/package-native.py', '--platform', classifier, '--library', out / ('liborchiddb_java.' + suffix), '--version', self.version,
             '--output', ROOT / 'clients/java/target/native-artifacts' / (classifier + '.jar')], env=env)
        self.stamp(stage, list(out.iterdir()) + [ROOT / 'clients/java/target/native-artifacts' / (classifier + '.jar'), matrix.location(self.version, self.commit) / target / 'orchid.duckdb_extension', matrix.location(self.version, self.commit) / target / 'build-manifest.json'])

    def package(self):
        if not self.completed('build'):
            raise RuntimeError('Complete release-build before packaging')
        if self.completed('package'):
            print('Reusing complete packages:', self.assets)
            return
        self.assets.mkdir(parents=True, exist_ok=True)
        if any(self.assets.iterdir()):
            raise RuntimeError('Incomplete upload directory exists; preserve it and retry with a new empty staging directory')
        with tempfile.TemporaryDirectory(dir=self.base, prefix='package-') as temporary:
            stage = Path(temporary)
            output = stage / 'upload'; output.mkdir()
            inventory = {name: [] for name in COMPONENTS}
            def add(component, name, entries):
                archive(output / name, entries)
                inventory[component].append(name)
            for target, (triple, classifier, node_platform, wheel_platform) in TARGETS.items():
                binaries = self.base / 'binaries' / target
                library = binaries / ('liborchiddb_compiler.' + ('dylib' if target == 'osx_arm64' else 'so'))
                metadata = json.dumps({'version': self.version, 'commit': self.commit, 'target': triple, 'abi_version': 2}, indent=2).encode()
                common = {'LICENSE.md': ROOT / 'LICENSE.md', 'manifest.json': metadata}
                add('cli', f'orchiddb-cli-{self.version}-{target}.tar.gz', {**common, 'README.txt': b'Install the DuckDB 1.5.2 shared library separately and make it available to the system dynamic loader (see README.md). Extract this archive and run bin/orchiddb query --file examples/people.cypher --schema examples/people.json --init examples/setup.sql --no-iceberg --format table.\n', 'bin/orchiddb': binaries / 'orchiddb', 'README.md': ROOT / 'cli/README.md', **{'examples/' + k: v for k, v in source_files(Path('cli/examples')).items()}})
                add('native', f'orchiddb-native-{self.version}-{target}.tar.gz', {**common, 'lib/' + library.name: library, 'include/orchiddb.h': ROOT / 'clients/native/include/orchiddb.h'})
                cpp = stage / ('cpp-' + target)
                run(['cmake', '-S', 'clients/cpp', '-B', cpp / 'build', '-DCMAKE_INSTALL_PREFIX=' + str(cpp / 'install'), '-DCMAKE_DISABLE_FIND_PACKAGE_nlohmann_json=TRUE', '-DORCHIDDB_NATIVE_LIBRARY=' + str(library)], env=self.env)
                run(['cmake', '--install', cpp / 'build'], env=self.env)
                add('cpp', f'orchiddb-cpp-{self.version}-{target}.tar.gz', {**common, 'README.md': ROOT / 'clients/cpp/README.md', **{'examples/' + k: v for k, v in source_files(Path('clients/cpp/examples')).items()}, **{str(p.relative_to(cpp / 'install')): p for p in (cpp / 'install').rglob('*') if p.is_file()}})
                python = stage / ('python-' + target)
                for name, path in source_files(Path('clients/python')).items():
                    destination = python / name; destination.parent.mkdir(parents=True, exist_ok=True); shutil.copy2(path, destination)
                native = python / 'src/orchiddb/native'; native.mkdir(exist_ok=True)
                shutil.copy2(library, native / library.name)
                (python / 'src/orchiddb/CORE_REVISION').write_text(self.commit + '\n')
                run([self.python, 'setup.py', 'bdist_wheel', '--plat-name', wheel_platform, '--dist-dir', output], cwd=python, env=self.env)
                inventory['python'] += [p.name for p in output.glob('*' + wheel_platform + '.whl')]
                elixir = source_files(Path('clients/elixir'))
                elixir.update({'CORE_REVISION': (self.commit + '\n').encode(), 'priv/native/' + library.name: library,
                              'RELEASE.txt': b'Use a Mix path dependency on this extracted directory. The NIF builds for your installed Erlang. Set ORCHIDDB_NATIVE_LIBRARY to priv/native/liborchiddb_compiler.so (Linux) or .dylib (macOS).\n'})
                add('elixir', f'orchiddb-elixir-{self.version}-{target}.tar.gz', elixir)
                packaged = package_extension(ROOT, matrix.location(self.version, self.commit) / target)
                tar, _, _ = extension_bundle(packaged, self.version, self.commit)
                shutil.copy2(tar, output / tar.name); inventory['extension'].append(tar.name)
            node = stage / 'node'; node.mkdir()
            for name, path in source_files(Path('clients/js')).items():
                destination = node / name; destination.parent.mkdir(parents=True, exist_ok=True); shutil.copy2(path, destination)
            shutil.copytree(ROOT / 'clients/js/dist', node / 'dist')
            for target, (_, _, node_platform, _) in TARGETS.items():
                destination = node / 'native' / node_platform; destination.mkdir(parents=True)
                library = next((self.base / 'binaries' / target).glob('liborchiddb_compiler.*'))
                shutil.copy2(library, destination / library.name)
                write(destination / 'manifest.json', {'version': self.version, 'core_revision': self.commit, 'abi_version': 2, 'sha256': digest(library)})
            run(['npm', 'pack', '--ignore-scripts', '--pack-destination', output], cwd=node, env=self.env)
            inventory['javascript'] = [p.name for p in output.glob('*.tgz')]
            run([sys.executable, 'clients/java/scripts/package-github.py', '--output', output, '--platforms', *[v[1] for v in TARGETS.values()]], env=self.env)
            inventory['java'] = [p.name for p in output.glob('orchiddb-java-*.zip')]
            kernels = output / f'orchiddb-jvm-kernels-{self.version}.zip'
            kernel_files = {'lib/orchiddb-jvm-0.1.0.jar': ROOT / 'jvm/target/orchiddb-jvm-0.1.0.jar', 'LICENSE.md': ROOT / 'LICENSE.md'}
            for dependency in (ROOT / 'jvm/target/classpath.txt').read_text().strip().split(os.pathsep):
                artifact = Path(dependency)
                if not artifact.is_file(): raise RuntimeError('Missing JVM runtime dependency: ' + dependency)
                key = 'lib/' + artifact.name
                if key in kernel_files and digest(kernel_files[key]) != digest(artifact): raise RuntimeError('Conflicting JVM runtime JAR: ' + key)
                kernel_files[key] = artifact
            with zipfile.ZipFile(kernels, 'w', zipfile.ZIP_DEFLATED) as zipped:
                for name, artifact in sorted(kernel_files.items()):
                    info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
                    zipped.writestr(info, artifact.read_bytes())
                zipped.writestr('README.txt', 'Optional JVM graph kernels. Use Java 21 and set ORCHIDDB_JVM_CLASSPATH to the absolute path of lib/* in this extracted directory. Third-party dependencies are included.\n')
            inventory['native'].append(kernels.name)
            add('rust', f'orchiddb-source-{self.version}.tar.gz', source_files(Path('.')))
            if any(not assets for assets in inventory.values()): raise RuntimeError('Missing client release assets')
            (output / 'SHA256SUMS').unlink(missing_ok=True)
            write(output / 'release-manifest.json', {'version': self.version, 'commit': self.commit, 'platforms': list(TARGETS), 'components': inventory,
                  'assets': {p.name: digest(p) for p in sorted(output.iterdir()) if p.is_file()}})
            (output / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in sorted(output.iterdir()) if p.is_file()))
            self.assets.rmdir()
            output.rename(self.assets)
        self.stamp('package', list(self.assets.iterdir()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['check', 'build', 'package', 'all'])
    parser.add_argument('--version', required=True)
    args = parser.parse_args()
    release = Release(args.version)
    release.check()
    if args.stage == 'all':
        release.build(); release.package()
        print('Upload every file in:', release.assets)
    elif args.stage != 'check':
        getattr(release, args.stage)()


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f'release: {error}', file=sys.stderr)
        sys.exit(1)
