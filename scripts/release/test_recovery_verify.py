import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import cache_recovery
import verify


class RecoveryTests(unittest.TestCase):
    def test_preserves_stale_objects_and_invalidates_only_sasl(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            profile = root / 'target/release-linux_arm64/aarch64-unknown-linux-gnu/release'
            archive = profile / 'build/sasl2-sys-abc/out/lib/libsasl2.a'
            archive.parent.mkdir(parents=True); archive.write_bytes(b'archive __ubsan_handle_bad')
            fingerprint = profile / '.fingerprint/sasl2-sys-abc'
            fingerprint.mkdir(parents=True); (fingerprint / 'stamp').write_text('old')
            other = profile / 'build/other/out/keep'; other.parent.mkdir(parents=True); other.write_bytes(b'keep')
            backups = cache_recovery.recover(root)
            self.assertEqual(len(backups), 1)
            self.assertFalse(archive.exists()); self.assertFalse(fingerprint.exists())
            self.assertEqual(other.read_bytes(), b'keep')
            self.assertEqual((backups[0] / 'build/sasl2-sys-abc/out/lib/libsasl2.a').read_bytes(), b'archive __ubsan_handle_bad')
            self.assertEqual(cache_recovery.recover(root), [])

    def test_clean_archive_is_untouched(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / 'target/release-linux_amd64/x86_64-unknown-linux-gnu/release/build/sasl2-sys-abc/out/lib.a'
            archive.parent.mkdir(parents=True); archive.write_bytes(b'clean archive')
            self.assertEqual(cache_recovery.recover(root), [])
            self.assertEqual(archive.read_bytes(), b'clean archive')

    def test_verification_rejects_tampering_before_execution(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(verify, 'run') as run:
            root = Path(tmp)
            (root / 'release-manifest.json').write_text('{}')
            (root / 'SHA256SUMS').write_text('0' * 64 + '  release-manifest.json\n')
            with self.assertRaisesRegex(RuntimeError, 'SHA256SUMS'): verify.verify(root)
            run.assert_not_called()
