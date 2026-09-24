"""Static comparison renderer. No client-side fetch or service required."""
import csv,hashlib,io,json,importlib.util
from pathlib import Path
from html import escape as e
ROOT=Path(__file__).resolve().parents[2]/'conformance'
spec=importlib.util.spec_from_file_location('conformance_compare',ROOT/'compare.py')
compare=importlib.util.module_from_spec(spec);spec.loader.exec_module(compare)
PRODUCTS={'crabgraph':'Crabgraph','ladybug':'Ladybug','neo4j':'Neo4j','puppygraph':'PuppyGraph','sqlg':'SQLg'}
LABELS={'pass':'Matches expected','mismatch':'Result differs','query-error':'Query rejected','timeout':'Timed out','harness-error':'Harness error','stale':'Stale result'}
def code(x):return '<pre>'+e(json.dumps(x,ensure_ascii=False,indent=2))+'</pre>'
def render(out):
 probes=json.loads((ROOT/'probes.json').read_text())['tests']
 caps=json.loads((ROOT/'data/capabilities.json').read_text())
 inventory=json.loads((ROOT/'data/inventory.json').read_text())
 results={p:json.loads((ROOT/f'results/{p}.json').read_text()) for p in PRODUCTS}
 investigations=json.loads((ROOT/'data/investigations.json').read_text())
 lookup={p:{r['id']:r for r in d['results']} for p,d in results.items()}
 download=out/'downloads/conformance';download.mkdir(parents=True,exist_ok=True)
 for file in [ROOT/'probes.json',*ROOT.glob('results/*.json'),*ROOT.glob('data/*.json')]:
  (download/file.name).write_bytes(file.read_bytes())
 chunks=['<nav class="comparison-jumps" aria-label="Comparison sections"><a href="#tested-queries">Query results</a><a href="#capabilities">Product capabilities</a><a href="#corpus">Full corpus inventory</a><a href="#method">Method and updates</a><a href="#versions">Versions</a></nav>']
 chunks.append('<p>Compare the exact behavior of five free editions. Open any result for its output or error. The capability table includes source links and clearly labels paid features.</p>')
 chunks.append('<div class="comparison-controls" hidden><label>Search features<input id="comparison-search" type="search" placeholder="e.g. paths, null, vector, backup"></label><label>Show<select id="comparison-filter"><option value="all">Everything</option><option value="differences">Different query outcomes</option><option value="crab-wins">Crabgraph matches; a peer differs</option><option value="peer-wins">A peer matches; Crabgraph differs</option><option value="enterprise">Enterprise features</option></select></label><label>Language<select id="comparison-language"><option value="all">All languages</option><option value="cypher">Cypher</option><option value="gremlin">Gremlin</option><option value="sparql">SPARQL</option></select></label><button id="comparison-reset" type="button">Reset</button><p id="comparison-count" role="status"></p></div>')
 chunks.append('<h2 id="tested-queries">Tested queries</h2><p>'+str(len(probes))+' probes cover patterns, paths, joins, subqueries, collections, aggregates, nulls, functions, mutations and traversal state. A pass applies to the displayed query and fixture. Query rejection can reflect a dialect difference; it does not establish that every equivalent expression is unavailable.</p><p class="comparison-key"><span class="status pass">Matches expected</span> · <span class="status mismatch">Result differs</span> different rows · <span class="status query-error">Query rejected</span> engine error · No adapter: this language was not run for that product.</p>')
 def table_start(caption):return '<div class="comparison-scroll" tabindex="0" role="region" aria-label="'+caption+'"><table class="comparison-table"><caption>'+caption+'</caption><thead><tr><th scope="col">Feature / exact probe</th>'+''.join('<th scope="col">'+v+'</th>' for v in PRODUCTS.values())+'</tr></thead><tbody>'
 chunks.append(table_start('Executable query comparison'))
 export=[]
 for t in probes:
  fingerprint=hashlib.sha256(json.dumps(t,sort_keys=True).encode()).hexdigest()
  cells={}
  for p in PRODUCTS:
   r=lookup[p].get(t['id'])
   if r and r.get('probe_sha256')!=fingerprint:r={**r,'status':'stale'}
   cells[p]=r
  statuses=[r['status'] for r in cells.values() if r]
  observed=[r for r in cells.values() if r]
  different=len(set(statuses))>1 or any('rows' in a and 'rows' in b and not compare.equal_rows(a['rows'],b['rows'],t['ordered']) for a in observed for b in observed)
  crab=cells['crabgraph'];peers=[r for p,r in cells.items() if p!='crabgraph' and r]
  flags=[]
  if different:flags.append('differences')
  if crab and crab['status']=='pass' and any(r['status'] in ['mismatch','query-error'] for r in peers):flags.append('crab-wins')
  if crab and crab['status'] in ['mismatch','query-error'] and any(r['status']=='pass' for r in peers):flags.append('peer-wins')
  basis={'gremlin':'Expectation corroborated by TinkerGraph 3.7.4; provider differences still require review.','cypher':'Manually derived expectation, corroborated by Neo4j Cypher 5. Neo4j is not an independent standards oracle.','sparql':'Manually derived expectation; independent RDF reference validation pending.'}[t['language']]
  investigation=investigations.get(t['id'],{'state':'No semantic adjudication recorded','note':'A disagreement may be a dialect choice, fixture or adapter issue, expectation error, or engine defect. Inspect all outputs before deciding.'})
  review='<p class="evidence-label">'+e(basis)+'</p><p><strong>'+e(investigation['state'])+'</strong>: '+e(investigation['note'])+'</p>'
  if investigation.get('source'):review+='<a href="'+e(investigation['source'])+'">Semantic reference</a>'
  chunks.append(f'<tr class="comparison-row" data-language="{t["language"]}" data-flags="{" ".join(flags)}" id="{t["id"]}"><th scope="row"><small>{e(t["category"])}</small><a href="#{t["id"]}">{e(t["title"])}</a><details><summary>Query and expected result</summary><pre>{e(t["query"])}</pre><p>Expected rows ({"ordered" if t["ordered"] else "unordered; duplicates preserved"})</p>{code(t["expected"])}{review}</details></th>')
  for p,r in cells.items():
   if not r:chunks.append('<td><span class="not-tested">No adapter</span></td>');continue
   label=LABELS[r['status']]
   timing=r.get('timing',{});median=timing.get('median_ms',r.get('elapsed_ms',0))
   difference=''
   if r['status']=='mismatch' and 'rows' in r:
    missing=[];extra=list(r['rows'])
    for wanted in t['expected']:
     hit=next((i for i,v in enumerate(extra) if compare.equivalent(v,wanted)),None)
     if hit is None:missing.append(wanted)
     else:extra.pop(hit)
    difference='<p>Expected</p>'+code(t['expected'])+'<p>Actual</p>'+code(r['rows'])+'<p>Missing rows (with multiplicity)</p>'+code(missing)+'<p>Additional rows (with multiplicity)</p>'+code(extra)
    if t['ordered'] and not missing and not extra:difference+='<p>Same rows, different order.</p>'
   detail={k:v for k,v in r.items() if k not in ['id','status','probe_sha256']}
   chunks.append('<td><details><summary><span class="status '+r['status']+'">'+label+'</span><span class="timing">'+f'{median:g} ms median'+'</span></summary><p>'+e(results[p]['finished_at'][:10])+'</p>'+difference+'<p>Recorded evidence and timing samples</p>'+code(detail)+'</details></td>')
   export.append([t['id'],t['category'],t['title'],p,results[p]['build']['version'],r['status'],t['query'],json.dumps(t['expected']),json.dumps(r.get('rows')),r.get('error',''),results[p]['finished_at'],median,timing.get('min_ms',''),timing.get('max_ms',''),json.dumps(timing.get('samples_ms',[])),investigation['state']])
  chunks.append('</tr>')
 chunks.append('</tbody></table></div>')
 chunks.append('<h2 id="capabilities">Product capabilities and editions</h2><p>Documentation evidence, reviewed 23 September 2026. These entries describe availability; they are separate from executed tests. “Not assessed” means no reviewed claim is recorded in this table. Extensions and backend-specific features are named in their cells.</p>')
 chunks.append(table_start('Architecture, integrations, indexing, operations and editions'))
 for i,row in enumerate(caps):
  flags='enterprise' if any(c['kind']=='enterprise' for c in row['cells'].values()) else ''
  chunks.append(f'<tr class="comparison-row" data-language="capability" data-flags="{flags}"><th scope="row"><small>{e(row["category"])}</small>{e(row["title"])}</th>')
  for p in PRODUCTS:
   c=row['cells'].get(p)
   chunks.append('<td><a class="capability '+c['kind']+'" href="'+e(c['source'])+'">'+e(c['text'])+'</a><small>'+('Paid edition' if c['kind']=='enterprise' else 'Documentation')+'</small></td>' if c else '<td><span class="not-tested">Not assessed</span></td>')
  chunks.append('</tr>')
 chunks.append('</tbody></table></div>')
 chunks.append('<h2 id="corpus">Full imported corpus inventory</h2><p>'+str(len(inventory))+' imported cases, grouped below with every case linked to its source. This inventory includes the repository’s Ladybug and TinkerPop corpora; these cases have not been cross-executed by the portable runner. Corpus size is not a pass count. The versioned case digest is available in the download.</p>')
 groups={}
 for row in inventory:groups.setdefault((row['suite'],row['group']),[]).append(row)
 for (suite,group),rows in groups.items():
  chunks.append('<details class="corpus-group"><summary>'+e(suite+' / '+group)+f' <small>{len(rows)} cases</small></summary><ul>'+''.join('<li><a href="https://github.com/henneberger/new-graph/blob/main/'+e(r['path'])+'">'+e(r['id'])+'</a></li>' for r in rows)+'</ul></details>')
 chunks.append('''<h2 id="method">Method and keeping this current</h2>
<p>The shared fixture contains four people and four directed KNOWS edges, including a cycle and one absent age. Every product receives the same logical data. Ladybug uses declared node and relationship tables; PuppyGraph maps PostgreSQL tables; SQLg stores the graph in PostgreSQL. Crabgraph uses hybrid execution. Neo4j probes explicitly select Cypher 5.</p>
<p>Expected results are authored in the suite. All 73 Gremlin expectations were checked against TinkerGraph 3.7.4. Comparisons preserve duplicate rows, nulls and list order; top-level order is required only when specified. Numbers use a 10⁻⁹ tolerance. Mutations are rolled back by transactional adapters. PuppyGraph is queried through its read interface. Adapter failures are recorded separately from query errors. Each timing is client wall time over three consecutive executions, including the first; the cell shows the median and the evidence includes all samples, minimum and maximum. There is no warmup or concurrency load. In-process and network adapters have different overheads, and this four-node fixture is not a performance benchmark. Repeated outputs are retained to expose instability.</p>
<p>The probes exercise selected Cypher syntax, TinkerPop 3.7.4 semantics and mapped SPARQL queries. They do not establish complete openCypher, ISO GQL, TinkerPop or W3C certification. Neo4j 25 extensions, RDF datasets, constraints, concurrency, recovery and deployment behavior require their own fixtures; documentation entries remain documentation evidence until such a test is added.</p>
<p>GitHub Actions reruns the free engines weekly, on relevant main-branch commits and manually. Results are versioned JSON artifacts. The scheduled publication updates this static page through the same personal AWS credentials used by the website. Each cell is tied to a probe hash; changed expectations cannot silently reuse old results. The Crabgraph snapshot is retained when a source revision predates its engine API; other build or harness failures block refresh.</p>
<p>Add a probe, fixture and independently justified expectation in <a href="https://github.com/henneberger/new-graph/tree/main/conformance">the conformance suite</a>. Add language-specific equivalents as separate tests. Review edition claims when upgrading pinned versions. The complete corpus inventory provides the queue for broader fixture coverage.</p>
<p>Specifications and references: <a href="https://opencypher.org/resources/">openCypher</a> · <a href="https://tinkerpop.apache.org/docs/3.7.4/reference/">TinkerPop 3.7.4</a> · <a href="https://www.w3.org/TR/sparql11-query/">SPARQL 1.1</a> · <a href="https://neo4j.com/docs/cypher-manual/current/appendix/gql-conformance/">GQL feature accounting</a> · <a href="https://docs.ladybugdb.com/cypher/difference/">Ladybug dialect differences</a>.</p>''')
 chunks.append('<h2 id="versions">Versions and downloadable evidence</h2><p><a href="/downloads/conformance/comparison.csv">Query results CSV</a> · <a href="/downloads/conformance/probes.json">Fixture and expected results JSON</a> · <a href="/downloads/conformance/capabilities.json">Capability evidence JSON</a> · <a href="/downloads/conformance/inventory.json">Full corpus JSON</a></p>')
 for p,d in results.items():
  counts={}
  for t in probes:
   r=lookup[p].get(t['id'])
   if r:
    v=counts.setdefault(t['language'],[0,0]);v[1]+=1;v[0]+=r['status']=='pass'
  chunks.append('<details class="version-evidence"><summary>'+PRODUCTS[p]+' '+e(d['build']['version'])+' · '+e(d['finished_at'][:10])+'</summary><p>'+e('; '.join(f'{lang}: {a}/{b} probes passed' for lang,(a,b) in counts.items()))+'</p>'+code({'build':d['build'],'execution_environment':d.get('execution_environment',{})})+'<a href="/downloads/conformance/'+p+'.json">Download full run</a></details>')
 buf=io.StringIO();w=csv.writer(buf);w.writerow(['probe','category','title','product','version','status','query','expected','actual','error','tested_at','median_ms','min_ms','max_ms','samples_ms','investigation_state']);w.writerows(export);(download/'comparison.csv').write_text(buf.getvalue())
 return '\n'.join(chunks),[(x,y) for x,y in [('tested-queries','Tested queries'),('capabilities','Capabilities'),('corpus','Corpus inventory'),('method','Method'),('versions','Versions')]]
