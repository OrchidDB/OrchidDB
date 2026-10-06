#!/usr/bin/env python3
"""Build the standard mdBook guides with committed extension evidence."""
from pathlib import Path
from html import escape
import os
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'dist'
BASE = 'https://docs.orchiddb.com'
PAGES = [(slug, title) for title, slug in re.findall(
    r'^- \[([^\]]+)\]\(([^)]+)\.md\)$',
    (ROOT / 'content/SUMMARY.md').read_text(), re.MULTILINE)]


def build():
    mdbook = os.environ.get('MDBOOK', 'mdbook')
    try:
        subprocess.run([mdbook, 'clean', str(ROOT)], check=True)
        subprocess.run([mdbook, 'build', str(ROOT)], check=True)
    except FileNotFoundError:
        raise SystemExit('mdbook is required. Run website/docs/install-mdbook.sh or set MDBOOK to its executable.')
    shutil.copytree(ROOT / 'downloads', OUT / 'downloads', dirs_exist_ok=True)
    shutil.copyfile(ROOT / 'assets/favicon.svg', OUT / 'favicon.svg')

    shutil.copytree(ROOT.parents[1] / 'conformance/extension-results',
                    OUT / 'downloads/conformance', dirs_exist_ok=True)

    urls = ['/' if slug == 'index' else f'/{slug}.html' for slug, _ in PAGES]
    (OUT / 'robots.txt').write_text(f'User-agent: *\nAllow: /\nSitemap: {BASE}/sitemap.xml\n')
    (OUT / 'sitemap.xml').write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' +
        ''.join(f'<url><loc>{BASE}{escape(url)}</loc></url>' for url in urls) + '</urlset>\n')
    (OUT / 'llms.txt').write_text('# OrchidDB documentation\n\n' + ''.join(
        f'- [{title}]({BASE}/{slug}.html)\n' for slug, title in PAGES))
    print(f'Built {len(PAGES)} mdBook chapters and extension evidence in {OUT}')


if __name__ == '__main__':
    build()
