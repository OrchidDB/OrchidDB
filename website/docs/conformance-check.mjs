import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const base=process.env.DOCS_URL || 'http://127.0.0.1:5321';
const browser=await chromium.launch({headless:true,...(process.env.CHROME_PATH?{executablePath:process.env.CHROME_PATH}:{})});
const context=await browser.newContext({viewport:{width:1440,height:1050}});
const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.goto(base+'/conformance.html');
assert.equal(await page.locator('.comparison-row').count(),6533);
assert.equal(await page.locator('.feature-card').count(),361);
assert.equal(await page.locator('.feature-card:visible').count(),361);
const products={tinkerpop:['Crabgraph','SQLg','PuppyGraph'],opencypher:['Crabgraph','PuppyGraph'],rdf:['Crabgraph']};
// All features stay visible; each product has exactly one column.
for(const [suite,names] of Object.entries(products)){
  const valid=await page.locator('.feature-card[data-suite="'+suite+'"]').evaluateAll((cards,names)=>cards.every(card=>{
    const columns=[...card.querySelectorAll('.comparison-table thead th')].slice(1).map(h=>h.textContent);
    return card.querySelectorAll('.feature-matrix-row > td').length===names.length&&JSON.stringify(columns)===JSON.stringify(names);
  }),names);assert.equal(valid,true,suite+' execution columns');
  await page.locator('[data-language-tab="'+suite+'"]').click();
  assert.equal(await page.locator('.feature-card:visible').count(),361);
}
assert.equal(await page.locator('#capabilities .comparison-table thead th').count(),4);
assert.equal(await page.locator('.leaderboard-table').textContent().then(text=>/JVM|GraphComputer/.test(text)),false);
for(const value of ['crab-wins','peer-wins','adapter','failures']){
 await page.locator('#comparison-filter').selectOption(value);
 assert.equal(await page.locator('.comparison-row:not([hidden])').evaluateAll((rows,value)=>rows.every(row=>row.dataset.flags.split(' ').includes(value)),value),true);
}
await page.locator('#comparison-reset').click();
await page.locator('#comparison-search').fill('no-such-feature-123');
await page.locator('#empty-stage').waitFor();assert.equal(await page.locator('.feature-card:visible').count(),0);
await page.locator('#comparison-reset').click();
await page.locator('#comparison-search').fill('groupCount');
await page.waitForFunction(()=>[...document.querySelectorAll('.feature-card:not([hidden])')].some(card=>card.dataset.name==='groupCount()'));
await page.locator('#comparison-reset').click();
await page.screenshot({path:'/tmp/conformance-explorer-desktop.png'});
const card=page.locator('.feature-card[data-suite="tinkerpop"]:visible').first();
await card.locator('[data-product-focus="crabgraph"]').click();
const row=card.locator('.comparison-row').first();
await row.locator('[data-product="crabgraph"] .raw-evidence').waitFor({state:'attached'});
const actual=JSON.parse(await row.locator('[data-product="crabgraph"] .raw-evidence').textContent());
await row.locator('[data-product="upstream"] > summary').click();
await row.locator('[data-product="upstream"] .raw-evidence').waitFor({state:'attached'});
const original=JSON.parse(await row.locator('[data-product="upstream"] .raw-evidence').textContent());
assert.equal(actual.id,original.id);assert.ok(actual.case_sha256);
const cypherAnchor=await page.locator('.feature-card[data-suite="opencypher"] .comparison-row').first().getAttribute('id');
await page.goto(base+'/conformance.html#'+cypherAnchor);
assert.equal(await page.locator('.feature-card:visible').count(),361);
assert.equal(await page.locator('#'+cypherAnchor).isVisible(),true);
assert.equal(await page.locator('#'+cypherAnchor+' [data-product="upstream"]').getAttribute('open'),'');
const evidenceUrl=await page.locator('#'+cypherAnchor+' [data-product="upstream"]').getAttribute('data-evidence');
const bundle=await (await page.request.get(base+evidenceUrl)).json();assert.deepEqual(Object.keys(bundle.results),['crabgraph','puppygraph']);
await page.locator('[data-language-tab="tinkerpop"]').click();
await page.locator('#comparison-reset').click();
for(const width of [1440,1000,760,390]){
 await page.setViewportSize({width,height:844});
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'overflow '+width);
}
await page.evaluate(()=>scrollTo(0,0));await page.screenshot({path:'/tmp/conformance-explorer-mobile.png'});
const javaIndexLink=page.locator('#java-provider a[href$="index.json"]');
if(await javaIndexLink.count()){
 const response=await page.request.get(base+await javaIndexLink.getAttribute('href'));assert.equal(response.status(),200);
 const manifest=await response.json();const entries=Array.isArray(manifest)?manifest:manifest.entries;
 assert.equal(await page.locator('#java-provider tbody tr').count(),entries.length);
 for(const entry of entries){const raw=await page.request.get(base+'/downloads/conformance/java-provider/'+entry.file);assert.equal(raw.status(),200);}
}
assert.deepEqual(errors,[]);await context.close();
const nojs=await browser.newContext({javaScriptEnabled:false,viewport:{width:390,height:844}});
const staticPage=await nojs.newPage();await staticPage.goto(base+'/conformance.html');
assert.equal(await staticPage.locator('.comparison-row').count(),6533);
assert.equal(await staticPage.locator('.comparison-controls').isVisible(),false);
assert.equal(await staticPage.locator('.language-heading:visible').count(),3);
await staticPage.locator('.upstream-group > summary').first().click();
await staticPage.locator('.comparison-row [data-product="upstream"] > summary').first().click();
const download=staticPage.locator('.comparison-row [data-product="upstream"] a').nth(1);
assert.ok(await download.isVisible());
assert.equal((await staticPage.request.get(base+await download.getAttribute('href'))).status(),200);
assert.equal(await staticPage.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
await browser.close();console.log('All 361 visible features, product-level Gremlin results and Java evidence, filters, evidence, deep links, responsive layout and no-JavaScript view passed');
