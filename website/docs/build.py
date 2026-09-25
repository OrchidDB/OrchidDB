#!/usr/bin/env python3
"""Build the standard mdBook guides and the separate committed-evidence report."""
from pathlib import Path
from html import escape
import os
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'dist'
BASE = 'https://docs.crabgraph.net'
PAGES = [(slug, title) for title, slug in re.findall(
    r'^- \[([^\]]+)\]\(([^)]+)\.md\)$',
    (ROOT / 'content/SUMMARY.md').read_text(), re.MULTILINE)]


def build():
    mdbook = os.environ.get('MDBOOK', 'mdbook')
    try:
        subprocess.run([mdbook, 'build', str(ROOT)], check=True)
    except FileNotFoundError:
        raise SystemExit('mdbook is required. Run website/scripts/install-mdbook.sh or set MDBOOK to its executable.')
    shutil.copytree(ROOT / 'downloads', OUT / 'downloads', dirs_exist_ok=True)
    shutil.copyfile(ROOT.parent / 'favicon.svg', OUT / 'favicon.svg')

    # The large evidence explorer stays outside the book and its search index.
    from conformance_page import render
    article, _ = render(OUT)
    assets = OUT / 'assets'
    assets.mkdir(exist_ok=True)
    for name in ('conformance.css', 'conformance.js', 'report.css'):
        shutil.copyfile(ROOT / 'assets' / name, assets / name)
    (OUT / 'conformance-report.html').write_text('''<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Conformance report · Crabgraph</title>
<link rel="icon" href="favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="assets/conformance.css">
<link rel="stylesheet" href="assets/report.css">
<script src="assets/conformance.js" defer></script></head>
<body class="comparison-page"><a class="skip-link" href="#content">Skip to content</a>
<header class="report-header"><a href="conformance.html">← Back to the book</a>
<a href="https://crabgraph.net/">Crabgraph</a>
<a href="https://github.com/henneberger/new-graph">GitHub</a></header>
<main id="content"><h1>Conformance report</h1>
<p>Recorded upstream scenarios, individual outcomes, and reproducible evidence.</p>
''' + article + '\n</main></body></html>\n')

    urls = ['/' if slug == 'index' else f'/{slug}.html' for slug, _ in PAGES]
    urls.append('/conformance-report.html')
    (OUT / 'robots.txt').write_text(f'User-agent: *\nAllow: /\nSitemap: {BASE}/sitemap.xml\n')
    (OUT / 'sitemap.xml').write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' +
        ''.join(f'<url><loc>{BASE}{escape(url)}</loc></url>' for url in urls) + '</urlset>\n')
    (OUT / 'llms.txt').write_text('# Crabgraph documentation\n\n' + ''.join(
        f'- [{title}]({BASE}/{slug}.html)\n' for slug, title in PAGES) +
        f'- [Full conformance report]({BASE}/conformance-report.html)\n')
    print(f'Built {len(PAGES)} mdBook chapters and the conformance report in {OUT}')


if __name__ == '__main__':
    build()
