// PLAYWRIGHT_MODULE can point to an existing local Playwright installation.
import { pathToFileURL } from 'node:url';
import { readFileSync, mkdirSync } from 'node:fs';
import assert from 'node:assert/strict';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const base = process.env.DOCS_URL || 'http://127.0.0.1:5321';
const chapters = [...readFileSync(new URL('./content/SUMMARY.md', import.meta.url), 'utf8').matchAll(/^- \[([^\]]+)\]\(([^)]+)\.md\)$/gm)].map(([,title,slug]) => ({title,slug}));
const screenshots = new URL('../../target/site-review/', import.meta.url);
mkdirSync(screenshots, {recursive:true});
const browser = await chromium.launch({headless:true, ...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH} : {})});
try {
  const context = await browser.newContext({viewport:{width:1440,height:1000}, permissions:['clipboard-read','clipboard-write']});
  const page = await context.newPage();
  const errors=[];
  page.on('pageerror', e => errors.push(e.message));
  for (const width of [1440,390]) {
    await page.setViewportSize({width,height:844});
    for (const {slug,title} of chapters) {
      const response = await page.goto(`${base}/${slug}.html`);
      assert.equal(response.status(), 200, slug);
      assert.equal(await page.locator('main h1').innerText(), title);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, `${width}: ${slug}`);
    }
  }
  await page.setViewportSize({width:1440,height:1000});
  await page.goto(`${base}/mapped-graphs.html`);
  await page.locator('pre').first().hover();
  await page.locator('.clip-button').first().click();
  assert.match(await page.evaluate(() => navigator.clipboard.readText()), /CREATE TABLE users/);
  await page.locator('#mdbook-search-toggle').click();
  await page.locator('#mdbook-searchbar').pressSequentially('transactions');
  await page.locator('#mdbook-searchresults a').first().waitFor();
  assert.match(await page.locator('#mdbook-searchresults').innerText(), /Transactions and storage/);
  await page.locator('#mdbook-searchbar').fill('zzqxvnotfound98765');
  await page.locator('#mdbook-searchbar').press('End');
  await page.waitForFunction(() => document.querySelector('#mdbook-searchresults-header').textContent.includes('No search results'));
  await page.goto(`${base}/?search=transactions`);
  await page.locator('#mdbook-searchresults a').first().waitFor();
  assert.match(await page.locator('#mdbook-searchresults').innerText(), /Transactions and storage/);
  await page.goto(`${base}/quickstart.html`);
  if (!await page.locator('#mdbook-sidebar-toggle-anchor').isChecked()) await page.locator('#mdbook-sidebar-toggle').click();
  await page.screenshot({path:new URL('mdbook-desktop.png',screenshots).pathname});
  await page.setViewportSize({width:390,height:844});
  await page.goto(`${base}/index.html`);
  if (await page.locator('#mdbook-sidebar-toggle').getAttribute('aria-expanded') !== 'true') await page.locator('#mdbook-sidebar-toggle').click();
  await page.locator('#mdbook-sidebar a[href="quickstart.html"]').click();
  assert.equal(await page.locator('main h1').innerText(), 'Quickstart');
  await page.screenshot({path:new URL('mdbook-mobile.png',screenshots).pathname});
  assert.deepEqual(errors, []);
  const nojs = await browser.newContext({javaScriptEnabled:false,viewport:{width:390,height:844}});
  const plain = await nojs.newPage();
  await plain.goto(`${base}/index.html`);
  assert.equal(await plain.locator('main h1').innerText(), 'Introduction');
  if (!await plain.locator('#mdbook-sidebar-toggle-anchor').isChecked()) await plain.locator('#mdbook-sidebar-toggle').click();
  // Allow the stock CSS sidebar transition to finish before entering its no-JS iframe.
  await plain.waitForTimeout(500);
  await plain.frameLocator('#mdbook-sidebar iframe').locator('a[href="quickstart.html"]').click();
  assert.equal(await plain.locator('main h1').innerText(), 'Quickstart');
  console.log(`Checked ${chapters.length} mdBook chapters at desktop/mobile sizes, search, clipboard, sidebar, and navigation without JavaScript.`);
} finally {
  await browser.close();
}
