#!/usr/bin/env bash
# Local adapter checks, using the same classpath and parser isolation as the suite.
set -euo pipefail
cd "$(dirname "$0")/.."
export CONFORMANCE_ARCADEDB_TESTS=1
python="${CONFORMANCE_PYTHON:-python3}"
"$python" -m unittest discover -s conformance/upstream -p test_arcadedb.py -v
"$python" - <<'PY'
import os
import subprocess
import sys
from pathlib import Path
sys.path.insert(0, 'conformance/upstream')
from arcadedb import gremlin_classpath
root = Path('conformance/adapters/sqlg')
classpath = gremlin_classpath() + os.pathsep + str((root / 'target-arcadedb/test-classes').resolve())
tests = sorted(p.stem for p in (root / 'src/test/java').glob('*Test.java'))
subprocess.run([os.environ.get('CONFORMANCE_JAVA', 'java'),
                '--add-opens=java.base/java.lang=ALL-UNNAMED',
                '--add-opens=java.base/java.util=ALL-UNNAMED',
                '-cp', classpath, 'org.junit.runner.JUnitCore', *tests], check=True)
PY
