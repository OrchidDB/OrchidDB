#!/usr/bin/env python3
"""One local Cargo build per platform, sharing dependencies across all outputs."""
import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib
import urllib.request

from local_workspace import prepare, selection

P = {
    'macos-aarch64': None,
    'macos-x86_64': 'x86_64-apple-darwin',
    'linux-x86_64': 'x86_64-unknown-linux-gnu',
    'linux-aarch64': 'aarch64-unknown-linux-gnu',
}
REPOS = {'engine': 'orchiddb', 'native': 'orchiddb-native', 'java': 'orchiddb-java',
         'cli': 'orchiddb-cli', 'rust': 'orchiddb-rust', 'python': 'orchiddb-python',
         'javascript': 'orchiddb-js', 'elixir': 'orchiddb-elixir', 'cpp': 'orchiddb-cpp'}
KINDS = ['native', 'java', 'cli']


def run(argv, env, cwd=None):
    print('+ ' + ' '.join(map(str, argv)), flush=True)
    subprocess.run(list(map(str, argv)), env=env, cwd=cwd, check=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command_for(platform, manifest):
    command = ['cargo', 'zigbuild' if platform.startswith('linux') else 'build',
               '--locked', '--release', '--manifest-path', str(manifest)]
    target = P[platform]
    if target:
        command += ['--target', target + ('.2.34' if platform.startswith('linux') else '')]
    return command + selection()


def outputs(workspace, platform):
    cache = workspace / 'target' / ('local-linux' if platform.startswith('linux') else 'native-release')
    output = cache / (P[platform] or '') / 'release'
    extension = '.so' if platform.startswith('linux') else '.dylib'
    return cache, {'native': output / ('liborchiddb_compiler' + extension),
                   'java': output / ('liborchiddb_java' + extension), 'cli': output / 'orchiddb'}


def driver_setup(workspace, sources, cache, platform, env):
    spec = importlib.util.spec_from_file_location('cli_build', workspace / 'orchiddb-cli/scripts/release/build.py')
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    cargo = tomllib.loads((sources / 'orchiddb-cli/Cargo.toml').read_text())
    if cargo['dependencies']['duckdb'] != '=' + driver.DRIVER_CRATE:
        raise SystemExit('DuckDB build tooling does not match pinned CLI source')
    folder = cache / 'duckdb-static' / driver.DRIVER_VERSION / platform
    folder.mkdir(parents=True, exist_ok=True)
    name, expected = driver.ARCHIVES[platform]
    archive = folder / name
    url = f'https://github.com/duckdb/duckdb/releases/download/v{driver.DRIVER_VERSION}/{name}'
    if not archive.exists():
        temporary = archive.with_suffix('.download')
        urllib.request.urlretrieve(url, temporary)
        temporary.replace(archive)
    driver.unpack(archive, expected, folder / 'input')
    combined = folder / 'combined'
    combined.mkdir(exist_ok=True)
    library = combined / 'libduckdb_static.a'
    if not library.exists():
        libraries = sorted((folder / 'input').glob('*.a'))
        if platform.startswith('macos'):
            run(['/usr/bin/libtool', '-static', '-o', library, *libraries], env)
        else:
            ar = '/opt/homebrew/opt/llvm/bin/llvm-ar'
            if not Path(ar).exists():
                ar = shutil.which('llvm-ar', path=env['PATH']) or 'ar'
            commands = ['create ' + str(library), *('addlib ' + str(p) for p in libraries), 'save', 'end']
            subprocess.run([ar, '-M'], input='\n'.join(commands) + '\n', text=True, env=env, check=True)
    header = combined / 'duckdb.h'
    if not header.exists():
        header.write_bytes((folder / 'input/duckdb.h').read_bytes())
    env.update(DUCKDB_STATIC='1', DUCKDB_LIB_DIR=str(combined))
    if platform.startswith('linux'):
        arch = 'aarch64' if platform.endswith('aarch64') else 'x86_64'
        sdk = workspace / 'target/cross-sdk' / ('linux-arm64' if arch == 'aarch64' else 'linux')
        cpp = sdk / 'sysroot/usr/lib/gcc' / (arch + '-linux-gnu') / '12'
        if not (cpp / 'libstdc++.a').exists():
            run(['bash', Path(__file__).with_name('local_linux_sdk.sh'), workspace, arch], env)
        env['ORCHIDDB_CLI_CXX_DIR'] = str(cpp)
    return {'version': driver.DRIVER_VERSION, 'linkage': 'static', 'archive': url, 'sha256': expected}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--workspace', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--platform', choices=P)
    parser.add_argument('--plan', action='store_true', help='Print build commands without executing them')
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    release = workspace / '.releases' / args.version
    state = json.loads((release / 'state.json').read_text())
    if any(state['validation'].get(kind, {}).get('exit_code') != 0 for kind in ['core', 'clients']):
        raise SystemExit('Successful local validation is required')
    sources = release / 'local-source'
    sources.mkdir(exist_ok=True, parents=True)
    for kind, name in REPOS.items():
        source = sources / name
        if not source.exists():
            if args.plan:
                raise SystemExit('Missing pinned source checkout: ' + str(source))
            run(['git', 'worktree', 'add', '--detach', source, state['pins'][kind]], os.environ, workspace / name)
        head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
        dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=source, text=True).strip()
        if head != state['pins'][kind] or dirty:
            raise SystemExit('Release source must match the clean tested revision: ' + str(source))
    manifest = prepare(sources, release / 'build-workspace')
    platforms = [args.platform] if args.platform else list(P)
    if args.plan:
        for platform in platforms:
            print(platform + ': ' + ' '.join(command_for(platform, manifest)))
        return
    receipt = release / 'local-builds'
    receipt.mkdir(exist_ok=True)
    signature = {'pins': state['pins'], 'lock': digest(manifest.with_name('Cargo.lock')),
                 'builder': digest(Path(__file__)),
                 'workspace_tool': digest(Path(__file__).with_name('local_workspace.py')),
                 'driver_tool': digest(workspace / 'orchiddb-cli/scripts/release/build.py'),
                 'sdk_tool': digest(Path(__file__).with_name('local_linux_sdk.sh'))}

    def build(platform):
        cache, binaries = outputs(workspace, platform)
        records = {kind: receipt / f'{kind}-{platform}.json' for kind in KINDS}
        def reusable(kind):
            if not records[kind].exists() or not binaries[kind].exists():
                return False
            old = json.loads(records[kind].read_text())
            if old.get('signature') != signature or old['sha256'] != digest(binaries[kind]):
                return False
            metadata = binaries[kind].parent / 'BUILD.json'
            return kind != 'cli' or (metadata.exists() and old.get('metadata_sha256') == digest(metadata))
        if all(reusable(kind) for kind in KINDS):
            print('Reuse all outputs: ' + platform, flush=True)
            return
        env = dict(os.environ, RUSTUP_TOOLCHAIN='1.93.1', CARGO_TARGET_DIR=str(cache),
                   CARGO_BUILD_JOBS=os.environ.get('LOCAL_BUILD_JOBS', '2'), ORCHIDDB_RELEASE_BUILD='1')
        env['PATH'] = '/opt/homebrew/bin:' + str(workspace / '.releases/tooling/bin') + ':' + env.get('PATH', '')
        toolchain = Path.home() / '.rustup/toolchains/1.93.1-aarch64-apple-darwin/bin'
        if toolchain.exists():
            env['PATH'] = str(toolchain) + ':' + env['PATH']
        if platform.startswith('macos'):
            env['MACOSX_DEPLOYMENT_TARGET'] = '11.0'
        if P[platform]:
            run(['rustup', 'target', 'add', P[platform], '--toolchain', '1.93.1'], env)
        driver = driver_setup(workspace, sources, cache, platform, env)
        command = command_for(platform, manifest)
        # Always select all three packages: feature unification and the release
        # profile stay identical when resuming partially completed output.
        run(command, env, manifest.parent)
        metadata = binaries['cli'].parent / 'BUILD.json'
        metadata.write_text(json.dumps({'version': args.version, 'platform': platform,
            'source_commit': state['pins']['cli'], 'binary_sha256': digest(binaries['cli']),
            'rust': subprocess.check_output(['rustc', '--version'], env=env, text=True).strip(),
            'duckdb': driver}, indent=2) + '\n')
        for kind, binary in binaries.items():
            record = {'version': args.version, 'platform': platform, 'source_commit': state['pins'][kind],
                      'core_revision': state['pins']['engine'], 'sha256': digest(binary), 'local': True,
                      'binary': str(binary), 'command': command, 'signature': signature}
            if kind == 'cli':
                record['metadata_sha256'] = digest(metadata)
            records[kind].write_text(json.dumps(record, indent=2) + '\n')

    def group(items):
        for platform in items:
            build(platform)
    groups = [[args.platform]] if args.platform else [['macos-aarch64', 'macos-x86_64'], ['linux-x86_64', 'linux-aarch64']]
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(groups)) as pool:
        for _ in pool.map(group, groups):
            pass


if __name__ == '__main__':
    main()
