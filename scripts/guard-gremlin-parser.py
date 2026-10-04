#!/usr/bin/env python3
"""Reapply stack-growth guards after regenerating the ANTLR Gremlin parser.

Guard every rule entry: recursive cycles also occur through literals, predicates,
and source configuration, not just nested traversal expressions. This imposes no
input-depth limit. Idempotent, and fails if generated rule layout changes.
"""
from pathlib import Path
import re

path = Path(__file__).resolve().parents[1] / 'src/grammar/generated/gremlin/gremlinparser.rs'
text = path.read_text()
if '// OrchidDB: grow at every parser rule boundary.' not in text:
    pattern = r'(        let (?:mut )?recog = self;)(.*?)(\n    }\n})'
    def guard(match):
        return ('        // OrchidDB: grow at every parser rule boundary.\n'
                '        stacker::maybe_grow(1024 * 1024, 8 * 1024 * 1024, || {\n'
                + match[1] + match[2] + '\n        })' + match[3])
    text, count = re.subn(pattern, guard, text, flags=re.S)
    assert count >= 300, f'Unexpected generated layout: {count} rules'
    path.write_text(text)
