// Set PLAYWRIGHT_MODULE to a local Playwright package entry point.
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const base = process.env.DOCS_URL || 'http://127.0.0.1:5321';
const browser = await chromium.launch({headless:true, ...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH} : {})});
const context = await browser.newContext({viewport:{width:1440,height:1000},permissions:['clipboard-read','clipboard-write']});
const page = await context.newPage();
const errors=[];
page.on('pageerror',e=>errors.push(e.message));
const manifest = await (await page.request.get(base+'/search-index.json')).json();
for (const entry of manifest) {
  const response = await page.goto(base+entry.url);
  assert.equal(response.status(),200,entry.url);
  await page.locator('main h1').waitFor();
  assert.equal(await page.locator('[aria-current="page"]').count(),1);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,entry.url);
}
await page.goto(base+'/mapped-graphs.html');
await page.screenshot({path:'/tmp/crabgraph-docs-desktop.png',fullPage:false});
await page.locator('.copy').first().click();
assert.match(await page.evaluate(()=>navigator.clipboard.readText()),/CREATE TABLE users/);
await page.locator('#search-open').click();
await page.locator('#search-input').fill('transactions');
await page.locator('.search-result').first().waitFor();
assert.match(await page.locator('.search-result').first().innerText(),/Transactions and storage/);
await page.keyboard.press('Escape');
await page.locator('#search-dialog').waitFor({state:'hidden'});
await page.keyboard.press('/');
await page.locator('#search-input').fill('no-such-crabgraph-topic-zz');
assert.equal(await page.locator('.search-result').count(),0);
await page.keyboard.press('Escape');
await page.setViewportSize({width:390,height:844});
for (const entry of manifest) {
  await page.goto(base+entry.url);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'mobile '+entry.url);
}
await page.goto(base+'/index.html');
await page.locator('#menu').click();
assert.equal(await page.locator('#menu').getAttribute('aria-expanded'),'true');
await page.locator('#sidebar a[href="/quickstart.html"]').click();
assert.match(await page.locator('h1').innerText(),/Quickstart/);
await page.screenshot({path:'/tmp/crabgraph-docs-mobile.png',fullPage:false});
assert.deepEqual(errors,[]);
const nojs=await browser.newContext({javaScriptEnabled:false,viewport:{width:390,height:844}});
const plain=await nojs.newPage();
await plain.goto(base+'/index.html');
assert.equal(await plain.locator('#sidebar').isVisible(),true);
assert.equal(await plain.locator('main h1').innerText(),'Introduction');
await browser.close();
console.log(`Checked ${manifest.length} pages at desktop and mobile sizes, search, clipboard, menu, and navigation without JavaScript.`);
