#!/usr/bin/env python3
"""Fetch immutable upstream sources locally. Never runs from Actions."""
import json,os,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
CACHE=Path(os.environ.get('CONFORMANCE_UPSTREAM_CACHE',ROOT/'cache'))
def fetch():
 for name,source in json.loads((ROOT/'sources.json').read_text()).items():
  dest=CACHE/name
  if not (dest/'.git').exists():
   dest.mkdir(parents=True,exist_ok=True);subprocess.run(['git','init','-q',str(dest)],check=True)
   subprocess.run(['git','-C',str(dest),'remote','add','origin',source['url']],check=True)
  current=subprocess.run(['git','-C',str(dest),'rev-parse','HEAD'],capture_output=True,text=True).stdout.strip()
  if current!=source['revision']:
   subprocess.run(['git','-C',str(dest),'fetch','--depth','1','origin',source['revision']],check=True)
   subprocess.run(['git','-C',str(dest),'checkout','--detach','--quiet',source['revision']],check=True)
  assert not subprocess.check_output(['git','-C',str(dest),'status','--porcelain'],text=True).strip(),f'Modified upstream tree: {dest}'
  print(name,source['revision'])
if __name__=='__main__':fetch()
