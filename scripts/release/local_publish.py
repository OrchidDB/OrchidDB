#!/usr/bin/env python3
"""Upload verified local packages to immutable GitHub releases; never build remotely."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

REPOS = {'engine': 'OrchidDB', 'native': 'OrchidDB-native', 'rust': 'OrchidDB-rust',
         'cli': 'OrchidDB-cli', 'java': 'OrchidDB-java', 'python': 'OrchidDB-python',
         'javascript': 'OrchidDB-js', 'elixir': 'OrchidDB-elixir', 'cpp': 'OrchidDB-cpp'}

def run(args, **kwargs):
    return subprocess.check_output(list(map(str, args)), text=True, **kwargs).strip()

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--workspace', type=Path, default=Path(__file__).resolve().parents[3])
    args = parser.parse_args()
    root = args.workspace.resolve() / '.releases' / args.version
    verified = json.loads((root / 'local-verified.json').read_text())
    state = json.loads((root / 'state.json').read_text())
    assert verified['version'] == args.version and verified['sources'] == state['pins']
    assert all(state['validation'][key]['exit_code'] == 0 for key in ['core', 'clients'])
    tag = 'v' + args.version
    # Freeze publication inputs separately from resumable package outputs.
    for kind in REPOS:
        source = root / 'local-artifacts' / kind
        destination = root / 'local-publication' / kind
        destination.mkdir(parents=True, exist_ok=True)
        for name, expected in verified['artifacts'][kind].items():
            assert digest(source / name) == expected, f'Changed verified artifact: {kind}/{name}'
            shutil.copy2(source / name, destination / name)
        if kind == 'engine':
            for name in [f'conformance-{args.version}.zip', 'conformance.md', 'conformance-summary.json']:
                shutil.copy2(root / name, destination / name)
        files = {p.name: digest(p) for p in sorted(destination.iterdir())
                 if p.is_file() and p.name not in ['SHA256SUMS', 'release-manifest.json']}
        manifest = {'version': args.version, 'commit': state['pins'][kind],
                    'build_location': 'local', 'artifacts': files}
        (destination / 'release-manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
        files['release-manifest.json'] = digest(destination / 'release-manifest.json')
        (destination / 'SHA256SUMS').write_text(''.join(f'{value}  {name}\n' for name, value in sorted(files.items())))

    def upload(kind):
        repo = 'OrchidDB/' + REPOS[kind]
        directory = root / 'local-publication' / kind
        expected = {p.name: digest(p) for p in directory.iterdir() if p.is_file()}
        result = subprocess.run(['gh', 'release', 'view', tag, '--repo', repo,
                                 '--json', 'databaseId,isDraft,url'], text=True, capture_output=True)
        existing = json.loads(result.stdout) if result.returncode == 0 else None
        if existing is None:
            run(['gh', 'release', 'create', tag, '--repo', repo, '--verify-tag', '--draft',
                 '--title', 'OrchidDB ' + tag, '--notes-file', root / 'release-notes.md'])
            existing = json.loads(run(['gh', 'release', 'view', tag, '--repo', repo,
                                       '--json', 'databaseId,isDraft,url']))
        assets = json.loads(run(['gh', 'api', f'repos/{repo}/releases/{existing["databaseId"]}/assets', '--paginate']))
        remote = {asset['name']: asset for asset in assets}
        for name, value in expected.items():
            asset = remote.get(name)
            if asset and asset.get('digest') == 'sha256:' + value:
                continue
            if asset:
                with tempfile.TemporaryDirectory() as temporary:
                    run(['gh', 'release', 'download', tag, '--repo', repo, '--pattern', name, '--dir', temporary])
                    if digest(Path(temporary) / name) == value:
                        continue
            assert existing['isDraft'], f'Refusing to replace published bytes: {repo}/{name}'
            run(['gh', 'release', 'upload', tag, directory / name, '--repo', repo, '--clobber'])
        assert set(remote).issubset(expected), f'Unexpected assets in {repo}'
        # Verify GitHub's digest of every uploaded byte before publishing.
        assets = json.loads(run(['gh', 'api', f'repos/{repo}/releases/{existing["databaseId"]}/assets', '--paginate']))
        assert {item['name'] for item in assets} == set(expected)
        for asset in assets:
            if asset.get('digest') != 'sha256:' + expected[asset['name']]:
                with tempfile.TemporaryDirectory() as temporary:
                    run(['gh', 'release', 'download', tag, '--repo', repo, '--pattern', asset['name'], '--dir', temporary])
                    assert digest(Path(temporary) / asset['name']) == expected[asset['name']]
        print('Verified uploaded local assets: ' + repo, flush=True)
        return kind, dict(existing, repository=repo, revision=state['pins'][kind])

    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        receipts = dict(pool.map(upload, REPOS))
    (root / 'github-releases.json').write_text(json.dumps(receipts, indent=2) + '\n')
    # All nine distributions are uploaded and verified before any draft is made public.
    for kind, receipt in receipts.items():
        run(['gh', 'release', 'edit', tag, '--repo', receipt['repository'], '--draft=false',
             '--latest', '--notes-file', root / 'release-notes.md'])
        receipt.update(json.loads(run(['gh', 'release', 'view', tag, '--repo', receipt['repository'],
                                      '--json', 'databaseId,isDraft,url'])))
        assert not receipt['isDraft']
        (root / 'github-releases.json').write_text(json.dumps(receipts, indent=2) + '\n')
        print(receipt['url'], flush=True)

if __name__ == '__main__':
    main()
