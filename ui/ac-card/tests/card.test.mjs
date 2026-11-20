import assert from 'node:assert/strict';
import { after, before, beforeEach, test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

const { chromium } = createRequire(import.meta.url)('playwright');
const ui = new URL('../', import.meta.url);
let browser;
let page;

before(async () => {
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM_BIN });
  page = await browser.newPage({ viewport: { width: 320, height: 850 } });
  page.setDefaultTimeout(3000);
});
after(async () => { await browser?.close(); });

beforeEach(async () => {
  await page.goto('about:blank');
  await page.setContent('<main style="max-width:420px;margin:auto"></main>');
  await page.addScriptTag({ type: 'module', content: await readFile(new URL('index.js', ui), 'utf8') });
  await page.evaluate(() => {
    window.mountAc = ({ writable = false, allowed = writable, temperatureUnit = '°F' } = {}) => {
      const listeners = new Set();
      const units = new Set();
      const opened = [];
      const sent = [];
      let fail = false;
      let device = {
        device_id: 'ac_1', name: 'Office AC', name_override: 'Study AC',
        available: true, attributes: writable
          ? { temperature: 24.5, hvac_mode: 'auto', hvac_action: 'idle', heat_setpoint: 22.5,
              cool_setpoint: 25, min_setpoint_gap: 2, fan_mode: 'auto', vertical_vane: 'swing', horizontal_vane: 'c' }
          : { temperature: 24.5, hvac_mode: 'auto', hvac_action: 'idle', setpoint: 22,
              heating_setpoint: 22.5, cooling_setpoint: 25, fan_mode: 'quiet', vertical_vane: 'swing',
              compressor_frequency: 0, runtime: 617.1 }
      };
      const number = { writable, min: 16, max: 30, step: 0.5, unit: '°C' };
      const schema = { attributes: {
        temperature: { writable: false, unit: '°C' },
        hvac_mode: { writable, options: ['off', 'heat', 'auto', 'cool', 'dry', 'fan_only'] },
        setpoint: number, heat_setpoint: number, cool_setpoint: number,
        fan_mode: { writable, options: ['auto', 'quiet'] },
        vertical_vane: { writable, options: ['auto', 'swing'] },
        ...(writable ? { horizontal_vane: { writable, options: ['c', 'swing'] } } : {})
      } };
      const host = {
        devices: {
          get: id => id === device.device_id ? device : undefined,
          label: d => d.name_override || d.name,
          subscribe: listener => {
            listeners.add(listener);
            listener([device]);
            return () => listeners.delete(listener);
          }
        },
        theme: { sheet: new CSSStyleSheet() },
        units: {
          show: (value, unit) => {
            if (unit !== '°C' || temperatureUnit === '°C') return { value, unit, text: value + ' ' + unit };
            const text = (value * 9 / 5 + 32).toFixed(1);
            return { value: Number(text), unit: '°F', text: text + ' °F' };
          },
          subscribe: listener => {
            units.add(listener);
            listener();
            return () => units.delete(listener);
          }
        },
        me: { can: scope => allowed && scope === 'devices:command' },
        async fetch(path) {
          if (path !== '/devices/ac_1/schema') throw new Error('Unexpected path');
          return new Response(JSON.stringify(schema));
        },
        async command(id, body) {
          sent.push([id, body]);
          return fail ? { ok: false } : { ok: true };
        },
        ui: { openDevice: id => opened.push(id) }
      };
      window.update = next => {
        device = { ...device, ...next, attributes: { ...device.attributes, ...next.attributes } };
        listeners.forEach(listener => listener([device]));
      };
      const card = document.createElement('hc-ac-card');
      card.host = host;
      card.deviceId = device.device_id;
      card.form = 'standard';
      document.querySelector('main').append(card);
      Object.assign(window, { card, opened, sent, failCommand: () => { fail = true; },
        subscriptions: () => [listeners.size, units.size] });
    };
  });
});

const mount = config => page.evaluate(config => mountAc(config), config);
const byText = text => page.getByText(text, { exact: true });
const button = name => page.getByRole('button', { name, exact: true });
const selected = name => page.getByRole('combobox', { name });

