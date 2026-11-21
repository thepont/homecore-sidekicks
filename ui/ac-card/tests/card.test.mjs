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

beforeEach(() => preparePage(page));

async function preparePage(page) {
  await page.goto('about:blank');
  await page.setContent('<meta name="viewport" content="width=device-width,initial-scale=1">' +
    '<main style="max-width:420px;margin:auto"></main>');
  await page.addScriptTag({ type: 'module', content: await readFile(new URL('index.js', ui), 'utf8') });
  await page.evaluate(() => {
    window.mountAc = ({ writable = false, allowed = writable, temperatureUnit = '°F', mode = 'auto', setpointStep = 0.5, panelFixture = false, electrolux = false } = {}) => {
      const listeners = new Set();
      const units = new Set();
      const opened = [];
      const sent = [];
      let fail = false;
      let device = {
        device_id: 'ac_1', name: 'Office AC', name_override: 'Study AC',
        available: true, attributes: writable
          ? { temperature: 24.5, hvac_mode: mode, hvac_action: 'idle', setpoint: 22, heat_setpoint: 22.5,
              cool_setpoint: 25, min_setpoint_gap: 2, fan_mode: 'auto', vertical_vane: 'swing', horizontal_vane: 'c' }
          : { temperature: 24.5, hvac_mode: 'auto', hvac_action: 'idle', setpoint: 22,
              heating_setpoint: 22.5, cooling_setpoint: 25, fan_mode: 'quiet', vertical_vane: 'swing',
              compressor_frequency: 0, runtime: 617.1 }
      };
      const number = { writable, min: 16, max: 30, step: 0.5, unit: '°C' };
      const schema = { attributes: {
        temperature: { writable: false, unit: '°C' },
        hvac_mode: { writable, options: ['off', 'heat', 'auto', 'cool', 'dry', 'fan_only'] },
        setpoint: { ...number, step: setpointStep }, heat_setpoint: number, cool_setpoint: number,
        fan_mode: { writable, options: ['auto', 'quiet'] },
        vertical_vane: { writable, options: ['auto', 'swing'] },
        ...(writable ? { horizontal_vane: { writable, options: ['c', 'swing'] } } : {})
      } };
      if (electrolux) {
        device.attributes = { temperature: 22, setpoint: 21, hvac_mode: "heat", on: true, fan_mode: "auto", swing: false, vertical_vane: "off" };
        schema.attributes = {
          temperature: { writable: false, unit: "°C" },
          setpoint: { writable: true, min: 17, max: 30, step: 1, unit: "°C" },
          hvac_mode: { writable: true, options: ["off", "heat", "cool", "auto", "dry", "fan_only"] },
          fan_mode: { writable: true, options: ["auto", "low", "medium", "high", "turbo", "quiet"] },
          vertical_vane: { writable: true, options: ["off", "swing"] }
        };
      }
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
        ui: {
          openDevice(id) {
            opened.push(id);
            if (!panelFixture) return;
            const dialog = document.createElement('dialog');
            dialog.style.width = 'min(342px, 90vw)';
            dialog.setAttribute('aria-label', device.name_override);
            const close = document.createElement('button');
            close.textContent = 'Close';
            close.addEventListener('click', () => dialog.close());
            const panel = document.createElement('hc-ac-panel');
            panel.host = host;
            panel.deviceId = id;
            dialog.append(close, panel);
            dialog.addEventListener('close', () => dialog.remove());
            document.body.append(dialog);
            dialog.showModal();
          }
        }
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
}

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
      const centered = ['.name', '.reading', '.activity', '[data-target=heat] .target-value']
        .every(selector => Math.abs(center(box(selector)) - center(dial)) <= 1);
      const heat = '[data-target=heat]';
      const heatText = document.createRange();
      heatText.selectNodeContents(get(heat + ' output'));
      const target = heatText.getBoundingClientRect();
      return centered && dial.top <= box('.activity').top &&
        box('.activity').bottom <= box('.reading').top &&
        box('.reading').bottom <= dial.bottom &&
        box('.reading').bottom <= box('.room-label').top &&
        box('[data-target=cool]').bottom <= box('.name').top &&
        target.left > box(heat + ' button:first-child').right &&
        target.right < box(heat + ' button:last-child').left &&
        box('.modes').bottom <= box('.airflow').top &&
        document.documentElement.scrollWidth <= innerWidth;
    });
    assert(layout, width + 'px tile');
  }
});

