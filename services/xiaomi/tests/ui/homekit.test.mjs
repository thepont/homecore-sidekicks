import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)('playwright');
const source=new URL('../../../homekit/ui/index.js',import.meta.url);

test('an administrator can discover and pair an available accessory',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage({viewport:{width:390,height:700}});
    await page.setContent('<main></main>');
    await page.addScriptTag({content:await readFile(source,'utf8')});
    await page.evaluate(()=>{
      window.calls=[];
      const el=document.createElement('hc-homekit-setup');
      el.host={me:{can:()=>true},action:async(name,params)=>{
        calls.push([name,params]);
        if(name==='discover_accessories')return {ok:true,data:{accessories:[{id:'02:11:22:33:44:55',name:'Test hub',paired:false}]}};
        return {ok:true,data:{status:'ok'}};
      }};
      document.querySelector('main').append(el);
    });
    await page.getByRole('button',{name:'Find accessories'}).click();
    await page.getByLabel('Accessory').selectOption('02:11:22:33:44:55');
    await page.getByLabel('HomeKit setup code').fill('123-45-678');
    await page.getByRole('button',{name:'Connect'}).click();
    assert.deepEqual((await page.evaluate(()=>calls)).at(-1),['pair_accessory',{id:'02:11:22:33:44:55',pin:'123-45-678'}]);
    assert.equal(await page.getByText('Accessory connected.',{exact:true}).count(),1);
    assert.equal(await page.getByLabel('HomeKit setup code').inputValue(),'');
  } finally {await browser.close();}
});

test('an already paired accessory is identified without offering to pair it',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');
    await page.addScriptTag({content:await readFile(source,'utf8')});
    await page.evaluate(()=>{const el=document.createElement('hc-homekit-setup');el.host={me:{can:()=>true},action:async()=>({ok:true,data:{accessories:[{id:'02:11:22:33:44:55',name:'Test hub',paired:true}]}})};document.querySelector('main').append(el)});
    await page.getByRole('button',{name:'Find accessories'}).click();
    assert.equal(await page.getByText('Test hub is already paired.',{exact:true}).count(),1);
    assert.equal(await page.getByRole('button',{name:'Connect'}).isDisabled(),true);
  } finally {await browser.close();}
});

test('a viewer cannot start setup',async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_BIN});
  try {
    const page=await browser.newPage();await page.setContent('<main></main>');
    await page.addScriptTag({content:await readFile(source,'utf8')});
    await page.evaluate(()=>{const el=document.createElement('hc-homekit-setup');el.host={me:{can:()=>false},action:async()=>{throw Error('unexpected')}};document.querySelector('main').append(el)});
    assert.equal(await page.getByRole('button',{name:'Find accessories'}).isDisabled(),true);
  } finally {await browser.close();}
});
