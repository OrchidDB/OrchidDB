#!/usr/bin/env python3
"""Build the documentation with Python's standard library; no runtime server."""
from pathlib import Path
from html import escape
import re, json, shutil

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'dist'
BASE = 'https://docs.crabgraph.net'
GROUPS = [
 ('Get started', [('index','Introduction'),('installation','Installation'),('quickstart','Quickstart'),('concepts','Core concepts')]),
 ('Work with data', [('mapped-graphs','Map existing tables'),('mapping-reference','Mapping reference'),('views','Views and SQL sources'),('managed-graphs','Managed graphs'),('transactions','Transactions and storage'),('updates','Update properties')]),
 ('Query languages', [('cypher','Cypher'),('gremlin','Gremlin'),('sparql','SPARQL'),('ontology','Ontology mappings'),('rdf','RDF datasets')]),
 ('Integrate', [('parameters','Parameters and values'),('results','Arrow results'),('execution','Execution and plans'),('rust-api','Rust API'),('cli','CLI reference')]),
 ('Resources', [('recipes','Query recipes'),('configuration','Configuration'),('glossary','Glossary')]),
]
PAGES = [(slug,title,group) for group,pages in GROUPS for slug,title in pages]
def url(slug): return '/index.html' if slug == 'index' else f'/{slug}.html'
def inline(s):
    tokens=[]
    def protect(m):
        tokens.append('<code>'+escape(m[1])+'</code>'); return f'\x00{len(tokens)-1}\x00'
    s=re.sub(r'`([^`]+)`',protect,s)
    s=escape(s)
    s=re.sub(r'\[([^\]]+)\]\(([^)]+)\)',r'<a href="\2">\1</a>',s)
    s=re.sub(r'\*\*([^*]+)\*\*',r'<strong>\1</strong>',s)
    for n,t in enumerate(tokens): s=s.replace(f'\x00{n}\x00',t)
    return s

def render(src):
    lines=src.splitlines(); out=[]; toc=[]; i=0
    while i<len(lines):
        s=lines[i]
        if not s.strip(): i+=1; continue
        if s.startswith('```'):
            lang=s[3:].strip() or 'text'; code=[]; i+=1
            while i<len(lines) and not lines[i].startswith('```'): code.append(lines[i]); i+=1
            assert i<len(lines), 'Unclosed code fence'
            out.append(f'<div class="code-block"><div class="code-bar"><span>{escape(lang)}</span><button class="copy" type="button" aria-label="Copy code">Copy</button></div><pre tabindex="0"><code class="language-{escape(lang)}">{escape(chr(10).join(code))}</code></pre></div>'); i+=1; continue
        if s.startswith('## '):
            title=s[3:]; anchor=re.sub('[^a-z0-9]+','-',title.lower()).strip('-'); toc.append((anchor,title))
            out.append(f'<h2 id="{anchor}">{inline(title)}<a class="heading-anchor" href="#{anchor}" aria-label="Link to {escape(title)}">#</a></h2>'); i+=1; continue
        if s.startswith('### '): out.append(f'<h3>{inline(s[4:])}</h3>'); i+=1; continue
        if s.startswith('|'):
            rows=[]
            while i<len(lines) and lines[i].startswith('|'):
                cells=[x.strip() for x in lines[i].strip('|').split('|')]
                if not all(re.fullmatch(r'[-: ]+',x) for x in cells): rows.append(cells)
                i+=1
            out.append('<div class="table-wrap"><table><thead><tr>'+''.join('<th scope="col">'+inline(x)+'</th>' for x in rows[0])+'</tr></thead><tbody>'+''.join('<tr>'+''.join('<td>'+inline(x)+'</td>' for x in row)+'</tr>' for row in rows[1:])+'</tbody></table></div>'); continue
        if s.startswith('- '):
            items=[]
            while i<len(lines) and lines[i].startswith('- '): items.append('<li>'+inline(lines[i][2:])+'</li>'); i+=1
            out.append('<ul>'+''.join(items)+'</ul>'); continue
        if s.startswith('> '): out.append('<aside class="note">'+inline(s[2:])+'</aside>'); i+=1; continue
        para=[s]; i+=1
        while i<len(lines) and lines[i].strip() and not lines[i].startswith(('##','```','|','- ','> ')): para.append(lines[i]); i+=1
        out.append('<p>'+inline(' '.join(para))+'</p>')
    return '\n'.join(out),toc

