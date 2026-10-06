#!/usr/bin/env python3
"""Publish one locally validated extension. Never overwrite tags or assets."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile

from package_extension import ROOT
from matrix import validated_packages


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def preflight(version, repo):
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version) or len(version) > 31:
        raise SystemExit('Set VERSION to a new extension version, e.g. VERSION=0.1.0 (no v prefix)')
    if not re.fullmatch(r'[\w.-]+/[\w.-]+', repo):
        raise SystemExit('REPO must be a GitHub OWNER/REPO')
    changes = output('git', 'status', '--porcelain', '--untracked-files=all')
    if changes:
        raise SystemExit('Commit the intended release source before publishing; the checkout must be clean.\n'
                         f'Blocking changes in {ROOT}:\n{changes}')
    revision = output('git', 'rev-parse', 'HEAD')
    # A failed API request is never treated as an absent tag. Also verify that
    # the exact source revision has been pushed before spending time building.
    output('gh', 'api', f'repos/{repo}/commits/{revision}', '--jq', '.sha')
    tag = f'extension-v{version}'
    refs = json.loads(output('gh', 'api', f'repos/{repo}/git/matching-refs/tags/{tag}'))
    if any(ref['ref'] == f'refs/tags/{tag}' for ref in refs):
        raise SystemExit(f'{tag} already exists; use a new version. Published tags/assets are never replaced')
    return revision, tag


def bundle(directory, version, revision):
    metadata = json.loads((directory / 'manifest.json').read_text())
    if (metadata['revision'] != revision or metadata['working_tree_modified'] is not False
            or metadata['profile'] != 'release' or metadata['reused_rust'] is not False):
        raise SystemExit('Build an optimized extension from this clean revision without --skip-rust before publishing')
    if metadata['extension_version'] != version:
        raise SystemExit('Built extension version differs from VERSION; rebuild with VERSION set')
    name = f"orchid-{version}-duckdb-{metadata['duckdb_version']}-{metadata['platform']}"
    if not re.fullmatch(r'[\w.-]+', name):
        raise SystemExit('Invalid extension version or platform metadata')
    archive = directory / f'{name}.tar.gz'
    # Stable archives preserve exactly the same bytes when packaging is retried.
    with archive.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w') as tar:
            for filename in ('orchid.duckdb_extension', 'LICENSE.md', 'manifest.json', 'SHA256SUMS'):
                path = directory / filename
                info = tar.gettarinfo(str(path), arcname=f'{name}/{filename}')
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                info.mode = 0o644
                with path.open('rb') as source:
                    tar.addfile(info, source)
    checksum = archive.with_name(archive.name + '.sha256')
    checksum.write_text(f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n')
    return archive, checksum, metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--repo', default='OrchidDB/OrchidDB')
    parser.add_argument('--check', action='store_true', help='check prerequisites without building or publishing')
    parser.add_argument('--notes-file', type=Path)
    args = parser.parse_args()
    revision, tag = preflight(args.version, args.repo)
    if args.notes_file and not args.notes_file.is_file():
        parser.error('--notes-file must name an existing file')
    if args.check:
        print(f'Ready to release {tag} from {revision} to {args.repo}')
        return
    directories = validated_packages(args.version, revision)
    assets = []
    platforms = []
    for directory in directories:
        archive, checksum, metadata = bundle(directory, args.version, revision)
        assets.extend([str(archive), str(checksum)])
        platforms.append(metadata['platform'])
    notes = args.notes_file
    if notes is None:
        notes = directories[0] / 'release-notes.md'
        notes.write_text(
            f"Orchid DuckDB extension {args.version}\n\n"
            f"DuckDB: {metadata['duckdb_version']}\nPlatforms: {', '.join(platforms)}\nSource: {revision}\n\n"
            'Extract the archive and load orchid.duckdb_extension in a matching DuckDB installation. '
            'This extension is unsigned; start DuckDB with -unsigned (or allow_unsigned_extensions=true).\n'
        )
    command = ['gh', 'release', 'create', tag, *assets, '--repo', args.repo,
               '--target', revision, '--title', f'Orchid extension {args.version}', '--notes-file', str(notes)]
    if '-' in args.version:
        command.append('--prerelease')
    subprocess.run(command, cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
