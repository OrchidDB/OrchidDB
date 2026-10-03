#!/usr/bin/env python3
"""Coordinate local validation and resumable artifact builds across OrchidDB repositories."""
import argparse
import base64
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import struct
import tarfile
import tempfile
import tomllib
import uuid
import xml.etree.ElementTree as ET
import zipfile
import urllib.request

REPOS = {name: 'orchiddb' + suffix for name, suffix in {
    'engine': '', 'native': '-native', 'rust': '-rust', 'cli': '-cli',
    'java': '-java', 'python': '-python', 'javascript': '-js',
    'elixir': '-elixir', 'cpp': '-cpp',
}.items()}
PLATFORMS = {
    'linux-x86_64': 'x86_64-unknown-linux-gnu',
    'macos-aarch64': 'aarch64-apple-darwin',
    'macos-x86_64': 'x86_64-apple-darwin',
    'windows-x86_64': 'x86_64-pc-windows-msvc',
}
FAMILIES = {'native': ('native.yml', list(PLATFORMS)),
            'java': ('release.yml', list(PLATFORMS)),
            'cli': ('release.yml', list(PLATFORMS)[:3]),
            'rust': ('release.yml', ['sources']),
            'elixir': ('release.yml', ['sources'])}
CLIENTS = ['python', 'javascript', 'cpp']
TEST_COMMAND = re.compile(r'\bcargo(?:\s+\+\S+)?\s+test\b|\bnpm\s+(?:run\s+)?test\b|\b(?:pytest|ctest)\b|\bmix\s+test\b|\bnode\s+--test\b|scripts/(?:test\.sh|smoke\.py)|package-workspace\.py[^\n]*--test')


def command(*args, cwd=None):
    return subprocess.check_output(list(map(str, args)), cwd=cwd, text=True).strip()


def github(kind):
    return 'OrchidDB/' + REPOS[kind].replace('orchiddb', 'OrchidDB')


def api(path):
    return json.loads(command('gh', 'api', path))


