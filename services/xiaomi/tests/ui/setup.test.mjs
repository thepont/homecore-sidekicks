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
    await page.evaluate(()=>{window.calls=[];const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>{calls.push(name);return name==='begin_sign_in'?{ok:true,data:{qr_png:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='}}:{ok:true,data:{status:'ok',setup_status:'waiting'}}}};document.querySelector('main').append(el)});
    await page.getByLabel('Mi Home region').selectOption('cn');
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    assert.equal(await page.getByRole('img',{name:'Mi Home sign-in QR code'}).count(),1);
    assert.equal((await page.evaluate(()=>calls)).filter(x=>x==='begin_sign_in').length,1);
  } finally {await browser.close();}
});

test('completed setup clears the QR and shows the connection status',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>name==='begin_sign_in'?{ok:true,data:{qr_png:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='}}:{ok:true,data:{status:'ok',setup_status:'connected'}}};document.querySelector('main').append(el)});
    await page.getByLabel('Mi Home region').selectOption('cn');
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    await page.getByText('Xiaomi connected',{exact:true}).waitFor({timeout:5000});
    assert.equal(await page.getByRole('img',{name:'Mi Home sign-in QR code'}).count(),0);
  } finally {await browser.close();}
});

test('a saved gateway is shown as awaiting verification',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{let started=false;const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>name==='begin_sign_in'?(started=true,{ok:true,data:{qr_png:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='}}):{ok:true,data:{status:'ok',setup_status:started?'gateway_saved':'unconfigured'}}};document.querySelector('main').append(el)});
    await page.getByLabel('Mi Home region').selectOption('cn');
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    await page.getByText('Gateway saved. Connection needs verification.',{exact:true}).waitFor({timeout:5000});
    assert.equal(await page.getByRole('img',{name:'Mi Home sign-in QR code'}).count(),0);
  } finally {await browser.close();}
});

test('reopening setup restores the saved gateway state',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async()=>({ok:true,data:{status:'ok',setup_status:'gateway_saved'}})};document.querySelector('main').append(el)});
    await page.getByText('Gateway saved. Connection needs verification.',{exact:true}).waitFor({timeout:3000});
    assert.equal(await page.getByRole('button',{name:'Sign in with Mi Home'}).count(),0);
  } finally {await browser.close();}
});

test('the selected Mi Home region is sent when sign-in starts',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{window.sent=null;const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async(name,params)=>{if(name==='begin_sign_in')window.sent=params;return name==='begin_sign_in'?{ok:true,data:{qr_png:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='}}:{ok:true,data:{status:'ok',setup_status:'unconfigured'}}}};document.querySelector('main').append(el)});
    await page.getByLabel('Mi Home region').selectOption('sg',{timeout:1500});
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    assert.deepEqual(await page.evaluate(()=>window.sent),{region:'sg'});
  } finally {await browser.close();}
});

test('setup asks for a region before starting sign-in',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
    await page.evaluate(()=>{window.begun=false;const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async name=>{if(name==='begin_sign_in')window.begun=true;return {ok:true,data:{status:'ok',setup_status:'unconfigured'}}}};document.querySelector('main').append(el)});
    await page.getByRole('button',{name:'Sign in with Mi Home'}).click();
    assert.equal(await page.getByText('Choose your Mi Home region first.',{exact:true}).count(),1);
    assert.equal(await page.evaluate(()=>window.begun),false);
  } finally {await browser.close();}
});

test('region and sign-in controls have usable spacing without overflowing',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    for(const width of [320,390,1024]) {
      const page=await browser.newPage({viewport:{width,height:700}});
      await page.setContent('<main style="padding:16px"></main>');
      await page.addScriptTag({content:await readFile(new URL('index.js',ui),'utf8')});
      await page.evaluate(()=>{const el=document.createElement('hc-xiaomi-setup');el.host={me:{can:()=>true},action:async()=>({ok:true,data:{status:'ok',setup_status:'unconfigured'}})};document.querySelector('main').append(el)});
      const label=await page.getByText('Mi Home region',{exact:true}).boundingBox();
      const select=await page.getByLabel('Mi Home region').boundingBox();
      const button=await page.getByRole('button',{name:'Sign in with Mi Home'}).boundingBox();
      assert.ok(select.y >= label.y+label.height+4,`region label needs space before the selector at ${width}px`);
      assert.ok(button.y >= select.y+select.height+8,`sign-in needs space below the selector at ${width}px`);
      assert.ok(select.height >= 40 && button.height >= 40,`controls need comfortable touch targets at ${width}px`);
      assert.ok(select.x >= 0 && select.x+select.width <= width && button.x+button.width <= width,`controls must fit at ${width}px`);
      await page.close();
    }
  } finally {await browser.close();}
});
