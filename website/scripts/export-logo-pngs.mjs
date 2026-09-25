// Rasterize the exact production SVG without changing its geometry or ink color.
// PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs CHROME_PATH=/path/to/chrome node website/scripts/export-logo-pngs.mjs
import {pathToFileURL} from 'node:url';
import {readFile,copyFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const directory=new URL('../assets/logos/',import.meta.url);
const svg=await readFile(new URL('orchiddb.svg',directory),'utf8');
const browser=await chromium.launch({headless:true,...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH} : {})});
try {
  const page=await browser.newPage({deviceScaleFactor:1});
  for(const size of [50,128,256,512,1024]) {
    await page.setViewportSize({width:size,height:size});
    await page.setContent(`<style>html,body{margin:0;background:transparent}svg{display:block;width:100vw;height:100vh}</style>${svg}`);
    const png=await page.screenshot({path:new URL(`orchiddb-${size}.png`,directory).pathname,omitBackground:true});
    assert.equal(png.readUInt32BE(16),size);
    assert.equal(png.readUInt32BE(20),size);
    assert.equal(png[25],6,'PNG must retain RGBA');
    const pixels=await page.evaluate(async src=>{
      const image=new Image();image.src=src;await image.decode();
      const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;
      const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);
      const data=ctx.getImageData(0,0,image.width,image.height).data;
      let transparent=false,ink=false;
      for(let i=0;i<data.length;i+=4){if(data[i+3]===0)transparent=true;if(data[i+3]===255&&data[i]===53&&data[i+1]===35&&data[i+2]===48)ink=true;}
      return {transparent,ink};
    },'data:image/png;base64,'+png.toString('base64'));
    assert.deepEqual(pixels,{transparent:true,ink:true});
    console.log(`orchiddb-${size}.png: ${size}×${size}, transparent, #352330 ink`);
  }
  await copyFile(new URL('orchiddb-1024.png',directory),new URL('orchiddb.png',directory));
} finally {await browser.close();}
