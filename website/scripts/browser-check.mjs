import { pathToFileURL } from 'node:url';
import { mkdirSync } from 'node:fs';
import assert from 'node:assert/strict';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const base = process.env.SITE_URL || 'http://127.0.0.1:5320';
const docs = process.env.DOCS_URL || 'http://127.0.0.1:5321';
const output = new URL('../../target/site-review/', import.meta.url);
mkdirSync(output, {recursive:true});
const browser = await chromium.launch({headless:true,...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH} : {})});
const expected = ['Getting Started','Resources','Ecosystem','Community','Blog','Docs'];
try {
  const context = await browser.newContext({permissions:['clipboard-read','clipboard-write']});
  const page = await context.newPage();
  const errors=[];
  page.on('pageerror',e => errors.push(e.message));
  for (const width of [1440,1024,768,390,320]) {
    await page.setViewportSize({width,height:900});
    for (const path of ['/','/resources.html','/ecosystem.html','/community.html','/blog.html']) {
      assert.equal((await page.goto(base+path)).status(),200);
      assert.equal(await page.locator('main h1').count(),1);
      assert.deepEqual(await page.locator('.primary-nav a').allTextContents(),expected);
      assert.deepEqual(await page.locator('.site-footer nav[aria-label="Footer"] a').allTextContents(),expected);
      assert.deepEqual(await page.locator('.site-footer nav[aria-label="Social and source"] a').allTextContents(),['Mastodon','Twitter','LinkedIn','Slack','GitHub']);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),false,`${width}: ${path}`);
      if (path === '/') {
        const box = await page.locator('.tagline').boundingBox();
        assert.ok(Math.abs(box.x + box.width / 2 - width / 2) < 1, `Tagline centered at ${width}`);
        assert.equal(await page.locator('.result-box').innerText(), 'Iceberg Data Lake\nMapped tables · via DuckDB');
      }

      // Follow every first-party link against the local builds, including placeholders.
      if (width === 1440) {
        const links=await page.locator('a[href]').evaluateAll(nodes => nodes.map(n => n.getAttribute('href')));
        for (const href of links) {
          const target=new URL(href,base+path);
          const local = target.hostname === 'docs.orchiddb.com' ? docs+target.pathname+target.search : target.origin === base ? target.href : null;
          if (!local) continue;
          const response=await page.request.get(local);
          assert.equal(response.status(),200,href);
          if (target.hash) assert.ok((await response.text()).includes(`id="${target.hash.slice(1)}"`),href);
        }
      }
    }
  }
  await page.goto(base+'/');
  await page.locator('.menu-toggle').click();
  assert.equal(await page.locator('.menu-toggle').getAttribute('aria-expanded'),'true');
  await page.locator('.primary-nav a[href="/community.html"]').click();
  assert.equal(await page.locator('main h1').innerText(),'Community');
  await page.locator('.menu-toggle').click();
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.menu-toggle').getAttribute('aria-expanded'),'false');
  await page.route('https://docs.orchiddb.com/**',async route => route.fulfill({response:await route.fetch({url:route.request().url().replace('https://docs.orchiddb.com',docs)})}));
  await page.setViewportSize({width:1440,height:1000});
  await page.goto(base+'/');
  await page.locator('#site-search').fill('transactions');
  await page.locator('#site-search').press('Enter');
  await page.locator('#mdbook-searchresults a').first().waitFor();
  assert.match(await page.locator('#mdbook-searchresults').innerText(),/Transactions and storage/);
  for (const [width,name] of [[1440,'desktop'],[390,'mobile']]) {
    await page.setViewportSize({width,height:900});
    await page.goto(base+'/');
    await page.screenshot({path:new URL(`orchiddb-${name}.png`,output).pathname,fullPage:true});
  }
  await page.goto(base+'/');
  await page.locator('[data-install="rust"]').click();
  assert.equal(await page.locator('#install-rust').isVisible(), true);
  assert.equal(await page.locator('#install-cli').isVisible(), false);
  await page.locator('[data-copy="install-rust"]').click();
  await page.getByText('Command copied.', {exact:true}).waitFor();
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), 'cargo add orchiddb --git https://github.com/OrchidDB/OrchidDB');
  for (const language of ['python', 'javascript', 'java']) {
    await page.locator(`[data-install="${language}"]`).click();
    const panel = page.locator(`#install-${language}`);
    assert.equal(await panel.isVisible(), true);
    assert.match(await panel.innerText(), /Mock download/);
    assert.equal(await panel.locator('a.button').getAttribute('href'), `https://install.orchiddb.com/mock/orchiddb-${language}-placeholder.zip`);
  }
  await page.locator('[data-install="cli"]').click();
  await page.locator('[data-copy="install-cli"]').click();
  await page.getByText('Command copied.', {exact:true}).waitFor();
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), 'curl -fsSL https://install.orchiddb.com | bash');
  assert.equal(await page.locator('#install-cli').isVisible(), true);
  await page.evaluate(() => scrollTo(0,0));
  const crest = page.locator('.journal-crest');
  assert.equal(await crest.evaluate(image => image.complete && image.naturalWidth > 0), true);
  assert.equal(await page.locator('.hero-graph').count(), 0);
  assert.deepEqual(errors,[]);
  const nojs=await browser.newContext({javaScriptEnabled:false,viewport:{width:390,height:844}});
  const plain=await nojs.newPage();
  await plain.goto(base+'/');
  assert.equal(await plain.locator('#install-cli').isVisible(), true);
  assert.equal(await plain.locator('#install-rust').isVisible(), true);
  assert.equal(await plain.locator('.primary-nav').isVisible(),true);
  await plain.locator('.primary-nav a[href="/resources.html"]').click();
  assert.equal(await plain.locator('main h1').innerText(),'Resources');
  console.log('Checked all project pages at five widths, navigation order, local links/placeholders, menu, docs search, and navigation without JavaScript.');
} finally { await browser.close(); }
