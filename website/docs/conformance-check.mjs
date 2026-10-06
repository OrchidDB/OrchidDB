// Verify the current extension evidence exposed by the locally built book.
import {pathToFileURL} from 'node:url';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const base=process.env.DOCS_URL || 'http://127.0.0.1:5321';
const browser=await chromium.launch({headless:true,...(process.env.CHROME_PATH?{executablePath:process.env.CHROME_PATH}:{})});
try {
  const page=await browser.newPage();
  const response=await page.goto(base+'/conformance.html');
  assert.equal(response.status(),200);
  assert.equal(await page.locator('main h1').innerText(),'Conformance');
  for (const language of ['cypher','gremlin','sparql']) assert.equal(await page.locator('#'+language).count(),1);
  const summary=await (await page.request.get(base+'/downloads/conformance/summary.json')).json();
  const expected={cypher:{pass:3897},gremlin:{pass:1511},rdf:{pass:974,skipped:77,'not-applicable':74}};
  for (const [language,entry] of Object.entries(summary.reports)) {
    const download=await page.request.get(base+'/downloads/conformance/'+entry.path);
    assert.equal(download.status(),200);
    const bytes=await download.body();
    assert.equal(createHash('sha256').update(bytes).digest('hex'),entry.sha256);
    const report=JSON.parse(gunzipSync(bytes));
    assert.deepEqual(report.counts,expected[language]);
    assert.equal(report.build.extension.sha256,summary.extension_sha256);
    assert.equal(report.results.length,report.coverage.catalog_cases);
    assert.equal(new Set(report.results.map(r=>r.id)).size,report.results.length);
    if(language==='gremlin') assert.equal(report.execution_profile.single_instance_verified,true);
  }
  console.log('Checked extension evidence downloads, checksums, counts, unique cases, and shared artifact identity.');
} finally { await browser.close(); }
