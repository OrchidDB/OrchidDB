#!/usr/bin/env python3
"""Build release binaries locally; GitHub is never an execution backend."""
import argparse, concurrent.futures, hashlib, importlib.util, json, os, platform, shutil, subprocess, urllib.request
from pathlib import Path
P={'macos-aarch64':None,'macos-x86_64':'x86_64-apple-darwin','linux-x86_64':'x86_64-unknown-linux-gnu'}

def run(argv,env,cwd=None):
 print('+ '+' '.join(map(str,argv)),flush=True);subprocess.run(list(map(str,argv)),env=env,cwd=cwd,check=True)

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--version',required=True);p.add_argument('--workspace',type=Path,default=Path(__file__).resolve().parents[3]);p.add_argument('--platform',choices=P);p.add_argument('--kind',choices=['native','java','cli']);a=p.parse_args()
 w=a.workspace.resolve();release=w/'.releases'/a.version;state=json.loads((release/'state.json').read_text())
 if any(state['validation'].get(k,{}).get('exit_code')!=0 for k in ['core','clients']):raise SystemExit('Successful local validation is required')
 sources=release/'local-source';sources.mkdir(exist_ok=True,parents=True)
 repos={'engine':'orchiddb','native':'orchiddb-native','java':'orchiddb-java','cli':'orchiddb-cli','rust':'orchiddb-rust','python':'orchiddb-python','javascript':'orchiddb-js','elixir':'orchiddb-elixir','cpp':'orchiddb-cpp'}
 for kind,name in repos.items():
  src=sources/name
  if not src.exists():run(['git','worktree','add','--detach',src,state['pins'][kind]],os.environ,w/name)
  if subprocess.check_output(['git','rev-parse','HEAD'],cwd=src,text=True).strip()!=state['pins'][kind] or subprocess.check_output(['git','status','--porcelain'],cwd=src,text=True).strip():raise SystemExit('Release source must match the clean tested revision: '+str(src))
 receipt=release/'local-builds';receipt.mkdir(exist_ok=True)
 def build(target_platform,kind):
  target=P[target_platform];cache=w/'target'/({'linux-x86_64':'local-linux'}.get(target_platform,'native-release'))
  env=dict(os.environ,RUSTUP_TOOLCHAIN='1.93.1',CARGO_TARGET_DIR=str(cache),CARGO_BUILD_JOBS=os.environ.get('LOCAL_BUILD_JOBS','2'),ORCHIDDB_RELEASE_BUILD='1')
  env['PATH']='/opt/homebrew/bin:'+str(w/'.releases/tooling/bin')+':'+env.get('PATH','')
  toolchain=Path.home()/'.rustup/toolchains/1.93.1-aarch64-apple-darwin/bin'
  if toolchain.exists():env['PATH']=str(toolchain)+':'+env['PATH']
  if target_platform.startswith('macos'):env['MACOSX_DEPLOYMENT_TARGET']='11.0'
  output=cache/(target or '')/'release'
  filename='orchiddb' if kind=='cli' else ('liborchiddb_compiler' if kind=='native' else 'liborchiddb_java')+('.dylib' if target_platform.startswith('macos') else '.so')
  binary=output/filename;record=receipt/f'{kind}-{target_platform}.json'
  if record.exists() and binary.exists():
   old=json.loads(record.read_text())
   if old['source_commit']==state['pins'][kind] and old.get('core_revision')==state['pins']['engine'] and old['sha256']==hashlib.sha256(binary.read_bytes()).hexdigest():print('Reuse '+kind+'/'+target_platform,flush=True);return
  src=sources/repos[kind];manifest=src/('native/Cargo.toml' if kind=='java' else 'Cargo.toml')
  if target_platform=='linux-x86_64':command=['cargo','zigbuild','--locked','--release','--target',target+'.2.34','--manifest-path',manifest]
  else:command=['cargo','build','--locked','--release','--manifest-path',manifest]+(['--target',target] if target else [])
  if kind=='cli':
   spec=importlib.util.spec_from_file_location('cli_build',src/'scripts/release/build.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
   driver=cache/'duckdb-static'/mod.DRIVER_VERSION/target_platform;driver.mkdir(parents=True,exist_ok=True)
   name,expected=mod.ARCHIVES[target_platform];archive=driver/name;url=f'https://github.com/duckdb/duckdb/releases/download/v{mod.DRIVER_VERSION}/{name}'
   if not archive.exists():urllib.request.urlretrieve(url,archive)
   mod.unpack(archive,expected,driver/'input');combined=driver/'combined';combined.mkdir(exist_ok=True)
   libs=sorted((driver/'input').glob('*.a'));library=combined/'libduckdb_static.a'
   if not library.exists():
    if target_platform.startswith('macos'):run(['/usr/bin/libtool','-static','-o',library,*libs],env)
    else:
     ar=('/opt/homebrew/opt/llvm/bin/llvm-ar' if Path('/opt/homebrew/opt/llvm/bin/llvm-ar').exists() else shutil.which('llvm-ar',path=env['PATH']) or 'ar');commands=['create '+str(library),*('addlib '+str(x) for x in libs),'save','end'];subprocess.run([ar,'-M'],input='\n'.join(commands)+'\n',text=True,env=env,check=True)
   (combined/'duckdb.h').write_bytes((driver/'input/duckdb.h').read_bytes());env.update(DUCKDB_STATIC='1',DUCKDB_LIB_DIR=str(combined))
   extra=['-l','c++']
   if target_platform=='linux-x86_64':
    sdk=w/'target/cross-sdk/linux/sysroot/usr/lib/gcc/x86_64-linux-gnu/12'
    if not (sdk/'libstdc++.a').exists():run(['bash',Path(__file__).with_name('local_linux_sdk.sh'),w],env)
    command=['cargo-zigbuild','rustc','--locked','--release','--target',target+'.2.34','--manifest-path',manifest]
    extra=['-L','native='+str(sdk),'-l','static=stdc++']
   else:command[1]='rustc'
   command+=['--no-default-features','--features','quickwit,elasticsearch','--',*extra]
  run(command,env,src)
  digest=hashlib.sha256(binary.read_bytes()).hexdigest()
  meta={'version':a.version,'platform':target_platform,'source_commit':state['pins'][kind],'core_revision':state['pins']['engine'],'sha256':digest,'local':True,'binary':str(binary),'command':list(map(str,command))}
  if kind=='cli':
   buildmeta={'version':a.version,'platform':target_platform,'source_commit':state['pins'][kind],'rust':subprocess.check_output(['rustc','--version'],env=env,text=True).strip(),'binary_sha256':digest,'duckdb':{'version':mod.DRIVER_VERSION,'linkage':'static','archive':url,'sha256':expected}}
   (output/'BUILD.json').write_text(json.dumps(buildmeta,indent=2)+'\n')
  record.write_text(json.dumps(meta,indent=2)+'\n')
 def group(platforms):
  for item in platforms:
   for kind in ([a.kind] if a.kind else ['native','java','cli']):build(item,kind)
 groups=[[a.platform]] if a.platform else [['macos-aarch64','macos-x86_64'],['linux-x86_64']]
 with concurrent.futures.ThreadPoolExecutor(max_workers=len(groups)) as pool:
  for result in pool.map(group,groups):pass
if __name__=='__main__':main()
