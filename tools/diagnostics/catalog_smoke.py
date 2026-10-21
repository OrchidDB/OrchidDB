import argparse
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / 'tools/diagnostics/catalog-smoke'
PACKAGES = ['orchiddb-compiler-native', 'orchiddb-java-native', 'orchid-duckdb-compiler']


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


def elixir_env(base):
    env = dict(base, MIX_BUILD_PATH=str(ROOT / 'target/elixir'))
    erl = list(Path('/opt/homebrew/Cellar/erlang').glob('*/lib/erlang/bin')) if platform.system() == 'Darwin' else []
    if erl:
        env['PATH'] = str(max(erl)) + os.pathsep + env['PATH']
    return env


def clients(only):
    env = os.environ.copy()
    library = duckdb_library()
    env['DYLD_LIBRARY_PATH'] = str(library)
    env['LD_LIBRARY_PATH'] = str(library)
    suffix = 'dylib' if platform.system() == 'Darwin' else 'so'
    env['ORCHIDDB_NATIVE_LIBRARY'] = str(ROOT / f'target/debug/liborchiddb_compiler.{suffix}')
    env['PYTHONPATH'] = str(ROOT / 'clients/python/src')
    output = Path(env['AUTH_SMOKE_DIRECTORY'])
    selected = set(only or ['python','javascript','java','cpp','elixir','rust','extension'])
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
        subprocess.run(['mix', 'run', str(SOURCES / 'elixir.exs')],
                       cwd=ROOT / 'clients/elixir', env=elixir_env(env), check=True)
    if 'rust' in selected:
        # Cargo selects a coherent dependency graph; private fingerprint formats
        # and modification times cannot safely identify compatible Rust artifacts.
        run(['cargo', 'run', '--locked', '-p', 'orchiddb-client',
             '--example', 'catalog_smoke', '--features', 'orchid-catalog'],
            env=env | dict(DUCKDB_LIB_DIR=str(library), CARGO_TARGET_DIR=str(ROOT/'target')))
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
    parser.add_argument('--only',nargs='+',choices=['python','javascript','java','cpp','elixir','rust','extension'])
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
    if not args.no_build and (not args.only or 'elixir' in args.only):
        env = elixir_env(os.environ)
        for command in (['mix', 'deps.get'], ['mix', 'compile'],
                        ['mix', 'run', '-e', 'Adbc.download_driver!(:duckdb)']):
            subprocess.run(command, cwd=ROOT / 'clients/elixir', env=env, check=True)
    if not args.no_build and (not args.only or 'rust' in args.only):
        # Complete a cold Rust build before the fixture issues short-lived tokens.
        run(['cargo', 'build', '--locked', '-p', 'orchiddb-client',
             '--example', 'catalog_smoke', '--features', 'orchid-catalog'],
            env=os.environ | dict(DUCKDB_LIB_DIR=str(duckdb_library()), CARGO_TARGET_DIR=str(ROOT/'target')))
    command=[sys.executable,args.catalog/'tools/auth_smoke.py','--clients',sys.executable,Path(__file__).resolve(),'--no-build']
    if args.only:
        command.extend(['--only',*args.only])
    run(command)


if __name__=='__main__':
    main()
