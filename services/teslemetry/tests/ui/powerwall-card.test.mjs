import assert from 'node:assert/strict';
import { after, before, beforeEach, test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';

const { chromium } = createRequire(import.meta.url)('playwright');
const ui = new URL('../../ui/', import.meta.url);
let browser;
let page;

before(async () => {
  browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_BIN });
  page = await browser.newPage({ viewport: { width: 390, height: 600 } });
});
after(async () => { await browser?.close(); });

beforeEach(async () => {
  await page.goto('about:blank');
  await page.setContent('<main style="width:320px"></main>');
  await page.addScriptTag({ content: await readFile(new URL('index.js', ui), 'utf8') });
  await page.evaluate(() => {
    window.makeHost = (name = 'Garage Powerwall') => {
      let device = {
        device_id: 'teslemetry_powerwall', plugin_id: 'plugin.teslemetry-powerwall',
        name, device_type: 'sensor', available: true,
        attributes: { charge_pct: 65.2, battery_power_w: -1190, solar_power_w: 2412,
          home_power_w: 882, grid_power_w: -340 }
      };
      const listeners = new Set();
      const unitListeners = new Set();
      const opened = [];
      const sheet = new CSSStyleSheet();
      return {
        api: 1, pluginId: device.plugin_id, appVersion: '0.1.84',
        theme: { sheet, dark: () => false },
        devices: {
          get: id => device?.device_id === id ? device : undefined,
          label: d => d.name_override || d.name,
          subscribe: listener => {
            listeners.add(listener);
            listener(device ? [device] : []);
            return () => listeners.delete(listener);
          }
        },
        units: {
          show: (value, unit) => ({ value, unit, text: `${value}${unit === '%' ? '' : ' '}${unit}` }),
          subscribe: listener => {
            unitListeners.add(listener);
            listener({ temperature: 'C', measures: 'metric' });
            return () => unitListeners.delete(listener);
          }
        },
        ui: { openDevice: id => opened.push(id) },
        update: next => {
          device = next === null ? undefined : { ...device, ...next };
          for (const listener of listeners) listener(device ? [device] : []);
        },
        stats: () => ({ subscriptions: listeners.size, units: unitListeners.size, opened })
      };
    };
    window.host = makeHost();
    window.card = document.createElement('hc-teslemetry-card');
    card.host = host;
    card.deviceId = 'teslemetry_powerwall';
    card.form = 'standard';
    document.querySelector('main').append(card);
  });
});

const text = () => page.getByRole('button').textContent();

test('registers the sensor card and shows current readings with their units', async () => {
  const manifest = JSON.parse(await readFile(new URL('manifest.json', ui), 'utf8'));
  assert.equal(manifest.host_api, 1);
  assert.equal(manifest.entry, 'index.js');
  assert.ok(manifest.slots.some(slot => slot.slot === 'device.card' &&
    slot.element === 'hc-teslemetry-card' && slot.device_types.includes('sensor')));
  const content = await text();
  for (const expected of ['Garage Powerwall', '65.2%', '1.19 kW', '2.41 kW', '0.88 kW', '0.34 kW', 'Charging', 'Exporting']) {
    assert.ok(content.includes(expected), `${expected} must be visible`);
  }
  assert.equal(await page.getByRole('button', { name: /Open.*details/ }).count(), 1);
});

test('updates live, preserves zero, and hides retained readings when unavailable', async () => {
  await page.evaluate(() => host.update({ attributes: { charge_pct: 0, battery_power_w: 0 } }));
  assert.match(await text(), /0%/);
  assert.match(await text(), /0 kW/);
  assert.doesNotMatch(await text(), /2412|882|-340/);
  await page.evaluate(() => host.update({ available: false }));
  assert.match(await text(), /Unavailable/);
  assert.doesNotMatch(await text(), /0%|0 kW/);
  await page.evaluate(() => host.update({ available: true, attributes: { charge_pct: 82.6, battery_power_w: 482 } }));
  assert.match(await text(), /82.6%/);
  assert.match(await text(), /0.48 kW/);
});

test('rejects malformed readings instead of displaying them as valid values', async () => {
  for (const charge of [-1, 101, null, '85', Number.NaN, Number.POSITIVE_INFINITY]) {
    await page.evaluate(charge => host.update({ attributes: {
      charge_pct: charge, battery_power_w: '700', solar_power_w: null,
      home_power_w: Number.POSITIVE_INFINITY, grid_power_w: Number.NaN
    } }), charge);
    const content = await text();
    assert.match(content, /Waiting for readings/);
    assert.doesNotMatch(content, /NaN|Infinity|700|85%|101%|-1%|null/);
  }
  await page.evaluate(() => host.update(null));
  assert.match(await text(), /Waiting for device/);
});

