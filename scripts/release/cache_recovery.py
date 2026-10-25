"""Preserve and invalidate native objects built with incompatible sanitizer flags."""
import hashlib
import json
from pathlib import Path
import shutil
import time


def recover(root):
    recovered = []
    for cache in sorted((root / 'target').glob('release-linux_*')):
        for profile in cache.glob('*-unknown-linux-gnu/release'):
            stale = [p for p in (profile / 'build').glob('sasl2-sys-*/out/**/*.a')
                     if b'__ubsan_handle_' in p.read_bytes()]
            if not stale:
                continue
            backup = root / 'target/release-cache-recovery' / str(time.time_ns()) / cache.name
            backup.mkdir(parents=True)
            evidence = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in stale}
            moved = []
            # Keep the complete old build trees, rather than deleting caches or
            # reusing autotools objects whose Makefiles don't track CFLAGS.
            for category in ('build', '.fingerprint'):
                for path in sorted((profile / category).glob('sasl2-sys-*')):
                    destination = backup / category / path.name
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.move(str(path), destination)
                    moved.append({'from': str(path), 'to': str(destination)})
            (backup / 'receipt.json').write_text(json.dumps({'archives': evidence, 'moved': moved}, indent=2))
            recovered.append(backup)
            print('Preserved incompatible SASL cache; rebuilding:', backup, flush=True)
    return recovered
