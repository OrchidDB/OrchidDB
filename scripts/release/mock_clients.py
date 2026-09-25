#!/usr/bin/env python3
"""Create explicitly labeled, non-installable client download placeholders."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

CLIENTS = {'python': 'Python', 'javascript': 'JavaScript / TypeScript', 'java': 'Java'}

def generate(output):
    output.mkdir(parents=True, exist_ok=True)
    for key, label in CLIENTS.items():
        path = output / f'orchiddb-{key}-placeholder.zip'
        with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as archive:
            for name, body in {
                'README.txt': f'OrchidDB {label} — DOWNLOAD PLACEHOLDER\n\nThis is a mock download, not an installable SDK.\nIt contains no executable or library.\nClient API design: https://docs.orchiddb.com/client-apis.html\n',
                'placeholder.json': json.dumps({'project': 'OrchidDB', 'language': key, 'placeholder': True, 'installable': False}, indent=2) + '\n',
            }.items():
                entry = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
                entry.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(entry, body)
        path.with_name(path.name + '.sha256').write_text(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    generate(parser.parse_args().output)