test('the thermostat face shows the target above the live reading and keeps off controls locked', async () => {
  await mount({ writable: true, electrolux: true, temperatureUnit: '°C' });
  const target = page.locator('.dial-target');
  const dial = page.locator('.dial');
  await page.getByRole('slider', { name: 'Target temperature' }).waitFor();
  await target.getByText('21 °C', { exact: true }).waitFor();
  const targetBox = await target.boundingBox();
  const roomBox = await dial.getByText('22°C', { exact: true }).boundingBox();
  assert(targetBox.y + targetBox.height <= roomBox.y + 1);
  assert((await dial.locator('.range').getAttribute('d')).length > 0);
  assert.equal(await button('Raise target').evaluate(element => getComputedStyle(element).borderRadius), '50%');
  await page.evaluate(() => update({ attributes: { hvac_mode: 'off' } }));
  await dial.getByText('Off', { exact: true }).waitFor();
  await target.getByText('21 °C', { exact: true }).waitFor();
  assert(!(await button('Raise target').isVisible()));
  assert.equal(await page.getByRole('slider').count(), 0);
  assert.deepEqual(await page.evaluate(() => sent), []);
  await page.evaluate(() => update({ available: false }));
  assert(!(await target.isVisible()));
  assert.equal(await dial.locator('.range').getAttribute('d'), '');
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

async function pointOnDial(value, targetPage = page) {
  return targetPage.locator('.dial svg').evaluate((svg, value) => {
    const angle = (135 + 270 * (value - 16) / 14.5) * Math.PI / 180;
    const point = new DOMPoint(110 + 69 * Math.cos(angle), 88 + 69 * Math.sin(angle));
    const position = point.matrixTransform(svg.getScreenCTM());
    return { x: position.x, y: position.y };
  }, value);
}

async function beginDrag(from, to) {
  const start = await pointOnDial(from);
  const end = await pointOnDial(to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 5 });
}

test('drag previews a single temperature and sends one rounded setting on release', async () => {
  await mount({ writable: true, mode: 'cool', setpointStep: 1, temperatureUnit: '°C' });
  const target = page.getByRole('slider', { name: 'Target temperature' });
  await target.waitFor();
  const nearby = await pointOnDial(23.3);
  await page.mouse.click(nearby.x, nearby.y);
  assert.deepEqual(await page.evaluate(() => sent), []);
  await beginDrag(22, 23.7);
  await byText('24 °C').waitFor();
  assert.deepEqual(await page.evaluate(() => sent), []);
  await page.mouse.up();
  assert.deepEqual(await page.evaluate(() => sent), [['ac_1', { setpoint: 24 }]]);
  await byText('22 °C').waitFor();
  await page.evaluate(() => update({ attributes: { setpoint: 24 } }));
  await byText('24 °C').waitFor();
  assert.equal(await target.getAttribute('aria-valuenow'), '24');
});

test('Auto drag selects the nearest target and respects the other target', async () => {
  await mount({ writable: true, temperatureUnit: '°C' });
  const heat = page.getByRole('slider', { name: 'Heating target' });
  const cool = page.getByRole('slider', { name: 'Cooling target' });
  await heat.waitFor();
  assert.equal(await heat.getAttribute('aria-valuemax'), '23');
  assert.equal(await cool.getAttribute('aria-valuemin'), '24.5');
  await beginDrag(22.5, 27);
  await page.mouse.up();
  assert.deepEqual(await page.evaluate(() => sent), [['ac_1', { heat_setpoint: 23 }]]);
  await page.evaluate(() => update({ attributes: { heat_setpoint: 23 } }));
  await beginDrag(25, 27.8);
  await page.mouse.up();
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { cool_setpoint: 28 }]);
});

test('a cancelled drag, an offline unit and a read-only account send nothing', async () => {
  await mount({ writable: true, mode: 'heat', temperatureUnit: '°C' });
  const target = page.getByRole('slider', { name: 'Target temperature' });
  await target.waitFor();
  await beginDrag(22, 24);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await byText('22 °C').waitFor();
  await beginDrag(22, 24);
  await page.evaluate(() => update({ available: false }));
  await page.mouse.up();
  assert.deepEqual(await page.evaluate(() => sent), []);
  await page.evaluate(() => card.remove());
  await mount({ writable: true, allowed: false, mode: 'heat' });
  assert.equal(await target.count(), 0);
  await beginDrag(22, 24);
  await page.mouse.up();
  assert.deepEqual(await page.evaluate(() => sent), []);
});

test('dial accepts keyboard and touch input using the same temperature limits', async () => {
  await mount({ writable: true, mode: 'cool', setpointStep: 1 });
  const target = page.getByRole('slider', { name: 'Target temperature' });
  await target.press('ArrowRight');
  assert.deepEqual(await page.evaluate(() => sent), [['ac_1', { setpoint: 23 }]]);
  await page.evaluate(() => update({ attributes: { setpoint: 23 } }));
  const cdp = await page.context().newCDPSession(page);
  const start = await pointOnDial(23);
  const end = await pointOnDial(26);
  try {
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [start] });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [end] });
    assert.equal((await page.evaluate(() => sent)).length, 1);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { setpoint: 26 }]);
  } finally {
    await cdp.detach();
  }
});

