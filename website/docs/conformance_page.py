"""Render committed upstream evidence only. This module never runs tests."""
import csv,io,json,hashlib
from collections import Counter,defaultdict
from pathlib import Path
from html import escape as e
ROOT=Path(__file__).resolve().parents[2]/'conformance'
PRODUCTS={'crabgraph':'Crabgraph','sqlg':'SQLg','puppygraph':'PuppyGraph'}
SUITES={'opencypher':'openCypher TCK','tinkerpop':'Apache gremlin-test · Gherkin','rdf':'W3C SPARQL 1.0 / 1.1'}
LABELS={'pass':'Passed','fail':'Failed','unsupported':'Unsupported interface / feature','skipped':'Skipped','not-applicable':'Not applicable','adapter-error':'Adapter limitation / error','timeout':'Timed out','not-run':'Not run','stale':'Stale evidence'}
def pretty(value,limit=6000):
 text=json.dumps(value,ensure_ascii=False,indent=2)
 return '<pre>'+e(text[:limit])+('</pre><p>Excerpt; full evidence is available in the JSON download.</p>' if len(text)>limit else '</pre>')
def result_fingerprint(case):return hashlib.sha256(json.dumps(case,sort_keys=True).encode()).hexdigest()
def render(out):
 catalog=json.loads((ROOT/'upstream/catalog.json').read_text());cases=catalog['cases'];runs={};lookup={}
 download=out/'downloads/conformance';download.mkdir(parents=True,exist_ok=True)
 (download/'upstream-catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
 (download/'upstream-sources.json').write_bytes((ROOT/'upstream/sources.json').read_bytes())
 for p in PRODUCTS:
  for suite in SUITES:
   path=ROOT/'upstream-results'/f'{p}-{suite}.json'
   if path.exists():
    d=json.loads(path.read_text());runs[p,suite]=d;lookup[p,suite]={r['id']:r for r in d['results']};(download/path.name).write_bytes(path.read_bytes())
 reference=ROOT/'upstream-results/reference-tinkerpop.json'
 if reference.exists():(download/reference.name).write_bytes(reference.read_bytes())
 def get(p,c):
  result=lookup.get((p,c['suite']),{}).get(c['id'],{'status':'not-run','reason':'No committed upstream run for this case'})
  if result.get('case_sha256') and result['case_sha256']!=result_fingerprint(c):return {**result,'status':'stale'}
  return result
 html=['<nav class="comparison-jumps" aria-label="Comparison sections"><a href="#summary">Suite results</a><a href="#cases">Every upstream case</a><a href="#capabilities">Product capabilities</a><a href="#method">Method</a><a href="#versions">Versions and downloads</a></nav>',
 '<p>This comparison uses the original openCypher TCK, Apache TinkerPop gremlin-test Gherkin scenarios, and W3C SPARQL manifests. Fixtures and expected results come from pinned upstream revisions. The three products are Crabgraph, SQLg and PuppyGraph, using free editions.</p>',
 '<h2 id="summary">Suite results</h2><p>All '+str(len(cases))+' upstream scenarios are accounted for below. Passes require the scenario’s assertions to succeed. Failures, unsupported interfaces, explicit skips, adapter problems and timeouts remain separate. Counts apply to these pinned suites and execution profiles.</p>',
 '<div class="comparison-scroll summary-scroll" tabindex="0" role="region" aria-label="Suite result totals"><table class="comparison-table"><caption>Recorded outcomes, including scenarios not executed</caption><thead><tr><th scope="col">Upstream suite</th>'+''.join('<th scope="col">'+name+'</th>' for name in PRODUCTS.values())+'</tr></thead><tbody>']
 for suite,title in SUITES.items():
  subset=[c for c in cases if c['suite']==suite];html.append('<tr><th scope="row">'+title+'<small>'+str(len(subset))+' scenarios · '+e(catalog['sources'][suite]['version'])+'</small></th>')
  for p in PRODUCTS:
   counts=Counter(get(p,c)['status'] for c in subset);html.append('<td>'+''.join('<span class="count-line '+status+'">'+str(counts[status])+' '+e(LABELS[status].lower())+'</span>' for status in LABELS if counts[status])+'</td>')
  html.append('</tr>')
 html.append('</tbody></table></div><p class="timing-note">SQLg is compared on Gremlin; PuppyGraph on Cypher and Gremlin; Crabgraph on all three languages. An absent query interface is not counted as a failed query. The Gherkin comparison does not include TinkerPop’s JVM structure or GraphComputer suites.</p>')
 html.append('<h2 id="cases">Every upstream case</h2><div class="comparison-controls" hidden><label>Search cases<input id="comparison-search" type="search" placeholder="Feature, scenario name or upstream ID"></label><label>Show<select id="comparison-filter"><option value="all">All outcomes</option><option value="differences">Different observed outcomes</option><option value="failures">Failures and timeouts</option><option value="adapter">Adapter limitations / errors</option><option value="unexecuted">Skipped / unsupported</option><option value="crab-wins">Crabgraph passes; peer fails</option><option value="peer-wins">Peer passes; Crabgraph fails</option></select></label><label>Suite<select id="comparison-language"><option value="all">All suites</option>'+''.join('<option value="'+s+'">'+e(t)+'</option>' for s,t in SUITES.items())+'</select></label><button id="comparison-reset" type="button">Reset</button><p id="comparison-count" role="status"></p></div>')
 html.append('<p>Expand a feature group, then a case or result. Each case links to its original source and records the expected and actual output or diagnostic. Time is total local scenario wall time, including fixture and adapter work; query/step measurements are available inside the evidence. These measurements are not a controlled performance benchmark.</p>')
 groups=defaultdict(list)
 for c in cases:groups[(c['suite'],c['feature'])].append(c)
 export=[]
 for (suite,feature),members in groups.items():
  group_id=hashlib.sha256((suite+'/'+feature).encode()).hexdigest()[:20]
  feature_dir=download/'features';feature_dir.mkdir(exist_ok=True)
  evidence_url='/downloads/conformance/features/'+group_id+'.json'
  bundle={'cases':{c['id']:c for c in members},'results':{p:{c['id']:get(p,c) for c in members} for p in PRODUCTS}}
  (feature_dir/(group_id+'.json')).write_text(json.dumps(bundle,ensure_ascii=False)+'\n')
  html.append('<details class="upstream-group" data-suite="'+suite+'"><summary>'+e(feature)+' <small>'+str(len(members))+' scenarios</small></summary><div class="comparison-scroll" tabindex="0" role="region" aria-label="'+e(feature)+'"><table class="comparison-table"><caption>'+e(SUITES[suite])+'</caption><thead><tr><th scope="col">Upstream scenario</th>'+''.join('<th scope="col">'+n+'</th>' for n in PRODUCTS.values())+'</tr></thead><tbody>')
  for c in members:
   results={p:get(p,c) for p in PRODUCTS};statuses=[r['status'] for r in results.values() if r['status'] in ['pass','fail','timeout']];flags=[]
   if len(set(statuses))>1:flags.append('differences')
   if any(s in ['fail','timeout'] for s in statuses):flags.append('failures')
   if any(r['status'] in ['adapter-error','stale','not-run'] for r in results.values()):flags.append('adapter')
   if any(r['status'] in ['skipped','unsupported'] for r in results.values()):flags.append('unexecuted')
   if results['crabgraph']['status']=='pass' and any(results[p]['status']=='fail' for p in ['sqlg','puppygraph']):flags.append('crab-wins')
   if results['crabgraph']['status']=='fail' and any(results[p]['status']=='pass' for p in ['sqlg','puppygraph']):flags.append('peer-wins')
   anchor='case-'+hashlib.sha256(c['id'].encode()).hexdigest()[:16]
   html.append('<tr class="comparison-row" id="'+anchor+'" data-language="'+suite+'" data-flags="'+' '.join(flags)+'"><th scope="row"><a href="#'+anchor+'">'+e(c['name'])+'</a><small>'+e(c['id'])+'</small><details data-evidence="'+evidence_url+'" data-case="'+e(c['id'])+'" data-product="upstream"><summary>Scenario and expectation</summary><a href="'+e(c['source'])+'">Pinned upstream source ↗</a> · <a href="'+evidence_url+'">Evidence JSON</a><div class="evidence-content"></div></details></th>')
   for p,r in results.items():
    status=r['status'];elapsed=r.get('elapsed_ms');timing='<span class="timing">'+f'{elapsed:g} ms total</span>' if elapsed is not None and status!='not-applicable' else ''
    html.append('<td><details data-evidence="'+evidence_url+'" data-case="'+e(c['id'])+'" data-product="'+p+'"><summary><span class="status '+status+'">'+e(LABELS[status])+'</span>'+timing+'</summary>')
    if status!='not-run':html.append('<p><a href="/downloads/conformance/'+p+'-'+suite+'.json">Full run JSON</a> · find '+e(c['id'])+'</p>')
    html.append('<a href="'+evidence_url+'">Feature evidence JSON</a><div class="evidence-content"></div></details></td>')
    export.append([c['id'],suite,c['name'],p,status,r.get('elapsed_ms',''),r.get('reason',r.get('error','')),c['source']])
   html.append('</tr>')
  html.append('</tbody></table></div></details>')
 caps=json.loads((ROOT/'data/capabilities.json').read_text());caps=[{**c,'cells':{p:v for p,v in c['cells'].items() if p in PRODUCTS}} for c in caps];caps=[c for c in caps if c['cells']]
 (download/'capabilities.json').write_text(json.dumps(caps,indent=2)+'\n')
 html.append('<h2 id="capabilities">Product capabilities and editions</h2><p>These linked documentation claims are separate from the executed suite results. Enterprise features are explicitly marked. “Not assessed” is not an unsupported claim.</p><div class="comparison-scroll" tabindex="0" role="region" aria-label="Product capabilities"><table class="comparison-table"><caption>Reviewed documentation · 23 September 2026</caption><thead><tr><th scope="col">Capability</th>'+''.join('<th scope="col">'+n+'</th>' for n in PRODUCTS.values())+'</tr></thead><tbody>')
 for c in caps:
  html.append('<tr><th scope="row"><small>'+e(c['category'])+'</small>'+e(c['title'])+'</th>')
  for p in PRODUCTS:
   v=c['cells'].get(p);html.append('<td>'+('<a class="capability '+v['kind']+'" href="'+e(v['source'])+'">'+e(v['text'])+'</a><small>'+('Paid edition' if v['kind']=='enterprise' else 'Documentation evidence')+'</small>' if v else 'Not assessed')+'</td>')
  html.append('</tr>')
 html.append('''</tbody></table></div><h2 id="method">Method and interpretation</h2>
<p><strong>Upstream expectations.</strong> The original feature files are compiled with Cucumber’s Gherkin compiler, including Scenario Outline examples. Apache’s unmodified <code>gremlin-test 3.7.4 StepDefinition</code> methods perform Gremlin assertions. The openCypher adapter executes the upstream steps and compares their original result tables and graph side effects. It never treats a generic exception as a passing TCK error-category assertion: unclassified error type, detail or phase is recorded as an adapter limitation.</p>
<p><strong>RDF semantics.</strong> The W3C adapter loads manifest data and named graphs, checks positive and negative query syntax, and compares SELECT, ASK and graph results with their expected artifacts. It preserves RDF term identity, unbound variables, duplicate rows and global blank-node identity; graph results use isomorphism. Update interfaces, wire protocols, entailment configurations and federated service fixtures are accounted for explicitly.</p>
<p><strong>Fixtures and interfaces.</strong> SQLg uses PostgreSQL and upstream TinkerFactory fixtures. PuppyGraph maps disposable external PostgreSQL fixture tables; mutation outcomes refer to that configuration. For Cypher fixtures only, a local Neo4j instance materializes upstream GIVEN statements; it supplies no expected answers and is not a compared product. Fixtures that cannot be represented faithfully are excluded with a reason. Crabgraph uses a local repository build in hybrid mode and its RDF dataset API. Its formatted graph-value transport can prevent typed assertions; those cases are adapter limitations, not established semantic defects.</p>
<p><strong>Versions and scope.</strong> The TCK is pinned to 2024.3, while PuppyGraph documents openCypher 9. A failure in this newer corpus is not by itself evidence of violating a product’s declared version. Gremlin uses the pinned 3.7.4 language profile. W3C coverage is SPARQL 1.0 and 1.1. These are observed compatibility results, not certification or an overall product ranking. A failure deserves investigation of the engine, adapter and language/version contract.</p>
<p><strong>Local execution only.</strong> Test engines and harnesses run on the local workstation. GitHub Actions only builds and publishes static documentation and committed evidence; it does not run tests or validation jobs. A changed case hash makes old results stale. Version pins, exact source links, complete outcomes and raw diagnostics are downloadable. <a href="https://github.com/henneberger/new-graph/tree/main/conformance">Local reproduction commands and adapter source</a> describe the execution profiles and time limits.</p>
''')
 html.append('<h2 id="versions">Versions and downloadable evidence</h2><p><a href="/downloads/conformance/upstream-catalog.json">Complete upstream catalog JSON</a> · <a href="/downloads/conformance/upstream-sources.json">Pinned source revisions</a> · <a href="/downloads/conformance/upstream-comparison.csv">Comparison CSV</a> · <a href="/downloads/conformance/reference-tinkerpop.json">Apache reference-engine check</a></p>')
 for (p,s),d in runs.items():
  html.append('<details class="version-evidence"><summary>'+PRODUCTS[p]+' · '+SUITES[s]+' · '+e(d['finished_at'][:10])+'</summary>'+pretty({k:v for k,v in d.items() if k!='results'})+'<a href="/downloads/conformance/'+p+'-'+s+'.json">Full evidence JSON</a></details>')
 buf=io.StringIO();w=csv.writer(buf);w.writerow(['upstream_id','suite','scenario','product','status','scenario_wall_ms','diagnostic','upstream_source']);w.writerows(export);(download/'upstream-comparison.csv').write_text(buf.getvalue())
 return '\n'.join(html),[(s,t) for s,t in [('summary','Suite results'),('cases','Upstream cases'),('capabilities','Capabilities'),('method','Method'),('versions','Versions')]]
