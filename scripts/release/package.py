#!/usr/bin/env python3
"""Build portable CLI archives from a native release binary (Python 3.11+)."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    'x86_64-unknown-linux-gnu',
    'x86_64-apple-darwin',
    'aarch64-apple-darwin',
    'x86_64-pc-windows-msvc',
}


def package_metadata(tag):
    manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
    version = manifest['version']
    if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?', tag):
        raise ValueError('Release tags must have the form v0.1.0 or v0.1.0-rc.1')
    if tag != f'v{version}':
        raise ValueError(f'Tag {tag} does not match Cargo.toml version {version}')
    return manifest


def revision(ref='HEAD'):
    return subprocess.check_output(
        ['git', 'rev-parse', '--verify', ref], cwd=ROOT, text=True).strip()


def metadata(args):
    manifest = package_metadata(args.tag)
    commit = revision(f'refs/tags/{args.tag}^{{commit}}')
    if commit != revision():
        raise ValueError('The checkout must match the requested release tag')
    values = {'tag': args.tag, 'version': manifest['version'], 'commit': commit,
              'prerelease': str('-' in manifest['version'] or manifest['version'].startswith('0.')).lower()}
    if args.github_output:
        with Path(args.github_output).open('a') as stream:
            for key, value in values.items():
                stream.write(f'{key}={value}\n')
    print(json.dumps(values, indent=2))


def smoke(binary):
    binary = str(binary.resolve())
    help_result = subprocess.run([binary, '--help'], check=True, text=True,
                                 capture_output=True, timeout=60)
    if 'crabgraph' not in help_result.stdout.lower():
        raise ValueError('The supplied executable is not the Crabgraph CLI')
    result = subprocess.run([binary, '--query', 'RETURN 1 AS value'], check=True,
                            text=True, capture_output=True, timeout=120)
    if result.stdout.strip().splitlines() != ['value', '1']:
        raise ValueError(f'Unexpected CLI smoke result: {result.stdout!r}')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def archive(args):
    manifest = package_metadata(args.tag)
    if args.target not in TARGETS:
        raise ValueError(f'Unsupported release target: {args.target}')
    binary = args.binary.resolve()
    if not binary.is_file():
        raise ValueError(f'Missing release binary: {binary}')
    smoke(binary)
    args.output.mkdir(parents=True, exist_ok=True)
    stem = f'crabgraph-{args.tag}-{args.target}'
    windows = args.target.endswith('windows-msvc')
    executable = 'crabgraph.exe' if windows else 'crabgraph'
    destination = args.output / (stem + ('.zip' if windows else '.tar.gz'))
    with tempfile.TemporaryDirectory(prefix='crabgraph-release-') as temp:
        folder = Path(temp) / stem
        folder.mkdir()
        shutil.copy2(binary, folder / executable)
        (folder / executable).chmod(0o755)
        shutil.copy2(ROOT / 'LICENSE.md', folder / 'LICENSE.md')
        (folder / 'README.txt').write_text(
            f'Crabgraph {args.tag}\nTarget: {args.target}\n\n'
            'An embedded graph query engine written in Rust.\n\n'
            f'Extract this archive and run ./{executable} --help\n'
            f'First query: ./{executable} --query "RETURN 1 AS value"\n\n'
            'The CLI accepts Cypher and Gremlin. SPARQL is exposed through the Rust library.\n'
            'DuckDB is bundled into this build; system runtime libraries are still required.\n'
            'The JVM bridge and Java dependencies are not included in this CLI archive.\n'
            'macOS builds are not Developer ID signed or notarized.\n\n'
            'Documentation: https://docs.crabgraph.net/\n'
            'Source: https://github.com/henneberger/new-graph\n'
            'License terms: see LICENSE.md included in this archive.\n', encoding='utf-8')
        rust = subprocess.check_output(['rustc', '--version'], text=True).strip()
        info = {'project': 'Crabgraph', 'cargo_package': manifest['name'],
                'version': manifest['version'], 'tag': args.tag, 'target': args.target,
                'commit': revision(), 'rust': rust,
                'features': ['duckdb'], 'binary_sha256': digest(binary)}
        (folder / 'BUILD.json').write_text(json.dumps(info, indent=2) + '\n')
        if windows:
            with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED) as output:
                for path in sorted(folder.iterdir()):
                    output.write(path, arcname=f'{stem}/{path.name}')
        else:
            with tarfile.open(destination, 'w:gz') as output:
                output.add(folder, arcname=stem)
    destination.with_name(destination.name + '.sha256').write_text(
        f'{digest(destination)}  {destination.name}\n')
    print(destination)


def verify(args):
    archives = sorted(list(args.directory.glob('*.tar.gz')) + list(args.directory.glob('*.zip')))
    expected = {f'crabgraph-{args.tag}-{target}' + ('.zip' if target.endswith('windows-msvc') else '.tar.gz')
                for target in TARGETS}
    if {p.name for p in archives} != expected:
        raise ValueError('Release must contain exactly one archive for each of the four targets')
    lines = []
    for path in archives:
        line = f'{digest(path)}  {path.name}\n'
        if path.with_name(path.name + '.sha256').read_text() != line:
            raise ValueError(f'Checksum mismatch: {path.name}')
        lines.append(line)
    (args.directory / 'SHA256SUMS').write_text(''.join(lines))
    print(f'Verified {len(archives)} release archives and wrote SHA256SUMS')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    meta = sub.add_parser('metadata')
    meta.add_argument('--tag', required=True)
    meta.add_argument('--github-output', type=Path)
    meta.set_defaults(run=metadata)
    pack = sub.add_parser('archive')
    pack.add_argument('--tag', required=True)
    pack.add_argument('--target', required=True)
    pack.add_argument('--binary', type=Path, required=True)
    pack.add_argument('--output', type=Path, default=ROOT / 'target/release-packages')
    pack.set_defaults(run=archive)
    check = sub.add_parser('verify')
    check.add_argument('--tag', required=True)
    check.add_argument('--directory', type=Path, required=True)
    check.set_defaults(run=verify)
    args = parser.parse_args()
    try:
        args.run(args)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f'error: {error}\n')


if __name__ == '__main__':
    main()
