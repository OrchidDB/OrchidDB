"""Linux wrapper ABI and single-engine host validation contract."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('orchid_extension_build', Path(__file__).resolve().parents[2] / 'extension/scripts/build.py')
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class LinuxWrapperTests(unittest.TestCase):
    def test_architecture_and_toolchain_are_pinned_and_mounts_are_exact(self):
        for arch in ('arm64', 'amd64'):
            with self.subTest(arch=arch):
                command = build.linux_container(arch, [Path('/tmp/a path'), Path('/tmp/a path'), Path('/tmp/output')])
                self.assertEqual(command[:6], ['docker', 'run', '--rm', '--platform', 'linux/' + arch, '--volume'])
                resolved = str(Path('/tmp/a path').resolve())
                self.assertEqual(command.count(resolved + ':' + resolved), 1)
                self.assertIn('@sha256:', command[-1])
                self.assertIn('manylinux_2_28_', command[-1])

    def test_host_smoke_uses_existing_engine_and_checks_actual_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            vendor = Path(tmp)
            (vendor / 'python-duckdb-arm64/duckdb').mkdir(parents=True)
            with patch.object(build, 'run') as run:
                build.linux_load_check(['docker'], vendor, 'arm64', Path("/tmp/quote'd/orchid.duckdb_extension"))
            command = run.call_args.args[0]
            program = command[command.index('-c') + 1]
            queries = []
            class Database:
                def execute(self, query):
                    queries.append(query)
                    return self
                def fetchall(self):
                    return [(42,)]
            engine = types.SimpleNamespace(__version__='1.5.6', connect=lambda **kwargs: Database())
            calls = []
            c_loader = types.SimpleNamespace(RTLD_GLOBAL=256, CDLL=lambda path, **kwargs: calls.append((path, kwargs)))
            modules = {'duckdb': engine, '_duckdb': types.SimpleNamespace(__file__='/existing/host.so'), 'ctypes': c_loader}
            argv = ['-c', str(vendor / 'python-duckdb-arm64'), "/tmp/quote'd/orchid.duckdb_extension", '1.5.6']
            old_path = sys.path[:]
            try:
                with patch.dict(sys.modules, modules), patch.object(sys, 'argv', argv):
                    exec(program, {})
                self.assertEqual(calls, [('/existing/host.so', {'mode': 256})])
                self.assertIn("quote''d", queries[0])
                self.assertEqual(json.loads(queries[1].split("'release','", 1)[1][:-2]), {'tables': []})
                engine.__version__ = '1.5.2'
                with patch.dict(sys.modules, modules), patch.object(sys, 'argv', argv), self.assertRaises(AssertionError):
                    exec(program, {})
            finally:
                sys.path[:] = old_path
