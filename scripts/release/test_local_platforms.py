"""Fast release orchestration checks; no compilation or remote execution."""
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


build = load('local_build')
package = load('local_package')


class PlatformsTest(unittest.TestCase):
    def test_required_platforms(self):
        required = {'linux-aarch64', 'linux-x86_64', 'macos-aarch64', 'macos-x86_64'}
        self.assertEqual(set(build.P), required)
        self.assertEqual(set(package.PLATFORMS), required)
        self.assertEqual(package.PLATFORMS['linux-aarch64'][4], 'linux-arm64')

    def test_elf_architecture_is_checked_not_just_filename(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'liborchiddb_compiler.so'
            for platform, machine in [('linux-aarch64', 183), ('linux-x86_64', 62)]:
                data = bytearray(64)
                data[:6] = b'\x7fELF\x02\x01'
                struct.pack_into('<H', data, 18, machine)
                binary.write_bytes(data)
                package.check_binary(binary, platform)
                other = 'linux-x86_64' if machine == 183 else 'linux-aarch64'
                with self.assertRaisesRegex(AssertionError, 'Wrong ELF architecture'):
                    package.check_binary(binary, other)

    def test_one_command_per_platform_selects_all_outputs(self):
        for platform in build.P:
            command = build.command_for(platform, Path('/release/Cargo.toml'))
            selected = [command[i + 1] for i, arg in enumerate(command) if arg == '--package']
            self.assertEqual(selected, ['orchiddb-compiler-native', 'orchiddb-java-native', 'orchiddb-cli'])
            self.assertIn('--locked', command)
            self.assertIn('--no-default-features', command)
            self.assertNotIn('rustc', command)
            if platform.startswith('linux'):
                self.assertEqual(command[1], 'zigbuild')
                self.assertIn(build.P[platform] + '.2.34', command)
            self.assertEqual(command[-1], build.selection()[-1])

    def test_default_build_runs_four_times_and_reuses_completed_outputs(self):
        with tempfile.TemporaryDirectory() as folder:
            workspace = Path(folder)
            release = workspace / '.releases/1.2.3'
            pins = {kind: 'a' * 40 for kind in package.REPOS}
            for repo in package.REPOS.values():
                (release / 'local-source' / repo).mkdir(parents=True)
            driver = workspace / 'orchiddb-cli/scripts/release/build.py'
            driver.parent.mkdir(parents=True)
            driver.write_text('# fixture driver')
            manifest = release / 'build-workspace/Cargo.toml'
            manifest.parent.mkdir()
            manifest.write_text('[workspace]')
            manifest.with_name('Cargo.lock').write_text('version = 4')
            (release / 'state.json').write_text(json.dumps({
                'pins': pins, 'validation': {'core': {'exit_code': 0}, 'clients': {'exit_code': 0}}}))
            commands = []

            def run(command, env, cwd=None):
                commands.append(command)
                if command[0] != 'cargo':
                    return
                target = command[command.index('--target') + 1].removesuffix('.2.34') if '--target' in command else None
                platform = next(name for name, triple in build.P.items() if triple == target)
                for binary in build.outputs(workspace, platform)[1].values():
                    binary.parent.mkdir(parents=True, exist_ok=True)
                    binary.write_bytes(b'fixture binary')

            def output(command, **kwargs):
                return '' if command[1] == 'status' else 'a' * 40

            with patch.object(sys, 'argv', ['local_build', '--version', '1.2.3', '--workspace', str(workspace)]), \
                 patch.object(build, 'run', side_effect=run), \
                 patch.object(build, 'prepare', return_value=manifest), \
                 patch.object(build, 'driver_setup', return_value={'version': 'fixture'}), \
                 patch.object(build.subprocess, 'check_output', side_effect=output):
                build.main()
                self.assertEqual(len([cmd for cmd in commands if cmd[0] == 'cargo']), 4)
                commands.clear()
                build.main()
                self.assertEqual(commands, [])
                # A missing output resumes the same unified feature graph.
                build.outputs(workspace, 'linux-aarch64')[1]['java'].unlink()
                build.main()
                cargo = [cmd for cmd in commands if cmd[0] == 'cargo']
                self.assertEqual(cargo, [build.command_for('linux-aarch64', manifest)])
            self.assertEqual(len(list((release / 'local-builds').glob('*.json'))), 12)

    def test_linux_wheel_repairs_use_matching_architecture(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            packager = object.__new__(package.Package)
            packager.workspace = root
            packager.output = root / 'artifacts'
            packager.dest('python').mkdir(parents=True)
            packager.core = 'a' * 40
            commands = []

            def stage(kind):
                dest = root / 'stage'
                dest.mkdir(exist_ok=True)
                return dest

            binary = root / 'native.so'
            binary.write_bytes(b'fixture')

            def run(command, *args):
                command = list(map(str, command))
                commands.append(command)
                if 'bdist_wheel' in command:
                    tag = command[command.index('--plat-name') + 1]
                    (packager.dest('python') / f'orchiddb-1.2.3-py3-none-{tag}.whl').write_bytes(b'wheel')
                if command[0] == 'docker':
                    arch = 'aarch64' if command[command.index('--platform') + 1] == 'linux/arm64' else 'x86_64'
                    self.assertIn('manylinux_2_34_' + arch, command[-1])
                    self.assertIn('linux_' + arch + '.whl', command[-1])
                    dest = packager.dest('python') / 'repaired'
                    dest.mkdir()
                    (dest / f'orchiddb-1.2.3-py3-none-manylinux_2_34_{arch}.whl').write_bytes(b'repaired')

            with patch.object(packager, 'staged', side_effect=stage), \
                 patch.object(packager, 'binary', return_value=binary), \
                 patch.object(package, 'check_binary'), patch.object(package, 'run', side_effect=run):
                packager.python()
            self.assertEqual(len(list(packager.dest('python').glob('*.whl'))), 4)
            self.assertEqual(len([cmd for cmd in commands if cmd[0] == 'docker']), 2)


if __name__ == '__main__':
    unittest.main()
