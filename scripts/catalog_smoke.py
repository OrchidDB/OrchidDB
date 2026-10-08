import argparse
import os
from pathlib import Path
import platform
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / 'tools/diagnostics/catalog-smoke'
PACKAGES = ['orchiddb-compiler-native', 'orchiddb-java-native', 'orchiddb-cli', 'orchid-duckdb-compiler']


def run(command, **options):
    subprocess.run([str(value) for value in command], check=True, cwd=ROOT, **options)


def duckdb_library():
    configured = os.environ.get('DUCKDB_LIB_DIR')
    if configured:
        return Path(configured)
    machine = {'arm64':'aarch64', 'AMD64':'x86_64'}.get(platform.machine(), platform.machine())
    system = {'Darwin':'apple-darwin', 'Linux':'unknown-linux-gnu'}.get(platform.system())
    if system is None:
        raise SystemExit('Catalog smoke currently supports macOS and Linux')
    target = machine + '-' + system
    candidates = list((ROOT / 'target/duckdb-download').glob(f'{target}/*/libduckdb.*'))
    if not candidates:
        raise SystemExit('Set DUCKDB_LIB_DIR to the installed DuckDB shared library directory')
    return max(candidates, key=lambda path:path.stat().st_mtime).parent


def build_native_clients():
    library=duckdb_library()
    env=os.environ | dict(LIBRARY_PATH=str(library))
    command=['cargo','build','--locked','-j','1']
    for package in PACKAGES:
        command.extend(['-p',package,'--config',f'profile.dev.package.{package}.strip="debuginfo"'])
    command.extend(['--config','profile.dev.package.orchiddb.debug=0','--config','profile.dev.package.orchiddb.incremental=false'])
    run(command,env=env)
    run([sys.executable,ROOT/'scripts/clients.py','metadata'])


