#!/usr/bin/env python3
"""Verify engine/client registry archives together without publishing either crate."""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--core', type=Path, required=True)
p.add_argument('--rust', type=Path)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--test', action='store_true')
p.add_argument('--system-test-driver', action='store_true', help='Test with an existing DuckDB 1.5.2 library instead of building it')
a = p.parse_args()
a.output = a.output.resolve()
a.output.mkdir(parents=True, exist_ok=True)

def copy_checkout(source, destination):
    source = source.resolve()
    files = subprocess.check_output(['git', '-C', str(source), 'ls-files', '--cached', '--others', '--exclude-standard', '-z']).decode().split('\0')
    for name in set(filter(None, files)):
        src = source / name
        if src.is_file():
            (destination / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, destination / name)

with tempfile.TemporaryDirectory(prefix='orchiddb-crates-') as temp:
    root = Path(temp)
    copy_checkout(a.core, root / 'orchiddb')
    manifest = root / 'orchiddb/Cargo.toml'
    version = tomllib.loads(manifest.read_text())['package']['version']
    manifest.write_text(re.sub(r'\n\[workspace\]\n.*?(?=\n\[)', '\n', manifest.read_text(), flags=re.S))
    members = ['orchiddb']
    packages = [('orchiddb', version)]
    shutil.copy2(a.core / 'Cargo.lock', root / 'Cargo.lock')
    if a.rust:
        copy_checkout(a.rust, root / 'orchiddb-rust')
        members.append('orchiddb-rust')
        manifest = root / 'orchiddb-rust/Cargo.toml'
        client = tomllib.loads(manifest.read_text())
        if client['dependencies']['orchiddb']['version'] != '=' + version:
            raise SystemExit('Client engine version does not match the staged engine')
        manifest.write_text(re.sub(r'^orchiddb = \{[^\n]+', 'orchiddb = { version = "=' + version + '", path = "../orchiddb", default-features = false }', manifest.read_text(), flags=re.M))
        packages.append((client['package']['name'], client['package']['version']))
    (root / 'Cargo.toml').write_text('[workspace]\nresolver = "2"\nmembers = ' + repr(members).replace("'", '"') + '\n')
    subprocess.run(['cargo', 'metadata', '--format-version', '1'], cwd=root, check=True, stdout=subprocess.DEVNULL)
    if a.test:
        subprocess.run(['cargo', 'test', '--locked', '-p', 'orchiddb', '--test', 'sql_compiler', '--test', 'execution'], cwd=root, check=True)
        subprocess.run(['cargo', 'test', '--locked', '-p', 'orchiddb', '--doc', 'spargebra::'], cwd=root, check=True)
        if a.rust:
            subprocess.run(['cargo', 'test', '--locked', '-p', 'orchiddb-client', *(['--no-default-features'] if a.system_test_driver else [])], cwd=root, check=True)
    # Cargo verifies dependent packages against its local workspace package index.
    subprocess.run(['cargo', 'package', '--workspace', '--locked', '--registry', 'crates-io'], cwd=root, check=True)
    target = Path(os.environ.get('CARGO_TARGET_DIR', root / 'target')).resolve()
    for name, version in packages:
        artifact = target / 'package' / f'{name}-{version}.crate'
        shutil.copy2(artifact, a.output / artifact.name)
