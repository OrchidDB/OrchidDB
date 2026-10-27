"""Update Orchid package versions without rewriting third-party dependencies."""
import json
from pathlib import Path
import re


def update_manifest(name, text, old, new):
    name = Path(name).name
    if name in ('package.json', 'package-lock.json'):
        data = json.loads(text)
        if data.get('name', '').startswith('@orchiddb/') and data.get('version') == old:
            data['version'] = new
        for key, package in data.get('packages', {}).items():
            if (key == '' or package.get('name', '').startswith('@orchiddb/')) and package.get('version') == old:
                package['version'] = new
        return json.dumps(data, indent=2) + '\n'
    if name in ('Cargo.toml', 'pyproject.toml'):
        section = None
        lines = []
        for line in text.splitlines(keepends=True):
            if line.startswith('['):
                section = line.strip()
            if section in ('[package]', '[project]'):
                line = re.sub(r'^(version\s*=\s*")' + re.escape(old) + r'(")', lambda m: m[1] + new + m[2], line)
            if re.match(r'^orchiddb[\w-]*\s*=', line):
                line = line.replace('version = "=' + old + '"', 'version = "=' + new + '"')
            lines.append(line)
        return ''.join(lines)
    if name == 'pom.xml':
        # All versioned Orchid artifacts have this prefix, including parents.
        return re.sub(r'((?:<groupId>com\.orchiddb(?:\.examples)?</groupId>\s*<artifactId>[^<]+</artifactId>|<artifactId>orchiddb[^<]*</artifactId>)\s*<version>)' + re.escape(old) + r'(</version>)', lambda m: m[1] + new + m[2], text)
    if name == 'CMakeLists.txt':
        return re.sub(r'(\bVERSION\s+|find_package\(OrchidDB\s+)' + re.escape(old) + r'(?=[\s)])', lambda m: m[1] + new, text)
    if name == 'mix.exs':
        return text.replace('version: "' + old + '"', 'version: "' + new + '"')
    raise ValueError('Unsupported release manifest: ' + name)
