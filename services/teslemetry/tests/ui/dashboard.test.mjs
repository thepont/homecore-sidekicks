import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)('playwright');
const ui = new URL('../../ui/', import.meta.url);
let browser, page;
before(async () => {
  browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_BIN });
  page = await browser.newPage({ viewport: { width: 390, height: 700 } });
  page.setDefaultTimeout(1200);
  await page.setContent('<main style="width:320px"></main>');
  await page.addScriptTag({ content: await readFile(new URL('index.js', ui), 'utf8') });
  await page.evaluate(() => {
    window.device = { device_id: 'teslemetry_powerwall', available: true, attributes: {
      solar_power_w: 2400, total_solar_generation_wh: 12500, charge_pct: 70,
      grid_power_w: 1000, grid_energy_imported_wh: 3000,
      total_grid_energy_exported_wh: 2000, home_power_w: 500,
      tariff_available: true, tariff_buy_price_per_kwh: 0.3,
      tariff_sell_price_per_kwh: 0.08, tariff_currency: 'AUD'
    } };
    window.host = { devices: {
      get: id => id === device.device_id ? device : undefined,
      list: () => [device], subscribe: fn => { fn([device]); return () => {}; },
      history: async () => ({ok:true,data: []})
    }, ui: { openDevice: () => {} } };
  });
});
after(async () => { await browser?.close(); });
test('solar dashboard card is offered and shows live production', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.ok(manifest.slots.some(s => s.slot === 'dashboard.card' && s.element === 'hc-teslemetry-solar'));
  await page.evaluate(() => { const card = document.createElement('hc-teslemetry-solar'); card.host = host; document.querySelector('main').append(card); });
  assert.equal(await page.getByText('2.4 kW', { exact: true }).count(), 1);
});

test('solar card opens a history graph and returns to live reading', async () => {
  await page.evaluate(() => {
    host.devices.history = async () => ({ok: true, data: [
      {at: '2026-11-20T01:00:00Z', value: 1000},
      {at: '2026-11-20T02:00:00Z', value: 2400}
    ]});
  });
  await page.getByRole('button', {name: 'Solar details'}).click();
  assert.equal(await page.getByRole('img', {name: /solar power history/i}).count(), 1);
  await page.getByRole('button', {name: 'Back to solar'}).click();
  assert.equal(await page.getByText('2.4 kW', {exact:true}).count(), 1);
});

test('grid card shows estimated import cost when a tariff exists and works without one', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.ok(manifest.slots.some(s => s.slot === 'dashboard.card' && s.element === 'hc-teslemetry-grid'));
  await page.evaluate(() => { const c = document.createElement('hc-teslemetry-grid'); c.host = host; document.querySelector('main').replaceChildren(c); });
  await page.getByRole('button', {name:'Grid details'}).click();
  assert.equal(await page.getByText('Estimated import cost: AUD 0.90', {exact:true}).count(), 1);
  await page.evaluate(() => { device.attributes.tariff_available = false; document.querySelector('hc-teslemetry-grid').host = host; });
  assert.equal(await page.getByText('Import cost unavailable', {exact:true}).count(), 1);
  assert.equal(await page.getByText('1 kW', {exact:true}).count(), 1);
});

test('battery card shows charge without needing graph or tariff data', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.ok(manifest.slots.some(s => s.slot === 'dashboard.card' && s.element === 'hc-teslemetry-battery'));
  await page.evaluate(() => { const c = document.createElement('hc-teslemetry-battery'); c.host = host; document.querySelector('main').replaceChildren(c); });
  assert.equal(await page.getByText('70%', {exact:true}).count(), 1);
});
