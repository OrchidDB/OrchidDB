#!/usr/bin/env python3
"""Check generated pages, local destinations, fragments, and search coverage."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit, unquote
import json
from build import OUT, PAGES

class Page(HTMLParser):
    def __init__(self, path):
        super().__init__(); self.path=path; self.ids=set(); self.links=[]; self.h1=0; self.lang=False
        self.feed(path.read_text())
    def handle_starttag(self, tag, attrs):
        a=dict(attrs)
        if 'id' in a:
            assert a['id'] not in self.ids, f'Duplicate ID: {self.path}: {a["id"]}'
            self.ids.add(a['id'])
        if tag=='html': self.lang=a.get('lang')=='en'
        if tag=='h1': self.h1+=1
        for key in ('href','src'):
            if key in a: self.links.append(a[key])

pages={p:Page(p) for p in OUT.glob('*.html')}
count=0
for path,page in pages.items():
    assert page.h1==1 and page.lang, f'Invalid document structure: {path}'
    for link in page.links:
        u=urlsplit(link)
        if u.scheme or u.netloc: continue
        dest=OUT/unquote(u.path).lstrip('/') if u.path.startswith('/') else path.parent/unquote(u.path) if u.path else path
        if dest.is_dir(): dest=dest/'index.html'
        assert dest.is_file(), f'Broken link in {path.name}: {link}'
        if u.fragment and dest in pages:
            assert unquote(u.fragment) in pages[dest].ids, f'Broken fragment in {path.name}: {link}'
        count+=1
index=json.loads((OUT/'search-index.json').read_text())
assert len(index)==len(PAGES)
assert len({p['url'] for p in index})==len(PAGES)
for p in index:
    assert (OUT/p['url'].lstrip('/')).is_file()
    assert len(p['text'])>500, p['title']
print(f'Checked {len(pages)} HTML documents, {count} local links/assets, and {len(index)} search entries.')