def clients(only):
    env = os.environ.copy()
    library = duckdb_library()
    env['DYLD_LIBRARY_PATH'] = str(library)
    env['LD_LIBRARY_PATH'] = str(library)
    suffix = 'dylib' if platform.system() == 'Darwin' else 'so'
    env['ORCHIDDB_NATIVE_LIBRARY'] = str(ROOT / f'target/debug/liborchiddb_compiler.{suffix}')
    env['PYTHONPATH'] = str(ROOT / 'clients/python/src')
    output = Path(env['AUTH_SMOKE_DIRECTORY'])
    selected = set(only or ['python','javascript','java','cpp','elixir','rust','cli','extension'])
    if 'python' in selected:
        run([sys.executable, SOURCES / 'python.py'], env=env)
    if 'javascript' in selected:
        run(['node', SOURCES / 'javascript.mjs'], env=env)
    if 'java' in selected:
        java_home = subprocess.check_output(['/usr/libexec/java_home','-F','-v','21'], text=True).strip() if platform.system() == 'Darwin' else env['JAVA_HOME']
        jars = (ROOT / 'clients/java/target/native-smoke-classpath.txt').read_text().strip()
        drivers = list((Path.home()/'.m2/repository/org/duckdb/duckdb_jdbc').glob('*/*.jar'))
        if not drivers:
            raise SystemExit('Java smoke requires an installed DuckDB JDBC driver')
        driver = max(drivers, key=lambda path:path.stat().st_mtime)
        classpath = os.pathsep.join([str(ROOT/'clients/java/orchiddb-java/target/classes'),jars,str(driver)])
        run([Path(java_home)/'bin/java', '-Dorchiddb.native.path='+str(ROOT/f'target/debug/liborchiddb_java.{suffix}'), '-cp',classpath,SOURCES/'JavaSmoke.java'],env=env)
    if 'cpp' in selected:
        binary=output/'cpp-smoke'
        run(['c++','-std=c++17','-O0','-Iclients/cpp/include','-Iclients/cpp/build/_deps/json-src/single_include','-Iextension/vendor/duckdb-1.5.6/src/include',SOURCES/'cpp.cpp','-L'+str(library),'-lduckdb','-o',binary],env=env)
        run([binary],env=env)
    if 'elixir' in selected:
        erl=list(Path('/opt/homebrew/Cellar/erlang').glob('*/lib/erlang/bin')) if platform.system()=='Darwin' else []
        if erl:
            env['PATH']=str(max(erl))+os.pathsep+env['PATH']
        command=['elixir']
        for path in (ROOT/'target/elixir/lib').glob('*/ebin'):
            command.extend(['-pa',path])
        for path in ('runtime.ex','catalog_auth.ex','catalog.ex'):
            command.extend(['-r',ROOT/'clients/elixir/lib/orchid_db'/path])
        command.extend(['-r',ROOT/'clients/elixir/lib/orchid_db.ex',SOURCES/'elixir.exs'])
        run(command,env=env)
    if 'rust' in selected:
        binary=output/'rust-smoke'
        command=['rustc','--edition=2024','-C','debuginfo=0','-C','strip=debuginfo','-L','dependency='+str(ROOT/'target/debug/deps'),'-L',str(library),SOURCES/'rust.rs','-o',binary]
        client_artifact = max((ROOT/'target/debug/deps').glob('liborchiddb_client-*.rlib'), key=lambda path:path.stat().st_mtime)
        client_fingerprint = ROOT/'target/debug/.fingerprint'/client_artifact.stem.removeprefix('lib').replace('orchiddb_client-', 'orchiddb-client-', 1)/'lib-orchiddb_client.json'
        client_dependencies = json.loads(client_fingerprint.read_text())['deps']
        core_hash = next(dep[3] for dep in client_dependencies if dep[1] == 'orchiddb').to_bytes(8, 'little').hex()
        core_fingerprint = next(path for path in (ROOT/'target/debug/.fingerprint').glob('orchiddb-*/lib-orchiddb') if path.read_text() == core_hash)
        for crate in ('orchiddb','orchiddb_client','duckdb','tokio'):
            artifacts=list((ROOT/'target/debug/deps').glob('lib'+crate+'-*.rlib'))
            artifact=max(artifacts,key=lambda path:path.stat().st_mtime)
            if crate == 'orchiddb_client':
                artifact = client_artifact
            if crate == 'orchiddb':
                artifact = ROOT/'target/debug/deps'/('lib'+core_fingerprint.parent.name+'.rlib')
                fingerprint = ROOT / 'target/debug/.fingerprint' / artifact.stem.removeprefix('lib') / 'lib-orchiddb.json'
                dependencies = json.loads(fingerprint.read_text())['deps']
                tokio_hash = next(dep[3] for dep in dependencies if dep[1] == 'tokio').to_bytes(8, 'little').hex()
            if crate == 'tokio':
                fingerprint = next(path for path in (ROOT/'target/debug/.fingerprint').glob('tokio-*/lib-tokio') if path.read_text() == tokio_hash)
                artifact = ROOT / 'target/debug/deps' / ('lib'+fingerprint.parent.name+'.rlib')
            command.extend(['--extern',crate+'='+str(artifact)])
        run(command,env=env)
        run([binary],env=env)
    if 'cli' in selected:
        init=output/'init.sql'
        init.write_text("CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace');")
        common=[ROOT/'target/debug/orchiddb','query','MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name','--catalog',env['AUTH_SMOKE_URL'],'--scope','smoke','--graph','smoke','--init',init,'--no-iceberg','--format','table']
        for auth in (['--client-id','admin','--client-secret-env','AUTH_SMOKE_SECRET'],['--token-env','AUTH_SMOKE_TOKEN'],['--token-file',env['AUTH_SMOKE_TOKEN_FILE']],['--client-id','external-client','--client-secret-env','AUTH_SMOKE_IDP_SECRET','--issuer',env['AUTH_SMOKE_ISSUER']],['--auth','token_exchange','--subject-token-env','AUTH_SMOKE_TOKEN']):
            result=subprocess.check_output([str(value) for value in common+auth],cwd=ROOT,env=env,text=True)
            assert 'Ada' in result and 'Grace' in result
        print('PASS CLI: bearer, file, internal/external OAuth, exchange, DuckDB execution',flush=True)
    if 'extension' in selected:
        extension=ROOT/'extension/build/orchid.duckdb_extension'
        cli=ROOT/'extension/vendor/cli/duckdb'
        auths=["client_id='admin', client_secret_env='AUTH_SMOKE_SECRET'", "token_env='AUTH_SMOKE_TOKEN'", "token_file='"+env['AUTH_SMOKE_TOKEN_FILE']+"'", "client_id='external-client', client_secret_env='AUTH_SMOKE_IDP_SECRET', issuer='"+env['AUTH_SMOKE_ISSUER']+"'", "auth='token_exchange', subject_token_env='AUTH_SMOKE_TOKEN'"]
        for auth in auths:
            sql=f"LOAD '{extension}'; CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace'); CALL orchid_register_catalog('smoke','{env['AUTH_SMOKE_URL']}','smoke','smoke',{auth}); SELECT * FROM orchid_query('smoke','MATCH (p:Person)-[:PEER {{score:1, limit:1}}]->(q:Person) RETURN q.name AS name ORDER BY name');"
            result=subprocess.check_output([str(cli),'-unsigned','-csv','-noheader','-c',sql],cwd=ROOT,env=env,text=True)
            assert 'Ada' in result and 'Grace' in result
        print('PASS DuckDB extension: bearer, file, internal/external OAuth, exchange, query execution',flush=True)


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--no-build',action='store_true')
    parser.add_argument('--only',nargs='+',choices=['python','javascript','java','cpp','elixir','rust','cli','extension'])
    parser.add_argument('--catalog',type=Path,default=ROOT.parent/'orchid-catalog')
    args=parser.parse_args()
    if 'AUTH_SMOKE_URL' in os.environ:
        clients(args.only)
        return
    if not args.no_build:
        build_native_clients()
        run(['cargo','build','--locked','--manifest-path',args.catalog/'Cargo.toml'])
        run([sys.executable,ROOT/'extension/scripts/build.py','--skip-rust'])
        run(['npm','install','--ignore-scripts','--no-audit','--no-fund','--package-lock=false','--prefix','clients/js'])
        run(['npm','run','build','--prefix','clients/js'])
        java_env=os.environ.copy()
        if platform.system()=='Darwin':
            java_env['JAVA_HOME']=subprocess.check_output(['/usr/libexec/java_home','-F','-v','21'],text=True).strip()
        subprocess.run(['mvn','-pl','orchiddb-java','-am','-Dmaven.test.skip=true','compile'],cwd=ROOT/'clients/java',env=java_env,check=True)
        subprocess.run(['mvn','-q','-pl','orchiddb-java','dependency:build-classpath','-DincludeScope=runtime','-Dmdep.outputFile='+str(ROOT/'clients/java/target/native-smoke-classpath.txt')],cwd=ROOT/'clients/java',env=java_env,check=True)
    command=[sys.executable,args.catalog/'tools/auth_smoke.py','--clients',sys.executable,Path(__file__).resolve(),'--no-build']
    if args.only:
        command.extend(['--only',*args.only])
    run(command)


if __name__=='__main__':
    main()