test('a finger can start away from the dot on a narrow, scrollable mobile card', async () => {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true
  });
  try {
    const phone = await context.newPage();
    phone.setDefaultTimeout(2500);
    await preparePage(phone);
    await phone.evaluate(() => {
      document.body.style.minHeight = '200vh';
      document.querySelector('main').style.width = '174px';
      mountAc({ writable: true, mode: 'cool', setpointStep: 1, temperatureUnit: '°C' });
    });
    await phone.getByRole('slider', { name: 'Target temperature' }).waitFor();
    assert(await phone.evaluate(() => matchMedia('(any-pointer: coarse)').matches));
    const start = await pointOnDial(24.8, phone);
    const end = await pointOnDial(27.1, phone);
    const cdp = await context.newCDPSession(phone);
    const drag = async () => {
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [start] });
      for (const fraction of [0.25, 0.5, 0.75, 1]) {
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{
          x: start.x + (end.x - start.x) * fraction,
          y: start.y + (end.y - start.y) * fraction
        }] });
      }
    };
    await drag();
    await phone.getByText('27 °C', { exact: true }).waitFor();
    assert.deepEqual(await phone.evaluate(() => sent), []);
    assert.equal(await phone.evaluate(() => scrollY), 0);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
    await phone.getByText('22 °C', { exact: true }).waitFor();
    assert.deepEqual(await phone.evaluate(() => sent), []);

    await drag();
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    assert.deepEqual(await phone.evaluate(() => sent), [['ac_1', { setpoint: 27 }]]);
  } finally {
    await context.close();
  }
});

test('tapping the room reading opens the same controls with a larger dial', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.deepEqual(manifest.slots.find(slot => slot.slot === 'device.panel'),
    { slot: 'device.panel', element: 'hc-ac-panel' });
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true
  });
  try {
    const phone = await context.newPage();
    phone.setDefaultTimeout(2500);
    await preparePage(phone);
    await phone.evaluate(() => {
      document.querySelector('main').style.width = '174px';
      mountAc({ writable: true, mode: 'cool', temperatureUnit: '°C', panelFixture: true });
    });
    const card = phone.locator('hc-ac-card');
    await card.getByRole('slider', { name: 'Target temperature' }).waitFor();
    const tileDial = await card.locator('.dial').boundingBox();
    await card.getByRole('button', { name: 'Open controls for Study AC' }).getByText('25°C').tap();
    const dialog = phone.getByRole('dialog', { name: 'Study AC' });
    const panel = dialog.locator('hc-ac-panel');
    await panel.getByRole('slider', { name: 'Target temperature' }).waitFor();
    const expandedDial = await panel.locator('.dial').boundingBox();
    assert(expandedDial.width >= 224 && expandedDial.width >= 1.5 * tileDial.width);
    assert.equal(await panel.getByRole('button', { name: /Open controls/ }).count(), 0);
    assert.deepEqual(await phone.evaluate(() => opened), ['ac_1']);
    assert.deepEqual(await phone.evaluate(() => sent), []);
    await panel.getByRole('button', { name: 'Raise target', exact: true }).tap();
    assert.deepEqual(await phone.evaluate(() => sent), [['ac_1', { setpoint: 22.5 }]]);
    await dialog.getByRole('button', { name: 'Close' }).tap();
    await phone.locator('hc-ac-panel').waitFor({ state: 'detached' });
    assert.deepEqual(await phone.evaluate(() => subscriptions()), [1, 1]);
  } finally {
    await context.close();
  }
});

test('Electrolux controls use its reported setpoint, fan and swing schema', async () => {
  await mount({ writable: true, electrolux: true, temperatureUnit: '°C' });
  await page.getByRole('slider', { name: 'Target temperature' }).waitFor();
  await button('Raise target').click();
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { setpoint: 22 }]);
  await page.evaluate(() => update({ attributes: { setpoint: 22 } }));
  await selected('Fan speed').selectOption('quiet');
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { fan_mode: 'quiet' }]);
  await page.evaluate(() => update({ attributes: { fan_mode: 'quiet' } }));
  await selected('Vertical vane').selectOption('swing');
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { vertical_vane: 'swing' }]);
  await page.evaluate(() => update({ attributes: { vertical_vane: 'swing' } }));
  await button('Off').click();
  assert.deepEqual((await page.evaluate(() => sent)).at(-1), ['ac_1', { hvac_mode: 'off' }]);
  await page.evaluate(() => update({ attributes: { hvac_mode: 'off' } }));
  assert(await selected('Vertical vane').isDisabled());
});