test('only accepts the Powerwall and follows device changes without old readings', async () => {
  const accepts = await page.evaluate(() => {
    const Card = customElements.get('hc-teslemetry-card');
    return [Card.accepts({ device_id: 'teslemetry_powerwall' }),
      Card.accepts({ device_id: 'teslemetry_vehicle_example' }), Card.accepts(null)];
  });
  assert.deepEqual(accepts, [true, false, false]);
  await page.evaluate(() => { card.deviceId = 'missing'; });
  assert.match(await text(), /Waiting for device/);
  assert.doesNotMatch(await text(), /65.2%/);
  await page.evaluate(() => { card.deviceId = 'teslemetry_powerwall'; });
  assert.match(await text(), /65.2%/);
});

test('disconnect and host replacement release subscriptions and reconnect once', async () => {
  assert.deepEqual(await page.evaluate(() => host.stats()), { subscriptions: 1, units: 1, opened: [] });
  await page.evaluate(() => { card.host = host; });
  assert.equal(await page.evaluate(() => host.stats().subscriptions), 1);
  await page.evaluate(() => { card.remove(); });
  assert.deepEqual(await page.evaluate(() => host.stats()), { subscriptions: 0, units: 0, opened: [] });
  await page.evaluate(() => { host.update({ name: 'After reconnect' }); document.querySelector('main').append(card); });
  assert.match(await text(), /After reconnect/);
  assert.equal(await page.evaluate(() => host.stats().subscriptions), 1);
  await page.evaluate(() => { window.nextHost = makeHost('Replacement'); card.host = nextHost; });
  assert.deepEqual(await page.evaluate(() => host.stats()), { subscriptions: 0, units: 0, opened: [] });
  assert.match(await text(), /Replacement/);
  await page.evaluate(() => { card.remove(); });
  assert.deepEqual(await page.evaluate(() => nextHost.stats()), { subscriptions: 0, units: 0, opened: [] });
});

test('opens details with pointer, Enter and Space without device commands', async () => {
  const button = page.getByRole('button', { name: /Open.*details/ });
  await button.click();
  await button.focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Space');
  assert.deepEqual(await page.evaluate(() => host.stats().opened), Array(3).fill('teslemetry_powerwall'));
});

test('exposes current readings to a screen reader when the card is focused', async () => {
  const button = await page.getByRole('button').elementHandle();
  const accessible = await page.accessibility.snapshot({ root: button });
  assert.match(accessible.description, /65.2%/);
  assert.match(accessible.description, /1.19 kW/);
  assert.match(accessible.description, /2.41 kW/);
  await page.evaluate(() => host.update({ available: false }));
  const unavailable = await page.accessibility.snapshot({ root: button });
  assert.match(unavailable.description, /Unavailable/);
  assert.doesNotMatch(unavailable.description, /65.2%/);
});

test('shows net direction for each connection and treats missing readings as unknown', async () => {
  assert.match(await text(), /Charging/);
  assert.match(await text(), /Exporting/);
  assert.match(await text(), /Generating/);
  await page.evaluate(() => host.update({ attributes: {
    charge_pct: 73, battery_power_w: 800, grid_power_w: 650, solar_power_w: -20, home_power_w: 1430
  } }));
  for (const direction of ['Discharging', 'Importing', 'Consuming']) assert.ok((await text()).includes(direction));
  await page.evaluate(() => host.update({ attributes: { charge_pct: 73, battery_power_w: 0 } }));
  assert.match(await text(), /Idle/);
  assert.match(await text(), /No reading/);
  assert.doesNotMatch(await text(), /Importing|Exporting|Generating|Discharging|Charging/);
});

test('flow motion stops for unavailable, zero and reduced-motion states', async () => {
  const moving = () => page.evaluate(() => [...card.shadowRoot.querySelectorAll('.flow')]
    .filter(path => !path.hidden && getComputedStyle(path).display !== 'none' && getComputedStyle(path).animationName !== 'none').length);
  assert.ok(await moving() > 0);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  assert.equal(await moving(), 0);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.evaluate(() => host.update({ available: false }));
  assert.equal(await moving(), 0);
  await page.evaluate(() => host.update({ available: true, attributes: {
    charge_pct: 73, battery_power_w: 0, solar_power_w: 0, home_power_w: 0, grid_power_w: 0
  } }));
  assert.equal(await moving(), 0);
});

test('renders labels as text and remains within phone width in every card form', async () => {
  await page.evaluate(() => host.update({ name: '<img src=x onerror="window.injected=true">' }));
  assert.match(await text(), /<img src=x/);
  assert.equal(await page.evaluate(() => card.shadowRoot.querySelector('img')), null);
  for (const form of ['compact', 'standard', 'large']) {
    await page.evaluate(form => { card.form = form; }, form);
    assert.equal(await page.evaluate(() => {
      const button = card.shadowRoot.querySelector('button');
      return button.scrollWidth <= button.clientWidth;
    }), true, `${form} must not overflow`);
  }
});
