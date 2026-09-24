"""Render committed upstream evidence only. This module never runs tests."""
import csv,io,json,hashlib
from collections import Counter,defaultdict
from pathlib import Path
from html import escape as e
ROOT=Path(__file__).resolve().parents[2]/'conformance'
PRODUCTS={'crabgraph':'Crabgraph','sqlg':'SQLg','puppygraph':'PuppyGraph'}
SUITE_PRODUCTS={'opencypher':('crabgraph','puppygraph'),'tinkerpop':('crabgraph','sqlg','puppygraph'),'rdf':('crabgraph',)}
PROFILES={'crabgraph-jvm':'Crabgraph · JVM OLTP','crabgraph-computer':'Crabgraph · GraphComputer'}
COLUMNS={**PRODUCTS,**PROFILES}
SUITE_COLUMNS={**SUITE_PRODUCTS,'tinkerpop':('crabgraph','crabgraph-jvm','crabgraph-computer','sqlg','puppygraph')}
def column_name(key,suite):return 'Crabgraph · native Rust' if key=='crabgraph' and suite=='tinkerpop' else COLUMNS[key]
SUITES={'opencypher':'openCypher TCK','tinkerpop':'Apache gremlin-test · Gherkin','rdf':'W3C SPARQL 1.0 / 1.1'}
LABELS={'pass':'Passed','fail':'Failed','unsupported':'Unsupported interface / feature','skipped':'Skipped','not-applicable':'Not applicable','adapter-error':'Adapter limitation / error','timeout':'Timed out','not-run':'Not run','stale':'Stale evidence'}
def pretty(value,limit=6000):
 text=json.dumps(value,ensure_ascii=False,indent=2)
 return '<pre>'+e(text[:limit])+('</pre><p>Excerpt; full evidence is available in the JSON download.</p>' if len(text)>limit else '</pre>')
def result_fingerprint(case):return hashlib.sha256(json.dumps(case,sort_keys=True).encode()).hexdigest()
def render_java_evidence(download):
 source=ROOT/'upstream-results/java-provider';index=source/'index.json'
 html=['<section class="java-evidence" id="java-provider"><h2>Java provider tests</h2><p>Original Java assertions cover cases the upstream Gherkin framework cannot express. Supplemental checks are listed separately. These counts are not added to the Gherkin matrix or product leaderboard.</p>']
 if not index.exists():return ''.join(html)+'<p>No committed Java provider evidence is available.</p></section>'
 manifest=json.loads(index.read_text());entries=manifest if isinstance(manifest,list) else manifest['entries']
 target=download/'java-provider';target.mkdir(exist_ok=True);(target/'index.json').write_bytes(index.read_bytes())
 html.append('<div class="comparison-scroll"><table class="comparison-table"><thead><tr><th scope="col">Test selection</th><th scope="col">Evidence</th><th scope="col">Recorded outcomes</th><th scope="col">Run</th></tr></thead><tbody>')
 for entry in entries:
  relative=Path(entry['file']);path=(source/relative).resolve()
  if not path.is_relative_to(source.resolve()):raise ValueError('Java evidence must stay within java-provider/')
  destination=target/relative;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(path.read_bytes())
  counts=entry.get('counts',{});outcomes=' · '.join(str(count)+' '+LABELS.get(status,status).lower() for status,count in counts.items()) or 'No outcomes recorded'
  kind='Original upstream assertions' if entry.get('upstream_assertions') else 'Supplemental checks'
  complete='Complete' if entry.get('run_complete') else 'Incomplete — recorded cases only'
  html.append('<tr><th scope="row">'+e(entry['label'])+'</th><td>'+kind+'<br><a href="/downloads/conformance/java-provider/'+e(relative.as_posix())+'">Raw results JSON</a></td><td>'+e(outcomes)+'</td><td>'+complete+'</td></tr>')
 html.append('</tbody></table></div><p><a href="/downloads/conformance/java-provider/index.json">Java evidence index JSON</a></p></section>')
 return ''.join(html)
