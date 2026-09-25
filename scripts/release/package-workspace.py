#!/usr/bin/env python3
"""Package registry-ready crates together before their first registry publication.

Copy the reviewed source checkouts into a temporary Cargo workspace. Only local
manifest locations change; Cargo normalizes them to exact registry versions.
Cargo verifies the normalized packages using its workspace package index.
"""
import argparse, os, re, shutil, subprocess, tempfile
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--core',type=Path,required=True)
p.add_argument('--rust',type=Path)
p.add_argument('--cli',type=Path)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--no-verify',action='store_true',help='Only for packaging diagnostics; releases must verify')
a=p.parse_args()
if a.cli and not a.rust: p.error('--cli requires --rust')
a.output.mkdir(parents=True,exist_ok=True)
def copy_checkout(source,dest):
    source=source.resolve()
    files=subprocess.check_output(['git','-C',str(source),'ls-files','-z']).decode().split('\0')
    for f in filter(None,files):
        src=source/f
        if src.is_file():
            (dest/f).parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest/f)
with tempfile.TemporaryDirectory(prefix='orchiddb-crates-') as temp:
    root=Path(temp);members=['orchiddb','orchiddb/vendor/spargebra']
    copy_checkout(a.core,root/'orchiddb')
    core=root/'orchiddb/Cargo.toml'
    core.write_text(re.sub(r'\n\[workspace\]\n.*?(?=\n\[)', '\n',core.read_text(),flags=re.S))
    lock=a.core/'Cargo.lock'
    if a.rust:
        copy_checkout(a.rust,root/'orchiddb-rust');members.append('orchiddb-rust');lock=a.rust/'Cargo.lock'
        f=root/'orchiddb-rust/Cargo.toml'
        f.write_text(re.sub(r'^orchiddb = \{[^\n]+', 'orchiddb = { version = "=0.1.0", path = "../orchiddb", default-features = false }',f.read_text(),flags=re.M))
    if a.cli:
        copy_checkout(a.cli,root/'orchiddb-cli');members.append('orchiddb-cli');lock=a.cli/'Cargo.lock'
        f=root/'orchiddb-cli/Cargo.toml'
        f.write_text(re.sub(r'^orchiddb-client = \{[^\n]+', 'orchiddb-client = { version = "=0.1.0", path = "../orchiddb-rust", default-features = false }',f.read_text(),flags=re.M))
    (root/'Cargo.toml').write_text('[workspace]\nresolver = "2"\nmembers = '+str(members).replace("'",'"')+'\n')
    shutil.copy2(lock,root/'Cargo.lock')
    # Resolve source-location changes while retaining the reviewed dependency versions.
    subprocess.run(['cargo','metadata','--format-version','1'],cwd=root,check=True,stdout=subprocess.DEVNULL)
    flags=['--no-verify'] if a.no_verify else []
    subprocess.run(['cargo','package','--workspace','--locked',*flags],cwd=root,check=True)
    target=Path(os.environ.get('CARGO_TARGET_DIR',root/'target')).resolve()
    for f in (target/'package').glob('orchiddb*.crate'): shutil.copy2(f,a.output/f.name)
