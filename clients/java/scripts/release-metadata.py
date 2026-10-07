#!/usr/bin/env python3
"""Validate a committed release tag and emit CI metadata without changing source files."""
import os
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[1]
ns = {'m': 'http://maven.apache.org/POM/4.0.0'}
version = ET.parse(root / 'pom.xml').findtext('m:version', namespaces=ns)
tag = os.environ['RELEASE_TAG']
if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+(?:-(?:alpha|beta|rc)\.[0-9]+)?', tag):
    raise SystemExit('Use an immutable vX.Y.Z release tag (optionally -alpha.N/-beta.N/-rc.N)')
if tag != 'v' + version:
    raise SystemExit('Tag must match the non-SNAPSHOT Maven version committed in pom.xml')
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
tag_commit = subprocess.check_output(['git', 'rev-parse', 'refs/tags/' + tag + '^{commit}'], cwd=root, text=True).strip()
if head != tag_commit:
    raise SystemExit('Checkout does not match release tag')
for module in ['orchiddb-java', 'orchiddb-gremlin']:
    parent = ET.parse(root / module / 'pom.xml').findtext('m:parent/m:version', namespaces=ns)
    if parent != version:
        raise SystemExit('Module parent versions must match')
core = (root / 'native/CORE_REVISION').read_text().strip()
if not re.fullmatch('[0-9a-f]{40}', core):
    raise SystemExit('CORE_REVISION must pin a full commit')
result = f'version={version}\ncore={core}\ncommit={head}\n'
if os.environ.get('GITHUB_OUTPUT'):
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(result)
print(result, end='')
