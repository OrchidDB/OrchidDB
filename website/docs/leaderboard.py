"""Rank recorded outcomes within each language, never across different suites."""
import json
import math
from collections import Counter
from html import escape
from pathlib import Path

PRODUCTS = {'crabgraph': 'Crabgraph', 'sqlg': 'SQLg', 'puppygraph': 'PuppyGraph', 'janusgraph': 'JanusGraph', 'neo4j': 'Neo4j Community', 'jena': 'Apache Jena TDB2'}
SUITES = [('tinkerpop', 'Gremlin', ('crabgraph', 'sqlg', 'puppygraph', 'janusgraph')),
          ('opencypher', 'Cypher', ('crabgraph', 'neo4j', 'puppygraph')),
          ('rdf', 'SPARQL', ('crabgraph', 'jena'))]


def passed_runtime(results):
    passed = [r for r in results if r['status'] == 'pass']
    durations = [r.get('elapsed_ms') for r in passed]
    valid = [t for t in durations if isinstance(t, (int, float)) and not isinstance(t, bool) and math.isfinite(t) and t >= 0]
    return {'elapsed_ms': sum(valid) if len(valid) == len(passed) else None,
            'timed_passes': len(valid), 'passed': len(passed)}


def render(cases, get_result, runs, root, download):
    baseline_path = root / 'data/parity-baseline.json'
    baseline = json.loads(baseline_path.read_text()) if baseline_path.exists() else {'suites': {}}
    if baseline_path.exists():
        (download / 'parity-baseline.json').write_bytes(baseline_path.read_bytes())
    report = {'baseline_recorded_at': baseline.get('recorded_at'), 'suites': {}}
    html = ['<section id="leaderboard" class="leaderboard"><h2>Current leaderboard</h2>',
            '<p>Ranked by passed upstream scenarios within each language. Crabgraph is compared as one product, with one recorded outcome per scenario from a single suite run; execution details are available in the evidence. Peer-only passes are cases another compared engine passes that this engine has not passed.</p>',
            '<div class="leaderboard-scroll"><table class="leaderboard-table"><thead><tr><th>Language</th><th>Rank</th><th>Engine</th><th>Passed / total</th><th>Peer-only passes</th><th>Passed runtime</th><th>Run (UTC)</th></tr></thead>']
    for suite, title, products in SUITES:
        subset = [case for case in cases if case['suite'] == suite]
        results = {product: [get_result(product, case) for case in subset] for product in products}
        passes = {product: {r['id'] for r in values if r['status'] == 'pass'} for product, values in results.items()}
        order = sorted(products, key=lambda product: -len(passes[product]))
        report['suites'][suite] = {'total': len(subset), 'products': {}}
        html.append('<tbody>')
        for index, product in enumerate(order):
            passed = len(passes[product])
            rank = 1 + sum(len(passes[other]) > passed for other in products)
            peers = set().union(*(passes[other] for other in products if other != product))
            gaps = sorted(peers - passes[product])
            run = runs.get((product, suite), {})
            evidence = f'{product}-{suite}'
            runtime = passed_runtime(results[product]) if product == 'crabgraph' else None
            duration = (f"{runtime['elapsed_ms']/1000:,.2f} s" if runtime['elapsed_ms'] is not None else 'Unavailable') if runtime else '—'
            report['suites'][suite]['products'][product] = {
                'rank': rank if len(products) > 1 else None, 'passed': passed,
                'passed_runtime': runtime,
                'counts': dict(Counter(r['status'] for r in results[product])),
                'peer_only_passes': len(gaps), 'peer_only_case_ids': gaps,
                'finished_at': run.get('finished_at'), 'build': run.get('build'),
                'source': run.get('source'),
                'evidence': evidence + '.json',
            }
            html.append('<tr' + (' class="crabgraph-standing"' if product == 'crabgraph' else '') + '>')
            if index == 0:
                html.append(f'<th scope="rowgroup" rowspan="{len(products)}"><a href="#language-{suite}">{title}</a></th>')
            rank_label = str(rank) if len(products) > 1 else '—'
            gap_label = f'{len(gaps):,}' if len(products) > 1 else '—'
            timestamp = run.get('finished_at', '')[:16].replace('T', ' ')
            html.append(f'<td>{rank_label}</td><th scope="row">{PRODUCTS[product]}</th><td><strong>{passed:,}</strong> / {len(subset):,}</td><td>{gap_label}</td><td>{duration}</td><td><a href="/downloads/conformance/{evidence}.json">{escape(timestamp)}</a></td></tr>')
        html.append('</tbody>')
    html.append('</table></div><p>Crabgraph passed runtime sums recorded scenario time for passed cases only, including setup and assertions. Failed and other non-passing cases are excluded.</p><p class="leaderboard-downloads"><a href="/downloads/conformance/leaderboard.json">Leaderboard and gap cases JSON</a> · <a href="/downloads/conformance/parity-baseline.json">Starting baseline</a></p></section>')
    (download / 'leaderboard.json').write_text(json.dumps(report, indent=2) + '\n')
    return '\n'.join(html)
