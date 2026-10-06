#!/usr/bin/env python3
"""Package an already-built local extension without rebuilding or publishing it."""
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]


def package(root=ROOT):
    artifact = root / 'extension/build/orchid.duckdb_extension'
    if not artifact.is_file():
        raise SystemExit('Build the extension first: python3 extension/scripts/build.py --release')
    payload = artifact.read_bytes()
    fields = [payload[-512 + i * 32:-512 + (i + 1) * 32].rstrip(b'\0').decode() for i in range(8)][::-1]
    if fields[0] != '4' or fields[4] != 'CPP':
        raise SystemExit('Unrecognized DuckDB extension metadata')
    if not fields[1].startswith(('osx_', 'linux_')):
        raise SystemExit('Only Linux/macOS extension packages are supported')
    sha = hashlib.sha256(payload).hexdigest()
    receipt = artifact.with_name('build-manifest.json')
    provenance = json.loads(receipt.read_text()) if receipt.exists() else {}
    if provenance and provenance.get('sha256') != sha:
        raise SystemExit('Build manifest does not match the extension; rebuild before packaging')
    out = root / 'target/packages' / sha
    out.mkdir(parents=True, exist_ok=True)
    shutil.copy2(artifact, out / artifact.name)
    shutil.copy2(root / 'LICENSE.md', out / 'LICENSE.md')
    metadata = {'artifact': artifact.name, 'sha256': sha,
                'revision': provenance.get('revision'),
                'working_tree_modified': provenance.get('working_tree_modified'),
                'profile': provenance.get('profile'), 'reused_rust': provenance.get('reused_rust'),
                'signed': False, 'duckdb_version': fields[2], 'platform': fields[1], 'extension_version': fields[3]}
    (out / 'manifest.json').write_text(json.dumps(metadata, indent=2) + '\n')
    (out / 'SHA256SUMS').write_text(f'{sha}  {artifact.name}\n')
    return out


if __name__ == '__main__':
    print(package())