def save(path, state):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(state, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def blob(repo, commit, name):
    return command('git', 'show', f'{commit}:{name}', cwd=repo)


def package_version(repo, commit, kind):
    if kind in ['engine', 'native', 'rust', 'cli']:
        return tomllib.loads(blob(repo, commit, 'Cargo.toml'))['package']['version']
    if kind == 'java':
        return ET.fromstring(blob(repo, commit, 'pom.xml')).findtext('{http://maven.apache.org/POM/4.0.0}version')
    if kind == 'python':
        return tomllib.loads(blob(repo, commit, 'pyproject.toml'))['project']['version']
    if kind == 'javascript':
        return json.loads(blob(repo, commit, 'package.json'))['version']
    name, pattern = ('mix.exs', r'version:\s*"([^"]+)"') if kind == 'elixir' else ('CMakeLists.txt', r'project\(OrchidDB VERSION ([^ ]+)')
    return re.search(pattern, blob(repo, commit, name))[1]


def audit_text(text, label):
    if TEST_COMMAND.search(text):
        raise ValueError(f'GitHub runtime tests are forbidden: {label}')
    if re.search(r'^\s+tags:', text, re.M):
        raise ValueError(f'Release tags must not start duplicate builds: {label}')
    for line in text.splitlines():
        if re.search(r'\bmvn\b.*\b(?:verify|install|deploy|package)\b|build(?:-gremlin)?\.sh', line):
            if '-DskipTests' not in line:
                raise ValueError(f'Maven build must explicitly skip tests: {label}: {line.strip()}')


def audit_workflows(workspace):
    for repo in REPOS.values():
        for path in (workspace / repo / '.github/workflows').glob('*.y*ml'):
            audit_text(path.read_text(), path)


def audit_remote_workflows(state):
    """Audit the actual remote definitions, not an unpushed local edit."""
    revisions = {}
    for kind in REPOS:
        revision = api(f'repos/{github(kind)}/commits/main')['sha']
        revisions[kind] = revision
        files = api(f'repos/{github(kind)}/contents/.github/workflows?ref={revision}')
        for item in files:
            if item['name'].endswith(('.yml', '.yaml')):
                data = api(f'repos/{github(kind)}/contents/{item["path"]}?ref={revision}')
                audit_text(base64.b64decode(data['content']).decode(), github(kind) + '/' + item['path'])
    state['workflow_revisions'] = revisions


def reject_blocked_quota(report, today=None):
    today = today or dt.date.today()
    for warning in report.get('warnings', []):
        if 'publishing is blocked' not in warning.lower():
            continue
        reset = re.search(r'resets on ([A-Za-z]+ \d{1,2}, \d{4})', warning)
        if reset and dt.datetime.strptime(reset[1], '%B %d, %Y').date() <= today:
            continue
        raise ValueError('Maven Central quota preflight: ' + warning)


def check_maven_quota(workspace):
    """Refresh a previous deployment's account warnings before starting expensive work."""
    reports = list(workspace.glob('release-*/maven-deployment.json')) + list((workspace / '.releases').glob('*/maven-deployment.json'))
    if not reports:
        return
    path = max(reports, key=lambda p: p.stat().st_mtime)
    report = json.loads(path.read_text())
    settings = Path.home() / '.m2/settings.xml'
    if settings.exists():
        ns = {'m': 'http://maven.apache.org/SETTINGS/1.0.0'}
        servers = ET.parse(settings).getroot().findall('m:servers/m:server', ns)
        central = next((s for s in servers if s.findtext('m:id', namespaces=ns) == 'central'), None)
        if central is not None:
            deployment = str(uuid.UUID(report['deploymentId']))
            user = central.findtext('m:username', namespaces=ns)
            password = central.findtext('m:password', namespaces=ns)
            token = base64.b64encode(f'{user}:{password}'.encode()).decode()
            request = urllib.request.Request('https://central.sonatype.com/api/v1/publisher/status?id=' + deployment,
                                             method='POST', headers={'Authorization': 'Bearer ' + token})
            with urllib.request.urlopen(request, timeout=30) as response:
                report = json.load(response)
    reject_blocked_quota(report)


def validate_pins(workspace, pins, version):
    def read(kind, name):
        return blob(workspace / REPOS[kind], pins[kind], name)
    for kind in REPOS:
        if package_version(workspace / REPOS[kind], pins[kind], kind) != version:
            raise ValueError(f'{kind}: package version differs from {version}')
    if read('native', 'CORE_REVISION') != pins['engine'] or read('java', 'native/CORE_REVISION') != pins['engine']:
        raise ValueError('Native/JNI core pins do not match the engine release')
    rust = tomllib.loads(read('rust', 'Cargo.toml'))['dependencies']['orchiddb']
    if rust['rev'] != pins['engine'] or rust['version'] != '=' + version:
        raise ValueError('Rust client engine pin/version mismatch')
    cli = tomllib.loads(read('cli', 'Cargo.toml'))['dependencies']['orchiddb-client']
    if cli['rev'] != pins['rust']:
        raise ValueError('CLI Rust-client pin mismatch')
    for kind in ['python', 'javascript', 'cpp', 'elixir']:
        if read(kind, 'NATIVE_REVISION') != pins['native']:
            raise ValueError(f'{kind}: shared compiler revision mismatch')
    for kind in ['python', 'elixir']:
        if read(kind, 'CORE_REVISION') != pins['engine']:
            raise ValueError(f'{kind}: engine revision mismatch')


def make_plan(workspace, version):
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('Expected a stable X.Y.Z release version without the v prefix')
    pins = {}
    for kind, directory in REPOS.items():
        repo = workspace / directory
        if command('git', 'status', '--porcelain', cwd=repo):
            raise ValueError(f'{directory}: commit intended changes before planning a release')
        pins[kind] = command('git', 'rev-parse', 'HEAD', cwd=repo)
        tag = subprocess.run(['git', 'rev-parse', '--verify', f'refs/tags/v{version}^{{commit}}'], cwd=repo, capture_output=True, text=True)
        if tag.returncode == 0 and tag.stdout.strip() != pins[kind]:
            raise ValueError(f'{directory}: existing version tag identifies different source; never move it')
    validate_pins(workspace, pins, version)
    audit_workflows(workspace)
    check_maven_quota(workspace)
    return {'schema': 1, 'version': version, 'workspace': str(workspace), 'pins': pins,
            'validation': {}, 'builds': {}, 'created_at': dt.datetime.now(dt.timezone.utc).isoformat()}


def validation_current(state):
    validation = state.get('validation', {})
    return (validation.get('pins') == state['pins'] and
            all(validation.get(name, {}).get('exit_code') == 0 for name in ['core', 'clients']))


def validate_locally(state, path):
    workspace = Path(state['workspace'])
    for kind, directory in REPOS.items():
        repo = workspace / directory
        if command('git', 'rev-parse', 'HEAD', cwd=repo) != state['pins'][kind] or command('git', 'status', '--porcelain', cwd=repo):
            raise ValueError('Source changed since planning; create a new plan before testing')
    for variable in ['ORCHIDDB_TEST_PG_URL', 'ORCHIDDB_TEST_PG_URI', 'ORCHIDDB_TEST_PG_JDBC']:
        if not os.environ.get(variable):
            raise ValueError('Set ' + variable + ' so PostgreSQL integration coverage is not silently skipped')
    env = dict(os.environ)
    env.setdefault('CARGO_TARGET_DIR', str(workspace / 'target/integration'))
    env.setdefault('CARGO_PROFILE_DEV_DEBUG', '0')
    env.setdefault('RUST_MIN_STACK', '16777216')
    commands = {'core': ['cargo', 'test', '--locked', '--manifest-path', str(workspace / 'orchiddb/Cargo.toml')],
                'clients': ['make', '-f', str(workspace / 'orchiddb/scripts/release/integration.mk'), 'test']}
    if state['validation'].get('pins') != state['pins']:
        state['validation'] = {'pins': state['pins'].copy()}
    for name, argv in commands.items():
        previous = state['validation'].get(name, {})
        log = path.parent / f'{name}-tests.log'
        if previous.get('exit_code') == 0 and log.exists() and hashlib.sha256(log.read_bytes()).hexdigest() == previous.get('log_sha256'):
            print(name + ': reuse successful local validation', flush=True)
            continue
        print(name + ': running locally; output: ' + str(log), flush=True)
        with log.open('w') as stream:
            result = subprocess.run(argv, cwd=workspace, env=env, stdout=stream, stderr=subprocess.STDOUT)
        state['validation'][name] = {'exit_code': result.returncode, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
        save(path, state)
        if result.returncode:
            raise ValueError(f'{name} validation failed; no release builds were started. See {log}')


def request_key(state, kind, platform, attempt):
    data = json.dumps([state['version'], state['pins'], kind, platform, attempt], sort_keys=True).encode()
    return hashlib.sha256(data).hexdigest()[:24]


def find_run(kind, workflow, request_id):
    runs = json.loads(command('gh', 'run', 'list', '--repo', github(kind), '--workflow', workflow,
                              '--event', 'workflow_dispatch', '--limit', '100', '--json', 'databaseId,displayTitle,status,conclusion,url,headSha'))
    found = [r for r in runs if request_id in r['displayTitle']]
    if len(found) > 1:
        raise ValueError('Duplicate dispatch request found; inspect runs before proceeding')
    return found[0] if found else None


def dispatch(state, path, kind, platform, workflow, inputs, retry=False):
    key = kind + '/' + platform
    entry = state['builds'].get(key)
    if entry and entry.get('artifacts'):
        return
    if entry and entry.get('run_id'):
        run = api(f'repos/{github(kind)}/actions/runs/{entry["run_id"]}')
        if run['status'] != 'completed' or run['conclusion'] == 'success':
            return
        if not retry:
            return
        # Preserve earlier run IDs and their artifacts; retry just this platform.
        attempts = entry.get('attempts', []) + [entry['run_id']]
        entry = {'attempt': entry.get('attempt', 0) + 1, 'attempts': attempts}
    if not entry:
        entry = {'attempt': 0, 'attempts': []}
    entry.setdefault('request_id', request_key(state, kind, platform, entry['attempt']))
    entry.update({'workflow': workflow, 'inputs': inputs})
    entry.setdefault('workflow_sha', state['workflow_revisions'][kind])
    state['builds'][key] = entry
    save(path, state)  # Persist identity before the network mutation.
    existing = find_run(kind, workflow, entry['request_id'])
    if existing:
        if existing['headSha'] != entry['workflow_sha']:
            raise ValueError('Workflow revision changed; inspect and re-audit before resuming')
        entry['run_id'] = existing['databaseId']
        entry['url'] = existing['url']
        save(path, state)
        return
    if entry.get('dispatch_started'):
        raise ValueError(f'{key}: dispatch result is uncertain. Check Actions; do not dispatch another build blindly')
    entry['dispatch_started'] = True
    save(path, state)
    argv = ['gh', 'workflow', 'run', workflow, '--repo', github(kind), '--ref', 'main']
    for name, value in {**inputs, 'request_id': entry['request_id']}.items():
        argv += ['-f', name + '=' + str(value)]
    result = subprocess.run(argv, capture_output=True, text=True)
    if result.returncode:
        if re.search(r'HTTP (?:400|401|403|404|422)', result.stderr):
            entry['dispatch_started'] = False  # The server definitively rejected this request.
            save(path, state)
        raise ValueError(key + ': ' + result.stderr.strip())
    existing = find_run(kind, workflow, entry['request_id'])
    if existing:
        if existing['headSha'] != entry['workflow_sha']:
            subprocess.run(['gh','run','cancel',str(existing['databaseId']),'--repo',github(kind)],check=True)
            raise ValueError('Workflow main changed during dispatch; cancelled the unaudited run')
        entry.update(run_id=existing['databaseId'], url=existing['url'])
    save(path, state)
    print(key + ': dispatched ' + entry['request_id'], flush=True)


def ensure_tags(state, path):
    workspace = Path(state['workspace'])
    tag = 'v' + state['version']
    for kind, directory in REPOS.items():
        repo = workspace / directory
        existing = subprocess.run(['git', 'rev-parse', '--verify', f'refs/tags/{tag}^{{commit}}'], cwd=repo, capture_output=True, text=True)
        if existing.returncode:
            subprocess.run(['git', 'tag', '-a', tag, state['pins'][kind], '-m', 'OrchidDB ' + tag], cwd=repo, check=True)
        elif existing.stdout.strip() != state['pins'][kind]:
            raise ValueError(f'{kind}: tag drift; refusing to build different source')
        if state.setdefault('pushed_tags', {}).get(kind) != state['pins'][kind]:
            subprocess.run(['git', 'push', 'origin', 'refs/tags/' + tag], cwd=repo, check=True)
            state['pushed_tags'][kind] = state['pins'][kind]
            save(path, state)


def artifact_names(kind, platform):
    if kind == 'native':
        return ['compiler-' + PLATFORMS[platform]]
    if kind == 'java':
        return ['native-' + platform]
    if kind == 'cli':
        return ['package-' + platform]
    if kind == 'rust':
        return ['verified-crates']
    if kind == 'elixir':
        return ['hex-package']
    if kind == 'javascript':
        return ['npm-package']
    systems = ['ubuntu-22.04', 'macos-14', 'macos-15-intel']
    return [('wheels-' if kind == 'python' else 'package-') + os for os in systems + (['windows-2022'] if kind == 'python' else [])]


def check_binary(data, platform):
    if platform.startswith('linux'):
        valid = data[:4] == b'\x7fELF' and struct.unpack_from('<H', data, 18)[0] == 62
    elif platform.startswith('windows'):
        offset = struct.unpack_from('<I', data, 60)[0]
        valid = data[:2] == b'MZ' and data[offset:offset+4] == b'PE\0\0' and struct.unpack_from('<H', data, offset+4)[0] == 0x8664
    else:
        valid = data[:4] == b'\xcf\xfa\xed\xfe' and struct.unpack_from('<I', data, 4)[0] == (0x100000c if platform.endswith('aarch64') else 0x1000007)
    if not valid:
        raise ValueError('Binary architecture differs from ' + platform)


def validate_artifact(folder, state, kind, platform):
    if (folder / 'SOURCE_COMMIT').read_text().strip() != state['pins'][kind]:
        raise ValueError('Artifact source revision differs from release plan')
    if kind == 'native':
        archives = list(folder.glob('*.tar.gz'))
        if len(archives) != 1:
            raise ValueError('Expected exactly one compiler archive')
        with tarfile.open(archives[0]) as stream:
            meta = json.load(stream.extractfile('manifest.json'))
            data = stream.extractfile(meta['library']).read()
        check_binary(data, platform)
        if (meta['version'] != state['version'] or meta['core_revision'] != state['pins']['engine'] or
                meta['target'] != PLATFORMS[platform] or hashlib.sha256(data).hexdigest() != meta['sha256']):
            raise ValueError('Native artifact version, platform or checksum mismatch')
    elif kind == 'java':
        with zipfile.ZipFile(folder / (platform + '.jar')) as stream:
            prefix = 'io/orchiddb/native/' + platform + '/'
            props = dict(line.split('=', 1) for line in stream.read(prefix + 'build.properties').decode().splitlines())
            name = next(n for n in stream.namelist() if n.startswith(prefix) and n.endswith(('.so', '.dylib', '.dll')))
            check_binary(stream.read(name), platform)
            if props['version'] != state['version'] or props['coreRevision'] != state['pins']['engine'] or props['sha256'] != hashlib.sha256(stream.read(name)).hexdigest():
                raise ValueError('JNI artifact metadata/checksum mismatch')
    elif kind == 'rust':
        if (folder / 'ENGINE_COMMIT').read_text().strip() != state['pins']['engine']:
            raise ValueError('Registry archive engine revision mismatch')
        for package in ['orchiddb', 'orchiddb-client']:
            if not (folder / f'{package}-{state["version"]}.crate').is_file():
                raise ValueError('Missing registry archive: ' + package)
    elif kind == 'cli':
        with tarfile.open(folder / f'orchiddb-v{state["version"]}-{platform}.tar.gz') as stream:
            files = {Path(m.name).name: m for m in stream.getmembers() if m.isfile()}
            meta = json.load(stream.extractfile(files['BUILD.json']))
            data = stream.extractfile(files['orchiddb']).read()
            check_binary(data, platform)
            if meta['source_commit'] != state['pins']['cli'] or meta['platform'] != platform or meta['version'] != state['version'] or meta['binary_sha256'] != hashlib.sha256(data).hexdigest():
                raise ValueError('CLI artifact metadata/checksum mismatch')
    elif kind == 'python':
        wheels = list(folder.glob('*.whl'))
        seen = set()
        for wheel in wheels:
            target = ('windows-x86_64' if 'win_amd64' in wheel.name else
                      'linux-x86_64' if 'manylinux' in wheel.name else
                      'macos-aarch64' if 'arm64' in wheel.name else 'macos-x86_64')
            seen.add(target)
            with zipfile.ZipFile(wheel) as stream:
                metadata = next(n for n in stream.namelist() if n.endswith('.dist-info/METADATA'))
                if '\nVersion: ' + state['version'] + '\n' not in stream.read(metadata).decode():
                    raise ValueError('Wheel version mismatch')
                if stream.read('orchiddb/CORE_REVISION').decode().strip() != state['pins']['engine']:
                    raise ValueError('Wheel core revision mismatch')
                libraries = [n for n in stream.namelist() if n.startswith('orchiddb/native/') and n.endswith(('.so', '.dylib', '.dll'))]
                if len(libraries) != 1:
                    raise ValueError('Expected one compiler per wheel')
                check_binary(stream.read(libraries[0]), target)
        if seen != set(PLATFORMS) or len(wheels) != 4:
            raise ValueError('All four platform wheels are required')
    elif kind == 'javascript':
        archives = list(folder.glob('*.tgz'))
        if len(archives) != 1:
            raise ValueError('Expected one npm archive')
        with tarfile.open(archives[0]) as stream:
            if json.load(stream.extractfile('package/package.json'))['version'] != state['version']:
                raise ValueError('npm version mismatch')
            for name, target in [('linux-x64', 'linux-x86_64'), ('darwin-arm64', 'macos-aarch64'), ('darwin-x64', 'macos-x86_64')]:
                prefix = 'package/native/' + name + '/'
                meta = json.load(stream.extractfile(prefix + 'manifest.json'))
                library = prefix + 'liborchiddb_compiler.' + ('so' if target.startswith('linux') else 'dylib')
                data = stream.extractfile(library).read()
                check_binary(data, target)
                if meta['version'] != state['version'] or meta['core_revision'] != state['pins']['engine'] or meta['sha256'] != hashlib.sha256(data).hexdigest():
                    raise ValueError('npm compiler metadata mismatch')
    elif kind == 'cpp':
        for name, target in [('Linux-x86_64', 'linux-x86_64'), ('Darwin-arm64', 'macos-aarch64'), ('Darwin-x86_64', 'macos-x86_64')]:
            with tarfile.open(folder / f'orchiddb-cpp-{state["version"]}-{name}.tar.gz') as stream:
                library = next(m for m in stream.getmembers() if m.isfile() and Path(m.name).name in ('liborchiddb_compiler.so', 'liborchiddb_compiler.dylib'))
                check_binary(stream.extractfile(library).read(), target)
    elif kind == 'elixir':
        with tarfile.open(folder / 'orchiddb.tar') as stream:
            metadata = stream.extractfile('metadata.config').read().decode()
            if not re.search(r'\{<<"version">>,\s*<<"' + re.escape(state['version']) + r'">>\}', metadata):
                raise ValueError('Hex package version mismatch')


def collect(state, path):
    for key, entry in state['builds'].items():
        kind, platform = key.split('/')
        if entry.get('artifacts'):
            for name, digest in entry['artifacts'].items():
                file = path.parent / 'artifacts' / kind / platform / name
                if not file.is_file() or hashlib.sha256(file.read_bytes()).hexdigest() != digest:
                    raise ValueError('Cached release artifact changed: ' + str(file))
            continue
        if not entry.get('run_id'):
            found = find_run(kind, entry['workflow'], entry['request_id'])
            if not found:
                continue
            if found['headSha'] != entry['workflow_sha']:
                raise ValueError('Recovered run used an unaudited workflow revision')
            entry.update(run_id=found['databaseId'], url=found['url'])
            save(path, state)
        available = api(f'repos/{github(kind)}/actions/runs/{entry["run_id"]}/artifacts?per_page=100')['artifacts']
        names = artifact_names(kind, platform)
        if not set(names) <= {a['name'] for a in available if not a['expired']}:
            continue
        destination = path.parent / 'artifacts' / kind / platform
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists():
            # Recover a process interruption after the directory rename but before state save.
            validate_artifact(destination, state, kind, platform)
        else:
            with tempfile.TemporaryDirectory(dir=destination.parent) as temporary:
                temporary = Path(temporary)
                combined = temporary / 'combined'
                combined.mkdir()
                for name in names:
                    part = temporary / name
                    subprocess.run(['gh', 'run', 'download', str(entry['run_id']), '--repo', github(kind), '--name', name, '--dir', str(part)], check=True)
                    if (part / 'SOURCE_COMMIT').read_text().strip() != state['pins'][kind]:
                        raise ValueError('Downloaded artifact source mismatch')
                    for file in part.iterdir():
                        if not file.is_file():
                            raise ValueError('Unexpected artifact subdirectory: ' + str(file))
                        output = combined / file.name
                        if output.exists() and output.read_bytes() != file.read_bytes():
                            raise ValueError('Conflicting artifact files: ' + file.name)
                        output.write_bytes(file.read_bytes())
                validate_artifact(combined, state, kind, platform)
                combined.rename(destination)
        entry['artifacts'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in destination.iterdir() if p.is_file()}
        save(path, state)
        print(key + ': collected and verified', flush=True)


def build(state, path, retry=False):
    if not validation_current(state):
        raise ValueError('Successful local validation for these exact source revisions is required before GitHub builds')
    for name in ['core', 'clients']:
        log = path.parent / f'{name}-tests.log'
        if not log.is_file() or hashlib.sha256(log.read_bytes()).hexdigest() != state['validation'][name]['log_sha256']:
            raise ValueError('Local validation evidence is missing or changed: '+name)
    audit_workflows(Path(state['workspace']))
    collect(state, path)
    audit_remote_workflows(state)
    save(path, state)
    ensure_tags(state, path)
    tag = 'v' + state['version']
    for kind, (workflow, platforms) in FAMILIES.items():
        for platform in platforms:
            inputs = {'ref': tag} if kind == 'rust' else ({'tag': tag} if kind == 'elixir' else {'tag': tag, 'platform': platform})
            dispatch(state, path, kind, platform, workflow, inputs, retry)
    native = {p: state['builds'].get('native/' + p, {}) for p in PLATFORMS}
    if all(entry.get('artifacts') for entry in native.values()):
        runs = json.dumps({p: entry['run_id'] for p, entry in native.items()}, separators=(',', ':'))
        for kind in CLIENTS:
            dispatch(state, path, kind, 'packages', 'release.yml', {'tag': tag, 'native_runs': runs}, retry)
    else:
        print('Client packaging waits for the shared compiler. Re-run build after collection; no completed build is repeated.')


def verify(state, path):
    """Write the publication handoff only when all required artifacts are present."""
    if not validation_current(state):
        raise ValueError('Local validation has not passed')
    for name in ['core', 'clients']:
        log = path.parent / f'{name}-tests.log'
        if not log.is_file() or hashlib.sha256(log.read_bytes()).hexdigest() != state['validation'][name]['log_sha256']:
            raise ValueError('Local validation evidence is missing or changed: ' + name)
    collect(state, path)
    expected = [f'{kind}/{platform}' for kind, (_, platforms) in FAMILIES.items() for platform in platforms]
    expected += [kind + '/packages' for kind in CLIENTS]
    missing = [key for key in expected if not state['builds'].get(key, {}).get('artifacts')]
    if missing:
        raise ValueError('Release is incomplete: ' + ', '.join(missing))
    for key in expected:
        kind, platform = key.split('/')
        validate_artifact(path.parent / 'artifacts' / kind / platform, state, kind, platform)
    save(path.parent / 'verified-release.json', state)
    print('All required platforms verified. Publication handoff:', path.parent / 'verified-release.json')


def status(state):
    print('Release', state['version'], '| local validation:', 'passed' if validation_current(state) else 'pending/failed')
    for key, entry in state['builds'].items():
        if entry.get('artifacts'):
            print(key.ljust(28), 'READY (verified local artifacts)')
        elif entry.get('run_id'):
            kind = key.split('/')[0]
            run = api(f'repos/{github(kind)}/actions/runs/{entry["run_id"]}')
            started = dt.datetime.fromisoformat(run['created_at'].replace('Z', '+00:00'))
            ended = dt.datetime.fromisoformat(run['updated_at'].replace('Z', '+00:00')) if run['status'] == 'completed' else dt.datetime.now(dt.timezone.utc)
            minutes = int((ended - started).total_seconds() / 60)
            label = run['conclusion'] or run['status']
            print(key.ljust(28), f'{label:12} {minutes:3}m', run['html_url'])
        else:
            print(key.ljust(28), 'dispatch pending; resume discovers the request before sending another')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['plan', 'test', 'build', 'retry', 'collect', 'verify', 'status', 'audit', 'quota'])
    parser.add_argument('--version', required=True)
    parser.add_argument('--workspace', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--state', type=Path)
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    path = args.state or workspace / '.releases' / args.version / 'state.json'
    try:
        if args.command == 'quota':
            check_maven_quota(workspace)
            print('No active Maven Central quota block reported.')
            return
        if args.command == 'audit':
            audit_workflows(workspace)
            print('Workflow policy passed: builds/packaging/publication only; no automatic tag builds.')
            return
        if args.command == 'plan':
            if path.exists():
                raise ValueError('Release state already exists; use status/build/retry to resume')
            save(path, make_plan(workspace, args.version))
            print('Saved immutable release plan:', path)
            return
        state = json.loads(path.read_text())
        if state['version'] != args.version or Path(state['workspace']) != workspace:
            raise ValueError('State belongs to another version or workspace')
        if args.command == 'test':
            validate_locally(state, path)
        elif args.command in ['build', 'retry']:
            build(state, path, args.command == 'retry')
        elif args.command == 'collect':
            collect(state, path)
        elif args.command == 'verify':
            verify(state, path)
        else:
            status(state)
    except (ValueError, subprocess.CalledProcessError, FileNotFoundError) as error:
        raise SystemExit(str(error)) from error


if __name__ == '__main__':
    main()
