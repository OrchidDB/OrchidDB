#!/usr/bin/env python3
"""Generate SQL Unicode property ranges from the pinned unicode-segmentation crate.
Usage: python3 scripts/dev/generate_grapheme_sql.py /path/to/unicode-segmentation-1.13.2
The crate's MIT license accompanies the generated file.
"""
import pathlib, re, sys
root = pathlib.Path(sys.argv[1])
source = (root / 'src/tables.rs').read_text()
assert 'UNICODE_VERSION: (u64, u64, u64) = (17, 0, 0)' in source
names = ['Any', 'CR', 'Control', 'Extend', 'Extended_Pictographic', 'InCB_Consonant', 'L', 'LF', 'LV', 'LVT', 'Prepend', 'Regional_Indicator', 'SpacingMark', 'T', 'V', 'ZWJ']
props = [[0,0,0] for _ in range(0x110000)]
section = source.split('const grapheme_cat_table:')[1].split('];',1)[0]
for a,b,c in re.findall(r"\('\\u\{([0-9a-fA-F]+)\}',\s*'\\u\{([0-9a-fA-F]+)\}',\s*GC_(\w+)\)", section):
    for n in range(int(a,16),int(b,16)+1): props[n][0] = names.index(c)
section = source.split('const InCB_Extend_table:')[1].split('];',1)[0]
for a,b in re.findall(r"\('\\u\{([0-9a-fA-F]+)\}',\s*'\\u\{([0-9a-fA-F]+)\}'\)", section):
    for n in range(int(a,16),int(b,16)+1): props[n][1] = 1
section = source.split('pub fn is_incb_linker')[1].split('pub mod grapheme')[0]
for c in re.findall(r"'\\u\{([0-9a-fA-F]+)\}'", section): props[int(c,16)][2] = 1
out=[]; start=0
for n in range(1,len(props)+1):
    if n==len(props) or props[n]!=props[start]:
        if props[start] != [0,0,0]: out.append(','.join(map(str,[start,n-1]+props[start])))
        start=n
path=pathlib.Path('src/ir/functions/portable/grapheme_ranges.csv')
path.write_text('\n'.join(out)+'\n')
path.with_suffix('.LICENSE').write_text((root/'LICENSE-MIT').read_text())
print(f'{len(out)} Unicode 17 property ranges')
