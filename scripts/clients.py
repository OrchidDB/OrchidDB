#!/usr/bin/env python3
"""Build and stage client bindings from this monorepo checkout."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def revision():
    value = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True):
        value += '-dirty'
    return value


def metadata(value):
    (ROOT / 'clients/native/CORE_REVISION').write_text(value + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['metadata', 'native'])
    parser.add_argument('--release', action='store_true')
    args = parser.parse_args()
    if args.action == 'metadata':
        metadata(revision())
        return
    command = ['cargo', 'build', '--locked', '-p', 'orchiddb-compiler-native']
    if args.release:
        command.append('--release')
    subprocess.run(command, cwd=ROOT, check=True)
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    if not target.is_absolute():
        target = ROOT / target
    library = {'Darwin': 'liborchiddb_compiler.dylib', 'Linux': 'liborchiddb_compiler.so'}[platform.system()]
    source = target / ('release' if args.release else 'debug') / library
    native = ctypes.CDLL(str(source))
    native.orchiddb_core_revision.restype = ctypes.c_char_p
    value = native.orchiddb_core_revision().decode()
    metadata(value)
    print(f'ORCHIDDB_NATIVE_LIBRARY={source}')


if __name__ == '__main__':
    main()