def render(out):
 catalog=json.loads((ROOT/'upstream/catalog.json').read_text());cases=catalog['cases'];runs={};lookup={}
 download=out/'downloads/conformance';download.mkdir(parents=True,exist_ok=True)
 (download/'upstream-catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
 (download/'upstream-sources.json').write_bytes((ROOT/'upstream/sources.json').read_bytes())
 for p in COLUMNS:
  for suite in SUITES:
   if p not in SUITE_COLUMNS[suite]:continue
   path=ROOT/'upstream-results'/f'{p}-{suite}.json'
   if path.exists():
    d=json.loads(path.read_text());runs[p,suite]=d;lookup[p,suite]={r['id']:r for r in d['results']};(download/path.name).write_bytes(path.read_bytes())
 reference=ROOT/'upstream-results/reference-tinkerpop.json'
 if reference.exists():(download/reference.name).write_bytes(reference.read_bytes())
 supplemental=[]
 for filename,label in [('crabgraph-native-build.json','Native build manifest'),('crabgraph-jvm-build.json','JVM build manifest'),('gremlin-final-gap-evidence.json','Gremlin case-to-profile evidence')]:
  path=ROOT/'upstream-results'/filename
  if path.exists():
   (download/filename).write_bytes(path.read_bytes());supplemental.append('<a href="/downloads/conformance/'+filename+'">'+label+'</a>')
 def get(p,c):
  result=lookup.get((p,c['suite']),{}).get(c['id'],{'status':'not-run','reason':'No committed upstream run for this case'})
  if result.get('case_sha256') and result['case_sha256']!=result_fingerprint(c):return {**result,'status':'stale'}
  return result
 html=['<div class="report-meta"><span>6,533 upstream scenarios · 3 products · 3 Crabgraph Gremlin profiles</span><nav aria-label="Comparison sections"><a href="#summary">Suite totals</a><a href="#capabilities">Capabilities</a><a href="#java-provider">Java tests</a><a href="#method">Method</a><a href="/downloads/conformance/upstream-comparison.csv">Download CSV ↓</a></nav></div>']
 from leaderboard import render as render_leaderboard
 html.append(render_leaderboard(cases,get,runs,ROOT,download))
 html.append('<nav class="language-tabs" aria-label="Query languages">'+''.join('<a href="#language-'+suite+'" data-language-tab="'+suite+'">'+label+'<span>'+str(len({c['feature'] for c in cases if c['suite']==suite}))+' features</span></a>' for suite,label in [('tinkerpop','Gremlin'),('opencypher','Cypher'),('rdf','SPARQL')])+'</nav>')
 html.append('<div class="comparison-controls" hidden><label class="feature-search">Find a graph feature<input id="comparison-search" type="search" placeholder="Try count, shortest path, aggregation…" autocomplete="off"></label><label>Results<select id="comparison-filter"><option value="all">All outcomes</option><option value="differences">Different outcomes</option><option value="failures">Failures / timeouts</option><option value="adapter">Adapter limitations</option><option value="unexecuted">Skipped / unsupported</option><option value="crab-wins">Native Crabgraph passes; peer fails</option><option value="peer-wins">Peer passes; native Crabgraph fails</option></select></label><button id="comparison-reset" type="button">Reset</button></div>')
 html.append('<p class="matrix-legend"><span><i class="complete"></i>All passed</span><span><i class="mixed"></i>Some passed</span><span><i class="failed"></i>No passes; failures recorded</span><span><i class="unknown"></i>Not evaluated</span></p><p class="matrix-note">Cells show passed / total upstream scenarios. Gremlin separates Crabgraph native Rust, JVM OLTP and GraphComputer execution; these profiles are not added together or counted as extra products. Select a cell for individual results and timings. All features are listed below.</p><p id="comparison-count" role="status"></p><div class="feature-browser" id="cases"><div class="feature-stage"><p id="empty-stage" hidden>No features match these filters.</p>')
 groups=defaultdict(list)
 for c in cases:groups[(c['suite'],c['feature'])].append(c)
 export=[];current_suite=None
 for (suite,feature),members in sorted(groups.items(),key=lambda item:{'tinkerpop':0,'opencypher':1,'rdf':2}[item[0][0]]):
  if suite!=current_suite:
   if current_suite is not None:html.append('</table></div></section>')
   current_suite=suite
   html.append('<section class="language-group" id="language-'+suite+'" data-suite="'+suite+'"><h2 class="language-heading">'+{'opencypher':'Cypher','tinkerpop':'Gremlin','rdf':'SPARQL'}[suite]+'</h2><div class="feature-matrix-scroll"><table class="feature-matrix"><thead><tr><th scope="col">Feature</th>'+''.join('<th scope="col" data-profile="'+p+'">'+column_name(p,suite)+'<small>'+e(str(runs.get((p,suite),{}).get('build',{}).get('version','Local build')))+'</small></th>' for p in SUITE_COLUMNS[suite])+'</tr></thead>')
  group_id=hashlib.sha256((suite+'/'+feature).encode()).hexdigest()[:20]
  feature_dir=download/'features';feature_dir.mkdir(exist_ok=True)
  evidence_url='/downloads/conformance/features/'+group_id+'.json'
  bundle={'cases':{c['id']:c for c in members},'results':{p:{c['id']:get(p,c) for c in members} for p in SUITE_COLUMNS[suite]}}
  (feature_dir/(group_id+'.json')).write_text(json.dumps(bundle,ensure_ascii=False)+'\n')
  feature_id='feature-'+group_id
  short=feature.split(' - ',1)[-1]
  if suite=='rdf':short=feature.split('/')[-1].replace('-',' ').capitalize()
  display_version=catalog['sources'][suite]['version'] if suite!='rdf' else ('1.0' if feature.startswith('sparql10/') else '1.1')
  language={'opencypher':'Cypher','tinkerpop':'Gremlin','rdf':'SPARQL'}[suite]
  html.append('<tbody class="feature-card" id="'+feature_id+'" data-suite="'+suite+'" data-name="'+e(short)+'" data-language-name="'+language+'"><tr class="feature-matrix-row"><th scope="row"><a href="#tests-'+group_id+'">'+e(short)+'</a><small>'+str(len(members))+' scenarios · '+e(display_version)+' · <a href="'+e(members[0]['source'])+'">Source ↗</a></small></th>')
  for product in SUITE_COLUMNS[suite]:
   name=column_name(product,suite)
   counts=Counter(get(product,c)['status'] for c in members)
   passed=counts['pass'];failed=counts['fail']+counts['timeout'];total=len(members)
   state='complete' if passed==total else 'mixed' if passed else 'failed' if failed else 'unknown'
   label='All passed' if passed==total else 'Mixed results' if passed else 'Failures recorded' if failed else 'Not applicable' if counts['not-applicable']==total else 'Not evaluated'
   version=runs.get((product,suite),{}).get('build',{}).get('version','Local build' if product.startswith('crabgraph') else 'Version not recorded')
   note=' · '.join(str(counts[k])+' '+{'fail':'failed','timeout':'timed out','skipped':'skipped','adapter-error':'adapter limitations','unsupported':'unsupported','not-applicable':'not applicable','stale':'stale','not-run':'not run'}[k] for k in LABELS if k!='pass' and counts[k]) or 'Every scenario passed'
   bars=''.join('<span class="segment '+k+'" style="width:'+str(v/total*100)+'%" title="'+str(v)+' '+e(LABELS[k])+'"></span>' for k,v in counts.items())
   html.append('<td><a class="matrix-cell '+state+'" href="#tests-'+group_id+'" data-product-focus="'+product+'"><strong>'+str(passed)+' / '+str(total)+'</strong><span>'+label+'</span><small>'+e(note)+'</small></a></td>')
  html.append('</tr><tr class="feature-evidence-row"><td colspan="'+str(len(SUITE_COLUMNS[suite])+1)+'"><details class="upstream-group" id="tests-'+group_id+'"><summary>'+e(short)+' — individual scenarios</summary><div class="comparison-scroll" tabindex="0" role="region" aria-label="'+e(feature)+'"><table class="comparison-table"><caption>'+e(SUITES[suite])+'</caption><thead><tr><th scope="col">Upstream scenario</th>'+''.join('<th scope="col">'+column_name(p,suite)+'</th>' for p in SUITE_COLUMNS[suite])+'</tr></thead><tbody>')
  for c in members:
   results={p:get(p,c) for p in SUITE_COLUMNS[suite]};statuses=[r['status'] for r in results.values() if r['status'] in ['pass','fail','timeout']];flags=[]
   if len(set(statuses))>1:flags.append('differences')
   if any(s in ['fail','timeout'] for s in statuses):flags.append('failures')
   if any(r['status'] in ['adapter-error','stale','not-run'] for r in results.values()):flags.append('adapter')
   if any(r['status'] in ['skipped','unsupported'] for r in results.values()):flags.append('unexecuted')
   if results['crabgraph']['status']=='pass' and any(p in results and results[p]['status']=='fail' for p in ['sqlg','puppygraph']):flags.append('crab-wins')
   if results['crabgraph']['status']=='fail' and any(p in results and results[p]['status']=='pass' for p in ['sqlg','puppygraph']):flags.append('peer-wins')
   anchor='case-'+hashlib.sha256(c['id'].encode()).hexdigest()[:16]
   html.append('<tr class="comparison-row" id="'+anchor+'" data-language="'+suite+'" data-flags="'+' '.join(flags)+'"><th scope="row"><a href="#'+anchor+'">'+e(c['name'])+'</a><details data-evidence="'+evidence_url+'" data-case="'+e(c['id'])+'" data-product="upstream"><summary>Scenario and expectation</summary><a href="'+e(c['source'])+'">Pinned upstream source ↗</a> · <a href="'+evidence_url+'">Evidence JSON</a><div class="evidence-content"></div></details></th>')
   for p,r in results.items():
    if p not in SUITE_COLUMNS[suite]:continue
    status=r['status'];elapsed=r.get('elapsed_ms');timing='<span class="timing">'+f'{elapsed:g} ms total</span>' if elapsed is not None and status!='not-applicable' else ''
    html.append('<td data-product-column="'+p+'"><details data-evidence="'+evidence_url+'" data-case="'+e(c['id'])+'" data-product="'+p+'"><summary><span class="status '+status+'">'+e(LABELS[status])+'</span>'+timing+'</summary>')
    if status!='not-run':html.append('<p><a href="/downloads/conformance/'+p+'-'+suite+'.json">Full run JSON</a> · find '+e(c['id'])+'</p>')
    html.append('<a href="'+evidence_url+'">Feature evidence JSON</a><div class="evidence-content"></div></details></td>')
    export.append([c['id'],suite,c['name'],'crabgraph' if p in PROFILES else p,p,status,r.get('elapsed_ms',''),r.get('reason',r.get('error','')),c['source']])
   html.append('</tr>')
  html.append('</tbody></table></div></details></td></tr></tbody>')
 html.append('</table></div></section></div></div><div class="report-appendix"><details class="report-section" id="summary"><summary>Suite totals <span>All 6,533 upstream scenarios</span></summary>')
 for suite,title in SUITES.items():
  subset=[c for c in cases if c['suite']==suite]
  html.append('<h3>'+title+' · '+str(len(subset))+' scenarios</h3><div class="comparison-scroll summary-scroll" tabindex="0" role="region" aria-label="'+title+' result totals"><table class="comparison-table"><caption>'+e(catalog['sources'][suite]['version'])+'</caption><thead><tr>'+''.join('<th scope="col">'+column_name(p,suite)+'</th>' for p in SUITE_COLUMNS[suite])+'</tr></thead><tbody><tr>')
  for p in SUITE_COLUMNS[suite]:
   counts=Counter(get(p,c)['status'] for c in subset)
   html.append('<td>'+''.join('<span class="count-line '+status+'">'+str(counts[status])+' '+e(LABELS[status].lower())+'</span>' for status in LABELS if counts[status])+'</td>')
  html.append('</tr></tbody></table></div>')
 html.append('</details>')
 html.append(render_java_evidence(download))
 caps=json.loads((ROOT/'data/capabilities.json').read_text());caps=[{**c,'cells':{p:v for p,v in c['cells'].items() if p in PRODUCTS}} for c in caps];caps=[c for c in caps if c['cells']]
 (download/'capabilities.json').write_text(json.dumps(caps,indent=2)+'\n')
 html.append('<details class="report-section" id="capabilities"><summary>Product capabilities and editions <span>51 documented capabilities</span></summary><p>These linked documentation claims are separate from the executed suite results. Enterprise features are explicitly marked. “Not assessed” is not an unsupported claim.</p><div class="comparison-scroll" tabindex="0" role="region" aria-label="Product capabilities"><table class="comparison-table"><caption>Documentation claims · <a href="/downloads/conformance/capabilities.json">Sources and review dates JSON</a></caption><thead><tr><th scope="col">Capability</th>'+''.join('<th scope="col">'+n+'</th>' for n in PRODUCTS.values())+'</tr></thead><tbody>')
 for c in caps:
  html.append('<tr><th scope="row"><small>'+e(c['category'])+'</small>'+e(c['title'])+'</th>')
  for p in PRODUCTS:
   v=c['cells'].get(p);html.append('<td>'+('<a class="capability '+v['kind']+'" href="'+e(v['source'])+'">'+e(v['text'])+'</a><small>'+('Paid edition' if v['kind']=='enterprise' else 'Documentation evidence')+'</small>' if v else 'Not assessed')+'</td>')
  html.append('</tr>')
 html.append('''</tbody></table></div></details><details class="report-section" id="method"><summary>Method and interpretation <span>Sources, fixtures and execution profiles</span></summary>
<p><strong>Upstream expectations.</strong> The original feature files are compiled with Cucumber’s Gherkin compiler, including Scenario Outline examples. Apache’s unmodified <code>gremlin-test 3.7.4 StepDefinition</code> methods perform Gremlin assertions. The openCypher adapter executes the upstream steps and compares their original result tables and graph side effects. It never treats a generic exception as a passing TCK error-category assertion: unclassified error type, detail or phase is recorded as an adapter limitation.</p>
<p><strong>RDF semantics.</strong> The W3C adapter loads manifest data and named graphs, checks positive and negative query syntax, and compares SELECT, ASK and graph results with their expected artifacts. It preserves RDF term identity, unbound variables, duplicate rows and global blank-node identity; graph results use isomorphism. Update interfaces, wire protocols, entailment configurations and federated service fixtures are accounted for explicitly.</p>
<p><strong>Fixtures and interfaces.</strong> SQLg uses PostgreSQL and upstream TinkerFactory fixtures. PuppyGraph maps disposable external PostgreSQL fixture tables; mutation outcomes refer to that configuration. For Cypher fixtures only, a local Neo4j instance materializes upstream GIVEN statements; it supplies no expected answers and is not a compared product. Fixtures that cannot be represented faithfully are excluded with a reason. Crabgraph uses a local repository build in hybrid mode and its RDF dataset API. Native result metadata preserves graph identities, numeric widths, paths, sets and typed map keys. Structured Cypher errors are compared against the required type, detail and execution phase.</p>
<p><strong>Gremlin execution profiles.</strong> Native Rust results exercise the Crabgraph traversal engine. JVM OLTP runs pinned TinkerPop traversal and callback machinery over the native CrabGraph provider. GraphComputer runs vertex programs over that provider. Each profile retains its own exclusions, source and binary hashes, and raw results. The product leaderboard uses native Crabgraph results only. Original Java provider tests and supplemental tests appear in the separate Java evidence table; their counts are not added to Gherkin totals.</p>
<p><strong>Timings.</strong> Recorded milliseconds include fixture setup, query execution and adapter work. Individual step timings and result differences are available in the evidence. The leaderboard ranks passed scenarios; these timings are not used as a performance ranking.</p>
<p><strong>Versions and scope.</strong> The TCK is pinned to 2024.3, while PuppyGraph documents openCypher 9. A failure in this newer corpus is not by itself evidence of violating a product’s declared version. Gremlin uses the pinned 3.7.4 language profile. W3C coverage is SPARQL 1.0 and 1.1. These are observed compatibility results, not certification or an overall product ranking. A failure deserves investigation of the engine, adapter and language/version contract.</p>
<p><strong>Local execution only.</strong> Test engines and harnesses run on the local workstation. GitHub Actions only builds and publishes static documentation and committed evidence; it does not run tests or validation jobs. A changed case hash makes old results stale. Version pins, exact source links, complete outcomes and raw diagnostics are downloadable. <a href="https://github.com/henneberger/new-graph/tree/main/conformance">Local reproduction commands and adapter source</a> describe the execution profiles and time limits.</p>
''')
 html.append('</details><details class="report-section" id="versions"><summary>Versions and downloadable evidence</summary><p><a href="/downloads/conformance/upstream-catalog.json">Complete upstream catalog JSON</a> · <a href="/downloads/conformance/upstream-sources.json">Pinned source revisions</a> · <a href="/downloads/conformance/upstream-comparison.csv">Comparison CSV</a> · <a href="/downloads/conformance/reference-tinkerpop.json">Apache reference-engine check</a></p>')
 if supplemental:html.append('<p>'+' · '.join(supplemental)+'</p>')
 for (p,s),d in runs.items():
  html.append('<details class="version-evidence"><summary>'+column_name(p,s)+' · '+SUITES[s]+' · '+e(d['finished_at'][:10])+'</summary>'+pretty({k:v for k,v in d.items() if k!='results'})+'<a href="/downloads/conformance/'+p+'-'+s+'.json">Full evidence JSON</a></details>')
 html.append('</details></div>')
 buf=io.StringIO();w=csv.writer(buf);w.writerow(['upstream_id','suite','scenario','product','execution_profile','status','scenario_wall_ms','diagnostic','upstream_source']);w.writerows(export);(download/'upstream-comparison.csv').write_text(buf.getvalue())
 return '\n'.join(html),[(s,t) for s,t in [('summary','Suite results'),('cases','Upstream cases'),('java-provider','Java provider tests'),('capabilities','Capabilities'),('method','Method'),('versions','Versions')]]
