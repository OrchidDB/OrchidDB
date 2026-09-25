#!/usr/bin/env python3
"""Validate immutable release inputs and create retryable, checksummed GitHub drafts."""
import argparse, hashlib, json, os, re, subprocess, tomllib
from pathlib import Path
import xml.etree.ElementTree as ET

def run(*args):
    return subprocess.check_output(args, text=True).strip()

def version(root):
    if (root/'pom.xml').exists():
        return ET.parse(root/'pom.xml').findtext('{http://maven.apache.org/POM/4.0.0}version')
    if (root/'package.json').exists(): return json.loads((root/'package.json').read_text())['version']
    if (root/'pyproject.toml').exists(): return tomllib.loads((root/'pyproject.toml').read_text())['project']['version']
    if (root/'mix.exs').exists(): return re.search(r'version: "([^"]+)"', (root/'mix.exs').read_text())[1]
    if (root/'CMakeLists.txt').exists(): return re.search(r'project\(OrchidDB VERSION ([^ ]+)', (root/'CMakeLists.txt').read_text())[1]
    return tomllib.loads((root/'Cargo.toml').read_text())['package']['version']

def validate(tag):
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-(?:alpha|beta|rc)\.\d+)?', tag):
        raise SystemExit('Expected vX.Y.Z (optionally -alpha.N/-beta.N/-rc.N)')
    if tag != 'v'+version(Path.cwd()): raise SystemExit('Tag/package version mismatch')
    if run('git','rev-parse','HEAD') != run('git','rev-parse',f'refs/tags/{tag}^{{commit}}'):
        raise SystemExit('HEAD must equal the immutable release tag')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('command', choices=['check','draft'])
    p.add_argument('--tag', default=os.environ.get('RELEASE_TAG'))
    p.add_argument('--directory', type=Path, default=Path('dist'))
    args=p.parse_args()
    if not args.tag: p.error('--tag or RELEASE_TAG required')
    validate(args.tag)
    if args.command=='check':
        print('Validated',args.tag,run('git','rev-parse','HEAD')); return
    files=sorted(f for f in args.directory.iterdir() if f.is_file() and f.name not in ['SHA256SUMS','release-manifest.json'] and not f.name.endswith('.sha256'))
    if not files: raise SystemExit('No release artifacts; refusing an empty release')
    meta={'version':version(Path.cwd()),'commit':run('git','rev-parse','HEAD'),'artifacts':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in files}}
    for pin in ['CORE_REVISION','native/CORE_REVISION','NATIVE_REVISION']:
        if Path(pin).exists(): meta[pin]=(Path(pin).read_text().strip())
    manifest=args.directory/'release-manifest.json';manifest.write_text(json.dumps(meta,indent=2,sort_keys=True)+'\n')
    files.append(manifest)
    checksums=args.directory/'SHA256SUMS'
    checksums.write_text(''.join(f'{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.name}\n' for f in files))
    files.append(checksums)
    existing=subprocess.run(['gh','release','view',args.tag,'--json','isDraft','--jq','.isDraft'],text=True,capture_output=True)
    if existing.returncode==0:
        if existing.stdout.strip()!='true': raise SystemExit('Published release is immutable; refusing to replace assets')
        subprocess.run(['gh','release','upload',args.tag,*map(str,files),'--clobber'],check=True)
    else:
        flags=['--prerelease'] if '-' in args.tag else []
        subprocess.run(['gh','release','create',args.tag,*map(str,files),'--verify-tag','--draft',*flags,'--title',f'OrchidDB {args.tag}','--notes','Validated artifacts for '+args.tag+'. See repository README for platform support and setup. SHA256SUMS and release-manifest.json identify the exact sources and artifacts.'],check=True)
if __name__=='__main__': main()
