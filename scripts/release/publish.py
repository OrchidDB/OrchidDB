#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
REPO = 'OrchidDB/OrchidDB'


def run(*args):
    return subprocess.check_output(list(map(str, args)), cwd=ROOT, text=True).strip()


def completed_assets(version):
    parent = ROOT / 'target/releases' / ('orchiddb-v' + version)
    for receipt in sorted(parent.glob('*/package.json'), key=lambda p: p.stat().st_mtime, reverse=True):
        state = json.loads(receipt.read_text())
        if state.get('version') != version or not state.get('files'):
            continue
        changed = run('git', 'diff', '--name-only', state['commit'], 'HEAD').splitlines()
        if any(name not in ('Makefile', 'README.md', 'scripts/release/release.py', 'scripts/release/publish.py') for name in changed):
            continue
        if all((ROOT / name).is_file() and hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == sha for name, sha in state['files'].items()):
            return receipt.parent / 'upload'
    return None


def publish(assets, version):
    manifest = json.loads((assets / 'release-manifest.json').read_text())
    if manifest['version'] != version:
        raise RuntimeError('Release version does not match the completed assets')
    if run('git', 'status', '--porcelain'):
        raise RuntimeError('Commit source changes before publishing')
    files = sorted(p for p in assets.iterdir() if p.is_file())
    checksums = dict(line.split('  ', 1)[::-1] for line in (assets / 'SHA256SUMS').read_text().splitlines())
    for path in files:
        if path.name != 'SHA256SUMS' and checksums.get(path.name) != hashlib.sha256(path.read_bytes()).hexdigest():
            raise RuntimeError('Release checksum mismatch: ' + path.name)
    run('gh', 'auth', 'status')
    run('git', 'push', 'origin', 'main')
    tag = 'v' + version
    releases = json.loads(run('gh', 'api', '--paginate', 'repos/' + REPO + '/releases'))
    existing = next((r for r in releases if r['tag_name'] == tag), None)
    if existing is None:
        refs = json.loads(run('gh', 'api', 'repos/' + REPO + '/git/matching-refs/tags/' + tag))
        if any(ref['ref'] == 'refs/tags/' + tag for ref in refs):
            raise RuntimeError('Tag already exists without a release; refusing to change it')
        notes = assets.parent / 'release-notes.md'
        notes.write_text('OrchidDB ' + version + '\n\nCLI, DuckDB extension, and all language clients for macOS ARM64, Linux ARM64, and Linux x86-64.\n\nThe CLI requires the DuckDB 1.5.2 shared library installed separately. The extension targets DuckDB 1.5.6.\n\nSource: ' + manifest['commit'] + '\n\nSee release-manifest.json and SHA256SUMS for asset details and checksums.\n')
        run('gh', 'release', 'create', tag, '--repo', REPO, '--target', manifest['commit'], '--title', 'OrchidDB ' + version, '--notes-file', notes, '--draft')
    remote = next(release for release in json.loads(run('gh', 'api', 'repos/' + REPO + '/releases')) if release['tag_name'] == tag)
    known = {asset['name']: asset for asset in remote['assets']}
    for path in files:
        sha = 'sha256:' + hashlib.sha256(path.read_bytes()).hexdigest()
        if path.name in known:
            if known[path.name].get('digest') != sha:
                raise RuntimeError('Existing GitHub asset differs; refusing to overwrite: ' + path.name)
            continue
        print('Uploading ' + path.name, flush=True)
        run('gh', 'release', 'upload', tag, path, '--repo', REPO)
    remote = next(release for release in json.loads(run('gh', 'api', 'repos/' + REPO + '/releases')) if release['tag_name'] == tag)
    uploaded = {asset['name']: asset.get('digest') for asset in remote['assets']}
    if any(uploaded.get(path.name) != 'sha256:' + hashlib.sha256(path.read_bytes()).hexdigest() for path in files):
        raise RuntimeError('GitHub upload verification failed')
    if remote['draft']:
        run('gh', 'release', 'edit', tag, '--repo', REPO, '--draft=false', '--latest')
    print(run('gh', 'release', 'view', tag, '--repo', REPO, '--json', 'url', '--jq', '.url'), flush=True)
