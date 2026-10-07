#!/usr/bin/env python3
"""Build a standalone CLI with checksum-pinned official DuckDB static libraries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tomllib
import urllib.request
import zipfile

DRIVER_VERSION = '1.5.2'
DRIVER_CRATE = '1.10502.0'
ARCHIVES = {
    'linux-aarch64': ('static-libs-linux-arm64.zip', '223fdbdcb130b9dfe2e000cfa37ddb2b88e6180054b2fa46c7cd035b5f48fc2f'),
    'linux-x86_64': ('static-libs-linux-amd64.zip', '5dc17bb9cf79ca2ebe03f50c12bb1cfe28e228d48080db2c5e73880b0843f5e8'),
    'macos-aarch64': ('static-libs-osx-arm64.zip', '0e5668abc62bd17266a92b03c43eff6a07cb3f573d0257bff3f5515cd81cd629'),
    'macos-x86_64': ('static-libs-osx-amd64.zip', '63db614fcf2ceeeef8df950295a3a293d2e268c5c1f1298b1395ed9ebf5c15e3'),
}


def unpack(archive, expected, destination):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise ValueError('DuckDB archive checksum mismatch')
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as stream:
        for info in stream.infolist():
            name = Path(info.filename)
            if len(name.parts) != 1 or not (name.name.endswith('.a') or name.name == 'duckdb.h'):
                raise ValueError('Unexpected DuckDB archive member: ' + info.filename)
            (destination / name).write_bytes(stream.read(info))
    if not (destination / 'libduckdb_static.a').is_file():
        raise ValueError('DuckDB static library is missing')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path.cwd())
    parser.add_argument('--platform', choices=ARCHIVES, required=True)
    parser.add_argument('--target', help='Optional Rust target for a local cross build')
    args = parser.parse_args()
    source = args.source.resolve()
    cargo = tomllib.loads((source / 'Cargo.toml').read_text())
    if cargo['dependencies']['duckdb'] != '=' + DRIVER_CRATE:
        raise SystemExit('Update the pinned DuckDB release and checksums to match Cargo.toml')
    expected_os = 'Linux' if args.platform.startswith('linux') else 'Darwin'
    if platform.system() != expected_os:
        raise SystemExit('Build on the matching operating system')
    expected_arch = 'arm64' if args.platform.endswith('aarch64') else 'x86_64'
    targets = {'linux-aarch64': 'aarch64-unknown-linux-gnu', 'linux-x86_64': 'x86_64-unknown-linux-gnu', 'macos-aarch64': 'aarch64-apple-darwin', 'macos-x86_64': 'x86_64-apple-darwin'}
    if args.target and args.target != targets[args.platform]:
        raise SystemExit('Rust target does not match the requested release platform')
    if not args.target and platform.machine().replace('aarch64', 'arm64') != expected_arch:
        raise SystemExit('Platform does not match the host architecture')
    target = Path(os.environ.get('CARGO_TARGET_DIR', source.parent / 'target')).resolve()
    cache = target / 'duckdb-static' / DRIVER_VERSION / args.platform
    cache.mkdir(parents=True, exist_ok=True)
    name, expected = ARCHIVES[args.platform]
    url = f'https://github.com/duckdb/duckdb/releases/download/v{DRIVER_VERSION}/{name}'
    archive = cache / name
    if not archive.exists():
        temporary = archive.with_suffix('.download')
        urllib.request.urlretrieve(url, temporary)
        temporary.replace(archive)
    unpack(archive, expected, cache / 'input')
    combined = cache / 'combined'
    combined.mkdir(exist_ok=True)
    libraries = sorted((cache / 'input').glob('*.a'))
    library = combined / 'libduckdb_static.a'
    if expected_os == 'Darwin':
        subprocess.run(['/usr/bin/libtool', '-static', '-o', str(library), *map(str, libraries)], check=True)
        cpp = 'c++'
    else:
        # MRI combines archives without losing identically named object files.
        commands = ['create ' + str(library), *('addlib ' + str(p) for p in libraries), 'save', 'end']
        subprocess.run(['ar', '-M'], input='\n'.join(commands) + '\n', text=True, check=True)
        cpp = 'stdc++'
    (combined / 'duckdb.h').write_bytes((cache / 'input/duckdb.h').read_bytes())
    env = dict(os.environ, DUCKDB_STATIC='1', DUCKDB_LIB_DIR=str(combined))
    command = ['cargo', 'rustc', '--locked', '--release', '--no-default-features']
    if args.target:
        command += ['--target', args.target]
    subprocess.run(command + ['--', '-l', cpp], cwd=source, env=env, check=True)
    output = target / args.target / 'release' if args.target else target / 'release'
    binary = output / 'orchiddb'
    metadata = {
        'version': cargo['package']['version'], 'platform': args.platform,
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip(),
        'rust': subprocess.check_output(['rustc', '--version'], text=True).strip(),
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'duckdb': {'version': DRIVER_VERSION, 'linkage': 'static', 'archive': url, 'sha256': expected},
    }
    (output / 'BUILD.json').write_text(json.dumps(metadata, indent=2) + '\n')


if __name__ == '__main__':
    main()
