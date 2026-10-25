"""Verify the exact packaged bytes before publication; keep successful receipts."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, **kwargs):
    print('+', ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), check=True, cwd=ROOT, **kwargs)


def verify(assets):
    assets = Path(assets).resolve()
    manifest = json.loads((assets / 'release-manifest.json').read_text())
    checksums = dict(line.split('  ', 1)[::-1] for line in (assets / 'SHA256SUMS').read_text().splitlines())
    actual = {p.name: sha(p) for p in assets.iterdir() if p.is_file()}
    if set(actual) != set(checksums) | {'SHA256SUMS'} or any(actual.get(n) != h for n, h in checksums.items()):
        raise RuntimeError('Release assets differ from SHA256SUMS')
    if any(actual.get(n) != h for n, h in manifest['assets'].items()):
        raise RuntimeError('Release assets differ from manifest')
    required = {'extension', 'native', 'python', 'javascript', 'java', 'rust', 'cpp', 'elixir', 'go'}
    if set(manifest['components']) != required or any(not v for v in manifest['components'].values()):
        raise RuntimeError('Missing release clients')
    # Receipt also changes when validation code changes.
    inputs = {'assets': actual, 'verifier': sha(Path(__file__)),
              'smoke': sha(ROOT / 'scripts/release/platform_smoke.py')}
    receipt = assets.parent / 'verified.json'
    if receipt.exists() and json.loads(receipt.read_text()).get('inputs') == inputs:
        print('Reusing verified package receipts', flush=True)
        return
    work = assets.parent / 'verification'
    work.mkdir(exist_ok=True)
    extracted = {}
    for component, names in manifest['components'].items():
        for name in names:
            path = assets / name
            destination = work / name.replace('.', '_')
            destination.mkdir(exist_ok=True)
            marker = destination / '.release-extracted-sha256'
            if marker.exists() and marker.read_text() == actual[name]:
                extracted[name] = destination
                continue
            if name.endswith(('.tar.gz', '.tgz')):
                with tarfile.open(path) as archive:
                    archive.extractall(destination, filter='data')
            else:
                with zipfile.ZipFile(path) as archive:
                    if any(Path(n).is_absolute() or '..' in Path(n).parts for n in archive.namelist()):
                        raise RuntimeError('Unsafe archive path')
                    archive.extractall(destination)
            marker.write_text(actual[name])
            extracted[name] = destination
    env = dict(os.environ)
    env.pop('ORCHIDDB_NATIVE_LIBRARY', None)
    env.pop('PYTHONPATH', None)
    python = work / 'python/bin/python'
    if not python.exists(): run([sys.executable, '-m', 'venv', python.parent.parent])
    run([python, '-m', 'pip', 'install', 'duckdb==1.5.6', 'pyarrow==25.0.0', 'pytest', 'pytest-subtests'])
    spec = importlib.util.spec_from_file_location('extension_build', ROOT / 'extension/scripts/build.py')
    build = importlib.util.module_from_spec(spec); spec.loader.exec_module(build)
    for platform in manifest['platforms']:
        native_name = next(n for n in manifest['components']['native'] if platform in n)
        ext_name = next(n for n in manifest['components']['extension'] if platform in n)
        native = next(extracted[native_name].rglob('liborchiddb_compiler.*'))
        extension = next(extracted[ext_name].rglob('orchid.duckdb_extension'))
        args = [ROOT / 'scripts/release/platform_smoke.py', '--binaries', native.parent,
                '--extension', extension, '--version', manifest['version'],
                '--commit', manifest['binary_commits'][platform]]
        if platform == 'osx_arm64':
            run([python, *args], env=env)
        else:
            arch = platform.removeprefix('linux_')
            container = build.linux_container(arch, [ROOT.parent])
            # Reuse the pinned DuckDB wheel installed during the wrapper build.
            vendor = ROOT / 'extension/vendor' / ('python-duckdb-' + arch)
            code = 'import runpy,sys;sys.path.insert(0,sys.argv.pop(1));sys.argv=sys.argv[1:];runpy.run_path(sys.argv[0],run_name="__main__")'
            run([*container, '/opt/python/cp312-cp312/bin/python', '-c', code, vendor, *args, '--export-host-symbols'])
        # All packaged copies of the runtime must be byte-identical to the
        # native runtime we just executed on this architecture.
        matches = 0
        for directory in extracted.values():
            for library in directory.rglob(native.name):
                if sha(library) == sha(native): matches += 1
        if matches < 5:
            raise RuntimeError('Missing matching runtime copies for ' + platform)
    wheel = next(n for n in manifest['components']['python'] if 'macosx' in n)
    run([python, '-m', 'pip', 'install', '--force-reinstall', '--no-deps', assets / wheel])
    run([python, '-m', 'pytest', '-q', ROOT / 'clients/python/tests/test_integration.py'], env=env)
    js = work / 'javascript'; js.mkdir(exist_ok=True)
    package = {'private': True, 'type': 'module', 'dependencies': {
        '@orchiddb/client': 'file:' + str(assets / manifest['components']['javascript'][0]),
        '@duckdb/duckdb-wasm': '1.33.1-dev57.0', 'apache-arrow': '17.0.0'}}
    (js / 'package.json').write_text(json.dumps(package))
    for source, dest in [('js-smoke.mjs', 'smoke.mjs'), ('adapter.mjs', 'adapter.mjs')]:
        shutil.copy2(ROOT / 'tools/diagnostics/release' / source, js / dest)
    run(['npm', 'install', '--prefix', js, '--no-audit', '--no-fund'])
    run(['node', js / 'smoke.mjs'], env=env)
    receipt.write_text(json.dumps({'inputs': inputs, 'platforms': manifest['platforms'],
                                   'checks': ['archive integrity', 'native ABI', 'extension execution',
                                              'runtime copies', 'installed Python', 'installed JavaScript']}, indent=2))
    print('Verified all platform packages and installed Python/JavaScript clients', flush=True)
