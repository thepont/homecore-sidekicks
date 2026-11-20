import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require('playwright');
const ui=new URL('../../ui/',import.meta.url);
test('an unconfigured integration invites an admin to begin QR sign-in',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');
    await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{window.calls=[];const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>{calls.push(name);return {ok:true,data:{status:'unconfigured'}}}};document.querySelector('main').append(el)});
    assert.equal(await page.getByText('Connect Xiaomi',{exact:true}).count(),1);
    assert.equal(await page.getByRole('button',{name:'Sign in with Mi Home'}).count(),1);
  } finally {await browser.close();}
});

test('starting sign-in shows the QR returned by the integration',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{window.calls=[];const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>{calls.push(name);return name==='begin_sign_in'?{ok:true,data:{qr_png:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='}}:{ok:true,data:{status:'waiting'}}}};document.querySelector('main').append(el)});
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    assert.equal(await page.getByRole('img',{name:'Mi Home sign-in QR code'}).count(),1);
    assert.deepEqual(await page.evaluate(()=>calls),['begin_sign_in']);
  } finally {await browser.close();}
});
