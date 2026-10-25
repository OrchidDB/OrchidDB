#!/usr/bin/env python3
"""Export executed Cypher conformance evidence as searchable HTML, JSONL and SQL.

Run conformance/upstream/run.py first; this tool never recompiles test queries.
Missing diagnostics, including expected errors, remain explicitly unclassified.
"""
import argparse
from collections import Counter
import hashlib
import html
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]


def classify(plan):
    if not plan or plan.get('capture') != 'executed':
        return 'no_plan_captured'
    if plan.get('fully_lowered_to_sql'):
        return 'single_sql_region'
    nodes = {line.strip() for line in plan.get('physical_plan', '').splitlines() if line.strip() and line.strip() != 'Executed subplan:'}
    if plan.get('sql_queries') and not plan.get('write_queries') and not plan.get('native_source_queries') and nodes and nodes <= {'DuckDbExec', 'DecodeTraversers', 'Pruned(DecodeTraversers)'}:
        return 'sql_with_result_decoding'
    if plan.get('sql_queries') or plan.get('write_queries') or plan.get('native_source_queries'):
        return 'sql_with_orchid_orchestration'
    return 'orchid_runtime'


def statements(plan):
    for key in ('sql_queries', 'write_queries', 'native_source_queries'):
        for value in plan.get(key, []):
            yield key, value if isinstance(value, str) else value['sql']


