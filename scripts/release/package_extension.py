#!/usr/bin/env python3
"""Package an already-built local extension without rebuilding or publishing it."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
artifact = root / 'extension/build/orchid.duckdb_extension'
if not artifact.is_file():
    raise SystemExit('Build the extension first: python3 extension/scripts/build.py --release')
payload = artifact.read_bytes()
fields = [payload[-512 + i * 32:-512 + (i + 1) * 32].rstrip(b'\0').decode() for i in range(8)][::-1]
if fields[0] != '4' or fields[4] != 'CPP':
    raise SystemExit('Unrecognized DuckDB extension metadata')
sha = hashlib.sha256(payload).hexdigest()
out = root / 'target/packages' / sha
out.mkdir(parents=True, exist_ok=True)
shutil.copy2(artifact, out / artifact.name)
shutil.copy2(root / 'LICENSE.md', out / 'LICENSE.md')
metadata = {'artifact': artifact.name, 'sha256': sha,
            'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'working_tree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True)),
            'signed': False, 'duckdb_version': fields[2], 'platform': fields[1], 'extension_version': fields[3]}
(out / 'manifest.json').write_text(json.dumps(metadata, indent=2) + '\n')
(out / 'SHA256SUMS').write_text(f'{sha}  {artifact.name}\n')
print(out)