test('the reusable slot displays live readings, optional airflow, and diagnostics on a read-only AC', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.equal(manifest.entry, 'index.js');
  assert.deepEqual(manifest.slots[0], { slot: 'device.card', element: 'hc-ac-card', forms: ['standard', 'large'] });
  await mount({});
  assert.equal(await page.locator('hc-ac-card .reading').textContent(), '76°F');
  await byText('72.5 °F').waitFor();
  await byText('77.0 °F').waitFor();
  await byText('Quiet').waitFor();
  await byText('Swing').waitFor();
  assert.equal(await selected('Fan speed').count(), 0);
  assert(!(await byText('Horizontal').isVisible()));
  await byText('Diagnostics').click();
  await byText('0 Hz').waitFor();
  await button('Study AC').click();
  assert.deepEqual(await page.evaluate(() => opened), ['ac_1']);
  await page.evaluate(() => update({ attributes: { temperature: 25, hvac_mode: 'heat',
    hvac_action: null, sub_mode: 'defrost' } }));
  await byText('Defrosting').waitFor();
  await page.evaluate(() => update({ attributes: { horizontal_vane: 'c', has_horizontal_vane: false } }));
  assert(!(await byText('Horizontal').isVisible()));
  await byText('71.6 °F').waitFor();
  await page.evaluate(() => update({ available: false }));
  await byText('Unavailable').waitFor();
  assert.equal(await byText('77.0 °F').count(), 0);
  await page.evaluate(() => card.remove());
  assert.deepEqual(await page.evaluate(() => subscriptions()), [0, 0]);
});

test('a writable AC uses declared controls, awaits a report, and enforces the auto range', async () => {
  await mount({ writable: true });
  const raise = button('Raise heating target');
  await raise.click();
  assert.deepEqual(await page.evaluate(() => sent), [['ac_1', { heat_setpoint: 23 }]]);
  await byText('Waiting for the unit…').waitFor();
  await byText('72.5 °F').waitFor();
  await page.evaluate(() => update({ attributes: { heat_setpoint: 23 } }));
  await byText('73.4 °F').waitFor();
  assert(await raise.isDisabled());
  assert(await button('Lower cooling target').isDisabled());
  assert.equal(await page.getByRole('button', { name: 'Auto', exact: true, pressed: true }).count(), 1);
  await selected('Horizontal vane').selectOption('swing');
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { horizontal_vane: 'swing' }]);
  await page.evaluate(() => update({ attributes: { horizontal_vane: 'swing' } }));
  await button('Dry').click();
  await page.evaluate(() => update({ attributes: { hvac_mode: 'dry' } }));
  assert(!(await byText('Heat to').isVisible()));
  assert(await selected('Fan speed').isEnabled());
  await button('Off').click();
  await page.evaluate(() => update({ attributes: { hvac_mode: 'off' } }));
  assert(await selected('Vertical vane').isDisabled());
  assert(!(await button('Cool').isDisabled()));
});

test('permission gates all controls and failed commands preserve reported values', async () => {
  await mount({ writable: true, allowed: false });
  await byText('72.5 °F').waitFor();
  assert.equal(await button('Raise heating target').count(), 0);
  assert.equal(await selected('Fan speed').count(), 0);
  await page.evaluate(() => card.remove());
  await mount({ writable: true });
  await page.evaluate(() => failCommand());
  await button('Lower heating target').click();
  await byText('Could not send change.').waitFor();
  await byText('72.5 °F').waitFor();
  assert(!(await button('Lower heating target').isDisabled()));
});

test('targets and airflow fit a phone card and keep their labels aligned', async () => {
  await mount({ writable: true });
  await selected('Horizontal vane').waitFor();
  for (const width of [174, 304]) {
    await page.locator('main').evaluate((main, width) => { main.style.width = width + 'px'; }, width);
    const layout = await page.evaluate(() => {
      const root = card.shadowRoot;
      const get = selector => root.querySelector(selector);
      const box = selector => get(selector).getBoundingClientRect();
      const center = rect => rect.left + rect.width / 2;
      const dial = box('.dial');
      const centered = ['.name', '.room-label', '.reading', '.activity', '[data-target=heat] .target-value']
        .every(selector => Math.abs(center(box(selector)) - center(dial)) <= 1);
      return centered && dial.bottom <= box('.room-label').top &&
        box('.room-label').bottom <= box('.reading').top && box('.reading').bottom <= box('.activity').top &&
        parseFloat(getComputedStyle(get('.reading')).fontSize) <= 24 &&
        box('.modes').bottom <= box('.airflow').top &&
        document.documentElement.scrollWidth <= innerWidth;
    });
    assert(layout, width + 'px tile');
  }
});

test('rounds the current temperature in Celsius and keeps the Auto targets precise', async () => {
  await mount({ temperatureUnit: '°C' });
  assert.equal(await page.locator('hc-ac-card .reading').textContent(), '25°C');
  await byText('22.5 °C').waitFor();
});

test('an accepted command without a matching report expires and leaves the old target visible', async () => {
  await page.clock.install();
  await mount({ writable: true });
  const lower = button('Lower heating target');
  await lower.click();
  await byText('Waiting for the unit…').waitFor();
  await page.clock.fastForward(15_001);
  await byText('Change not confirmed.').waitFor();
  await byText('72.5 °F').waitFor();
  assert(await lower.isEnabled());
  await page.clock.resume();
});