def export(report_path, output, catalog_path):
    report = json.loads(report_path.read_text())
    catalog = json.loads(catalog_path.read_text())
    cases = {row['id']: row for row in catalog['cases'] if row['suite'] == 'opencypher'}
    records = report['results']
    if len(records) != len(cases) or {r['id'] for r in records} != set(cases):
        raise ValueError('Input must contain every pinned Cypher test exactly once')
    output.mkdir(parents=True, exist_ok=True)
    (output / 'cases').mkdir(exist_ok=True)
    counts = Counter(); statuses = Counter(); functions = Counter(); boundaries = Counter()
    entries = []
    for index, result in enumerate(records, 1):
        case = cases[result['id']]
        expected_error = any('should be raised' in step['text'] for step in case['steps'])
        queries = []
        for observed in result.get('query_cost', {}).get('queries', []):
            cost = observed.get('cost') or {}
            plan = cost.get('execution_plan')
            classification = classify(plan)
            query = dict(query=observed.get('query', ''), phase=observed.get('phase', 'query'),
                         operation=observed.get('operation', 'cypher'), classification=classification,
                         plan=plan, measurements=cost.get('measurements'), missing_reason=cost.get('reason'))
            query['orchid_functions_in_sql'] = sorted(set(re.findall(r'\b(__orchid\w*|orchid\w*)\s*\(', '\n'.join(sql for _, sql in statements(plan or {})), re.I)))
            query['physical_nodes'] = [{'executor': 'DuckDB' if line.strip().startswith('DuckDbExec') else 'Orchid', 'node': line.strip(), 'depth': (len(line)-len(line.lstrip()))//2} for line in (plan or {}).get('physical_plan', '').splitlines() if line.strip() and line.strip() != 'Executed subplan:']
            queries.append(query)
            if query['phase'] == 'query':
                counts[classification] += 1
                functions.update(query['orchid_functions_in_sql'])
                for line in (plan or {}).get('physical_plan', '').splitlines():
                    if line.strip() and line.strip() != 'Executed subplan:': boundaries[line.strip()] += 1
        target = [q for q in queries if q['phase'] == 'query']
        classes = sorted({q['classification'] for q in target}) or ['no_plan_captured']
        name = f'{index:04d}-' + hashlib.sha256(case['id'].encode()).hexdigest()[:10]
        entry = dict(id=case['id'], name=case['name'], feature=case['feature'], source=case['source'],
                     status=result['status'], expected_error=expected_error, classifications=classes,
                     error=result.get('error'), actual_error=(result.get('actual') or {}).get('error') if isinstance(result.get('actual'),dict) else None,
                     original_steps=case['steps'], queries=queries, files={'json': f'cases/{name}.json', 'sql': f'cases/{name}.sql'})
        entries.append(entry); statuses[result['status']] += 1
        (output / entry['files']['json']).write_text(json.dumps(entry, indent=2, ensure_ascii=False) + '\n')
        sql_parts = [f'-- {case["id"]}', '-- SQL emitted for regions of executed requests, plus recorded writes and source fetches. Setup and targets are separate.',
                     '-- Schema DDL is a schema snapshot, not evidence of execution. Parameters/batches may be bound separately.']
        for n, query in enumerate(queries, 1):
            plan = query['plan'] or {}
            sql_parts.append(f'\n-- Request {n}; phase={query["phase"]}; classification={query["classification"]}')
            sql_parts.extend('-- CYPHER ' + line for line in query['query'].splitlines())
            for kind, sql in statements(plan): sql_parts += ['-- ' + kind, sql.rstrip(';') + ';']
            if not plan: sql_parts.append('-- No execution plan captured: ' + str(query['missing_reason']))
            if plan.get('native_source_queries'):
                sql_parts.append('-- Source SQL above is initiated by Orchid boundaries, outside relational SQL regions.')
        (output / entry['files']['sql']).write_text('\n'.join(sql_parts) + '\n')
    summary = dict(tests=len(entries), statuses=dict(statuses), target_query_classifications=dict(counts),
                   expected_error_tests=sum(e['expected_error'] for e in entries),
                   orchid_function_query_counts=dict(functions.most_common()), physical_node_occurrences=dict(boundaries.most_common()),
                   source_report=str(report_path.resolve()), source_report_sha256=hashlib.sha256(report_path.read_bytes()).hexdigest(),
                   build=report['build'], execution_profile=report.get('execution_profile'),
                   upstream=report['source'], started_at=report['started_at'], finished_at=report['finished_at'])
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    with (output / 'plans.jsonl').open('w') as stream:
        for entry in entries: stream.write(json.dumps(entry, ensure_ascii=False) + '\n')
    data = json.dumps(entries, ensure_ascii=False).replace('<', '\\u003c').replace('>', '\\u003e').replace('&', '\\u0026')
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><title>Cypher → DuckDB execution plans</title>
<style>body{font:15px system-ui;margin:24px;color:#182332}header{max-width:1100px}input,select{font:inherit;padding:9px;margin:4px}main{display:grid;grid-template-columns:370px minmax(0,1fr);gap:20px}nav{height:75vh;overflow:auto}button{display:block;text-align:left;width:100%;padding:10px;border:0;border-bottom:1px solid #ddd;background:#f4f6f8;cursor:pointer}button:hover{background:#dce9f5}article{min-width:0}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f4f6f8;padding:12px}details{border-top:1px solid #ccd;padding:10px 0}summary{cursor:pointer;font-weight:600}.tag{color:#52616d;font-size:12px}a{color:#1563a7}</style>
<header><h1>Cypher → DuckDB execution plans</h1><p>All pinned conformance scenarios, captured from real execution. Select a test to inspect its SQL and Orchid execution DAG.</p>
<p><b>SQL regions</b> execute inside DuckDB. <b>Orchid orchestration</b> connects regions and handles explicit graph/procedure/write boundaries. Orchid-prefixed functions inside SQL execute in DuckDB using the registered function library; they are not separate relational operators. Logical-plan joins and aggregates describe compilation, not DataFusion execution.</p>
<p>Setup and observation requests are separate from target queries. “No plan captured” is not proof that nothing executed: rejected queries and runtime failures do not return execution diagnostics. DAGs describe the scheduled host execution structure, not DuckDB’s internal physical plan. SQL listings are emitted region templates, not a statement-by-statement execution trace; boundaries can bind Arrow relations and parameters at runtime. The legacy residual_operators counter is not an Orchid-boundary count; inspect the physical nodes instead. “sql_with_result_decoding” means the captured physical DAG contains only DuckDB execution and Orchid result-decoding nodes, with no recorded persistence or source-fetch SQL. Result decoding and transaction/transport management can remain in Orchid even for a single SQL region.</p>
<p id="stats"></p><input id="search" placeholder="Search test, Cypher, SQL or operator" size="48"><select id="kind"><option value="">All execution types</option><option>single_sql_region</option><option>sql_with_result_decoding</option><option>sql_with_orchid_orchestration</option><option>orchid_runtime</option><option>no_plan_captured</option></select><span id="count"></span></header>
<main><nav id="list"></nav><article id="detail">Select a scenario.</article></main><script>
const rows=DATA;const totals=TOTALS;const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const block=(label,value)=>'<h4>'+esc(label)+'</h4><pre>'+esc(typeof value==='string'?value:JSON.stringify(value,null,2))+'</pre>';
document.getElementById('stats').textContent=rows.length+' tests • '+JSON.stringify(totals.statuses)+' • target requests: '+JSON.stringify(totals.target_query_classifications);
const indexed=rows.map(r=>JSON.stringify(r).toLowerCase());
function show(i){const r=rows[i];document.getElementById('detail').innerHTML='<h2>'+esc(r.name)+'</h2><p>'+esc(r.id)+' · '+esc(r.status)+(r.expected_error?' · expects an error':'')+'</p><p><a href="'+esc(r.source)+'">Upstream test</a> · <a href="'+r.files.json+'">JSON</a> · <a href="'+r.files.sql+'">SQL dump</a></p>'+ (r.actual_error?block('Reported error',r.actual_error):'')+r.queries.map((q,n)=>{let p=q.plan;return '<details '+(q.phase==='query'?'open':'')+'><summary>'+esc(q.phase)+' request '+(n+1)+' — '+esc(q.classification)+'</summary>'+block('Cypher',q.query)+(p?block('Physical DAG: SQL regions and Orchid boundaries',p.physical_plan)+block('Who executes each physical node',q.physical_nodes)+block('DuckDB SQL',p.sql_queries)+block('Persistence SQL',p.write_queries)+block('Orchid functions inside SQL',q.orchid_functions_in_sql)+block('Orchid-initiated source SQL (outside relational regions)',p.native_source_queries)+block('Execution measurements',q.measurements)+block('Logical compiler plan',p.logical_plan)+block('Schema snapshot',p.schema_sql):block('Missing diagnostics',q.missing_reason||'No execution evidence returned'))+'</details>'}).join('')+'<details><summary>Original upstream steps</summary>'+block('Steps',r.original_steps)+'</details>';location.hash=String(i)}
function filter(){let term=document.getElementById('search').value.toLowerCase(),kind=document.getElementById('kind').value;let ids=rows.map((r,i)=>i).filter(i=>(!term||indexed[i].includes(term))&&(!kind||rows[i].classifications.includes(kind)));document.getElementById('count').textContent=ids.length+' matches';document.getElementById('list').innerHTML=ids.map(i=>'<button data-id="'+i+'">'+esc(rows[i].name)+'<br><span class="tag">'+esc(rows[i].classifications.join(', '))+'</span></button>').join('')}
document.getElementById('list').onclick=e=>{const b=e.target.closest('button');if(b)show(Number(b.dataset.id))};document.getElementById('search').oninput=filter;document.getElementById('kind').onchange=filter;filter();if(location.hash&&rows[Number(location.hash.slice(1))])show(Number(location.hash.slice(1)));
</script></html>'''.replace('DATA', data).replace('TOTALS', json.dumps({'statuses':summary['statuses'],'target_query_classifications':dict(counts)}))
    (output / 'index.html').write_text(page)
    print(json.dumps({k: summary[k] for k in ('tests','statuses','target_query_classifications','expected_error_tests')}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--catalog', type=Path, default=ROOT / 'conformance/upstream/catalog.json')
    args = parser.parse_args()
    export(args.report, args.output, args.catalog)
