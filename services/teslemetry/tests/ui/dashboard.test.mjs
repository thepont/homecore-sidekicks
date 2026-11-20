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
