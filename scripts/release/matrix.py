#!/usr/bin/env python3
"""Local parallel three-platform extension builds and resumable artifacts."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

from package_extension import ROOT, package
sys.path.insert(0, str(ROOT / 'extension/scripts'))
from build import download

# These are DuckDB platform identifiers, not Cargo target triples.
PLATFORMS = {
    'osx_arm64': None,
    'linux_arm64': 'aarch64-unknown-linux-gnu',
    'linux_amd64': 'x86_64-unknown-linux-gnu',
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text()) if path.exists() else {}


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.pending')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def environment():
    env = dict(os.environ)
    # Reuse the workspace's existing local cross-compilation tools when present.
    tooling = ROOT.parent / '.releases/tooling/bin'
    env['PATH'] = str(tooling) + os.pathsep + env.get('PATH', '')
    env.setdefault('CARGO_BUILD_JOBS', '3')
    return env


def run(command, env=None):
    command = list(map(str, command))
    print('+ ' + ' '.join(command), flush=True)
    subprocess.run(command, cwd=ROOT, env=env or environment(), check=True)


def revision():
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()


def location(version, commit, root=ROOT):
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version) or len(version) > 31:
        raise SystemExit('Set VERSION, e.g. VERSION=0.4.0')
    return root / 'target/releases' / f'extension-v{version}' / commit


def prerequisites():
    if (platform.system(), platform.machine()) != ('Darwin', 'arm64'):
        raise SystemExit('The complete local release requires a macOS ARM64 host')
    for tool in ('cargo', 'rustup', 'cargo-zigbuild', 'zig'):
        if not shutil.which(tool, path=environment()['PATH']):
            raise SystemExit(f'Missing release tool: {tool}; see README release prerequisites')


def reusable(build, version, commit, target):
    artifact = build / 'orchid.duckdb_extension'
    record = read(build / 'build-manifest.json')
    return (artifact.is_file() and record.get('revision') == commit
            and record.get('working_tree_modified') is False
            and record.get('extension_version') == version and record.get('platform') == target
            and record.get('profile') == 'release' and record.get('reused_rust') is False
            and record.get('sha256') == digest(artifact))


def build_one(version, commit, target):
    build = location(version, commit) / target
    if reusable(build, version, commit, target):
        print(f'Reusing completed build: {target}', flush=True)
        return
    build.mkdir(parents=True, exist_ok=True)
    if target == 'osx_arm64':
        env = dict(environment(), ORCHID_BUILD_DIR=str(build))
        run([sys.executable, 'extension/scripts/build.py', '--release', '--skip-load-check', '--extension-version', version], env)
    else:
        triple = PLATFORMS[target]
        cache = ROOT / 'target' / f'release-{target}'
        env = dict(environment(), CARGO_TARGET_DIR=str(cache), ORCHID_BUILD_DIR=str(build),
                   ORCHID_VENDOR_DIR=str(ROOT / 'extension/vendor'))
        # Homebrew cargo/rustc may shadow rustup and lack its cross-target std.
        rustc = subprocess.check_output(['rustup', 'which', 'rustc'], cwd=ROOT, text=True).strip()
        env['PATH'] = str(Path(rustc).parent) + os.pathsep + env['PATH']
        run(['rustup', 'target', 'add', triple], env)
        rust = ['cargo', 'zigbuild', '--locked', '--release', '--manifest-path',
                'extension/compiler/Cargo.toml', '--target', triple + '.2.28']
        run(rust, env)
        run([sys.executable, 'extension/scripts/build.py', '--release', '--skip-rust',
             '--skip-load-check', '--extension-version', version, '--duckdb-platform', target,
             '--rust-target', triple], env)
        # --skip-rust is only for the wrapper step: cargo-zigbuild just built
        # this target above as part of the same release build.
        record = read(build / 'build-manifest.json')
        record.update(reused_rust=False, rust_build=rust)
        write(build / 'build-manifest.json', record)
    if not reusable(build, version, commit, target):
        raise SystemExit(f'{target}: build provenance/platform mismatch or checkout changed during the build')


def validated_packages(version, commit, root=ROOT):
    directories = []
    for target in PLATFORMS:
        build = location(version, commit, root) / target
        if not reusable(build, version, commit, target):
            raise SystemExit(f'Missing current build for {target}; all three platforms are required')
        validation = read(build / 'validation.json')
        if (validation.get('revision') != commit or validation.get('sha256') != digest(build / 'orchid.duckdb_extension')
                or validation.get('platform') != target):
            raise SystemExit(f'Missing validation for {target}; run the platform checks before publication')
        directories.append(package(root, build))
    return directories


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['check', 'build', 'package'])
    parser.add_argument('--version', required=True)
    parser.add_argument('--platform', choices=PLATFORMS, help='retry one build target')
    args = parser.parse_args()
    commit = revision()
    location(args.version, commit)
    if args.stage == 'package':
        for directory in validated_packages(args.version, commit):
            print(directory)
        return
    prerequisites()
    if args.stage == 'check':
        return
    prepare_sources()
    targets = [args.platform] if args.platform else list(PLATFORMS)
    parallel_build(lambda target: build_one(args.version, commit, target), targets)


def prepare_sources():
    vendor = ROOT / 'extension/vendor'
    download(f'https://github.com/duckdb/duckdb/archive/refs/tags/v1.5.6.tar.gz',
             vendor / 'duckdb-v1.5.6.tar.gz',
             '1fadcbe9e69e1470f9093b6bcde08daf477d729c449e59a807f45c346622099b')
    download('https://raw.githubusercontent.com/nlohmann/json/v3.12.0/single_include/nlohmann/json.hpp',
             vendor / 'json.hpp',
             'aaf127c04cb31c406e5b04a63f1ae89369fccde6d8fa7cdda1ed4f32dfc5de63')
    source = vendor / 'duckdb-1.5.6'
    if not source.exists():
        import tarfile
        with tarfile.open(vendor / 'duckdb-v1.5.6.tar.gz') as archive:
            archive.extractall(vendor, filter='data')

def parallel_build(build_target, targets):
    workers = int(os.environ.get('RELEASE_JOBS', '3'))
    if workers < 1:
        raise SystemExit('RELEASE_JOBS must be positive')
    with concurrent.futures.ThreadPoolExecutor(max_workers=min(workers, len(targets))) as pool:
        futures = [pool.submit(build_target, target) for target in targets]
        for future in concurrent.futures.as_completed(futures):
            future.result()


if __name__ == '__main__':
    main()