def build():
    OUT.mkdir(exist_ok=True)
    shutil.copytree(ROOT/'assets', OUT/'assets',dirs_exist_ok=True)
    shutil.copytree(ROOT/'downloads', OUT/'downloads',dirs_exist_ok=True)
    shutil.copyfile(ROOT.parent/'favicon.svg',OUT/'favicon.svg')
    search=[]
    for n,(slug,title,group) in enumerate(PAGES):
        src=(ROOT/'content'/f'{slug}.md').read_text()
        description,body=src.split('\n',1)
        article,toc=render(body)
        nav=''.join('<div class="nav-group"><p>'+escape(g)+'</p>'+''.join(f'<a href="{url(s)}"'+(' aria-current="page"' if s==slug else '')+'>'+escape(t)+'</a>' for s,t in pages)+'</div>' for g,pages in GROUPS)
        pager=''
        for index,label in [(n-1,'Previous'),(n+1,'Next')]:
            if 0<=index<len(PAGES):
                s,t,_=PAGES[index]; pager+=f'<a href="{url(s)}"><small>{label}</small><span>{escape(t)} {"→" if label=="Next" else ""}</span></a>'
        canonical=BASE+('/' if slug=='index' else url(slug))
        html=f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{escape(title)} · Crabgraph docs</title><meta name="description" content="{escape(description)}">
<link rel="canonical" href="{canonical}"><meta property="og:title" content="{escape(title)} · Crabgraph docs"><meta property="og:description" content="{escape(description)}"><meta property="og:type" content="website"><meta property="og:url" content="{canonical}"><meta name="theme-color" content="#f5f3ec"><link rel="icon" href="/favicon.svg" type="image/svg+xml"><link rel="stylesheet" href="/assets/docs.css"><script src="/assets/docs.js" defer></script></head>
<body><a class="skip" href="#content">Skip to content</a><header class="header"><a class="brand" href="/index.html"><img src="/favicon.svg" width="31" height="31" alt="">crabgraph<span>docs</span></a><div class="header-actions"><button id="search-open" type="button" hidden>Search docs <kbd>/</kbd></button><a href="https://crabgraph.net/">Website ↗</a><a class="github" href="https://github.com/henneberger/new-graph">GitHub ↗</a><button id="menu" type="button" aria-expanded="false" aria-controls="sidebar" hidden>Menu</button></div></header>
<div class="layout"><nav class="sidebar" id="sidebar" aria-label="Documentation"><div class="version">DOCUMENTATION <span>v0.1.0</span></div>{nav}<a class="nav-download" href="/llms.txt">Plain text index ↗</a></nav><main id="content"><div class="eyebrow">{escape(group)}</div><h1>{escape(title)}</h1><p class="lead">{escape(description)}</p>{article}<nav class="pager" aria-label="Adjacent pages">{pager}</nav><footer>Crabgraph documentation · <a href="https://crabgraph.net/">crabgraph.net</a></footer></main><aside class="toc"><p>ON THIS PAGE</p>{''.join(f'<a href="#{a}">{escape(t)}</a>' for a,t in toc)}<div class="toc-bottom">Graph languages.<br>Your data.</div></aside></div>
<dialog id="search-dialog" aria-labelledby="search-title"><div class="search-top"><label id="search-title" for="search-input">Search documentation</label><button id="search-close" type="button" aria-label="Close search">Esc</button></div><input id="search-input" type="search" placeholder="Try mappings, transactions, or Cypher" autocomplete="off"><p id="search-status" role="status"></p><div id="search-results"></div></dialog><div id="copy-status" class="sr-only" role="status"></div></body></html>'''
        (OUT/f'{slug}.html').write_text(html)
        search.append(dict(title=title,group=group,url=url(slug),description=description,text=re.sub(r'[`#|*]','',body)))
    (OUT/'search-index.json').write_text(json.dumps(search))
    (OUT/'robots.txt').write_text(f'User-agent: *\nAllow: /\nSitemap: {BASE}/sitemap.xml\n')
    (OUT/'sitemap.xml').write_text('<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'+''.join(f'<url><loc>{BASE}{"/" if s=="index" else url(s)}</loc></url>' for s,_,_ in PAGES)+'</urlset>')
    (OUT/'llms.txt').write_text('# Crabgraph documentation\n\nStatic guides for the Crabgraph Rust graph query engine.\n\n'+''.join(f'- [{t}]({BASE}{url(s)}): {g}\n' for s,t,g in PAGES))
    (OUT/'404.html').write_text('<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Page not found · Crabgraph docs</title><link rel="stylesheet" href="/assets/docs.css"><main><div class="eyebrow">404</div><h1>Let’s find your way.</h1><p>This address does not match a documentation page.</p><a href="/index.html">Go to the documentation →</a></main></html>')
    print(f'Built {len(PAGES)} documentation pages in {OUT}')
if __name__=='__main__': build()
