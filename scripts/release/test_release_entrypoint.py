"""Exercise the shell entry point without building or publishing artifacts."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class ReleaseEntrypointTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / 'scripts').mkdir()
        self.script = self.root / 'scripts/release-all.sh'
        shutil.copy2(Path(__file__).resolve().parents[1] / 'release-all.sh', self.script)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.log = self.root / 'calls'
        for name, body in {
            'uname': 'if [ "$1" = -s ]; then echo Darwin; else echo arm64; fi',
            'git': 'echo abc123',
            'make': 'printf "make %s\\n" "$*" >> "$CALLS"',
            'gh': 'printf "gh %s\\n" "$*" >> "$CALLS"',
            'python': '''if [ "$1" = -c ]; then exit 0; fi
printf "python %s\\n" "$*" >> "$CALLS"
if [ "${FAIL_BUILD:-}" = 1 ] && [ "$2" = build ]; then exit 42; fi''',
        }.items():
            path = self.bin / name
            path.write_text('#!/bin/sh\n' + body + '\n')
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        PYTHON=str(self.bin / 'python'), CALLS=str(self.log))

    def run_script(self, *args):
        result = subprocess.run(['bash', str(self.script), '--version', '1.2.3', *args],
                                cwd='/', env=self.env, text=True, capture_output=True)
        calls = self.log.read_text().splitlines() if self.log.exists() else []
        return result, calls

    def test_default_builds_and_packages_without_publishing(self):
        result, calls = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([line.split()[0] for line in calls], ['make', 'make', 'python', 'python'])
        self.assertIn('release-prepare', calls[0])
        self.assertIn('release-env', calls[1])
        self.assertIn('release.py build --version 1.2.3', calls[2])
        self.assertIn('release.py package --version 1.2.3', calls[3])
        self.assertIn('orchiddb-v1.2.3/abc123/upload/', result.stdout)

    def test_failed_build_stops_before_packaging(self):
        self.env['FAIL_BUILD'] = '1'
        result, calls = self.run_script()
        self.assertEqual(result.returncode, 42)
        self.assertFalse(any(' package ' in line or ' all ' in line or line.startswith('gh ') for line in calls))

    def test_publish_is_explicit_and_checks_auth_first(self):
        result, calls = self.run_script('--publish')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls[0], 'gh auth status')
        self.assertIn('release.py all --version 1.2.3', calls[-1])

    def test_check_does_not_bump_build_or_publish(self):
        result, calls = self.run_script('--check')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertIn('release-env', calls[0])
        self.assertIn('release.py check --version 1.2.3', calls[1])

    def test_invalid_options_do_not_start_work(self):
        for args in [('--publish', '--check'), ('--unknown',), ('--version',)]:
            with self.subTest(args=args):
                result, calls = self.run_script(*args)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(calls, [])


if __name__ == '__main__':
    unittest.main()
