#!/usr/bin/env python3
"""Check mdBook chapters, local links, anchors, downloads, and search coverage."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit, unquote
import json
import re
from build import OUT, PAGES


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.path, self.ids, self.links, self.h1, self.lang = path, set(), [], 0, False
        self.feed(path.read_text())

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            assert attrs['id'] not in self.ids, f'Duplicate ID: {self.path}: {attrs["id"]}'
            self.ids.add(attrs['id'])
        if tag == 'html':
            self.lang = attrs.get('lang') == 'en'
        if tag == 'h1':
            self.h1 += 1
        for key in ('href', 'src'):
            if key in attrs:
                self.links.append(attrs[key])


pages = {p.resolve(): Page(p) for p in OUT.glob('*.html')}
count = 0
for path, page in pages.items():
    assert page.lang, f'Missing language: {path}'
    # mdBook adds a toolbar title; the printable book has all chapter headings.
    assert page.h1 >= 1 or path.name == 'toc.html', f'Missing heading: {path}'
    for link in page.links:
        u = urlsplit(link)
        if u.scheme or u.netloc:
            continue
        dest = (OUT / unquote(u.path).lstrip('/') if u.path.startswith('/') else
                path.parent / unquote(u.path) if u.path else path)
        if dest.is_dir():
            dest /= 'index.html'
        dest = dest.resolve()
        assert dest.is_file(), f'Broken link in {path.name}: {link}'
        if u.fragment and dest in pages:
            assert unquote(u.fragment) in pages[dest].ids, f'Broken fragment in {path.name}: {link}'
        count += 1
search_path = next(OUT.glob('searchindex-*.js'))
raw = search_path.read_text()
urls = json.loads(re.search(r'"doc_urls":(\[[^\]]+\])', raw)[1])
indexed = {url.split('#')[0] for url in urls}
for slug, title in PAGES:
    assert f'{slug}.html' in indexed, f'Missing from search: {title}'
    page = (OUT / f'{slug}.html').read_text()
    assert 'assets/docs.css' not in page and 'assets/docs.js' not in page
assert (OUT / 'downloads/conformance/upstream-comparison.csv').is_file()
print(f'Checked {len(pages)} HTML documents, {count} local links/assets, and all {len(PAGES)} chapters in mdBook search.')
