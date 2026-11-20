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
  page.setDefaultTimeout(3000);
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

test('shows source buy and sell rates with currency and their independent periods', async () => {
  await page.evaluate(() => host.update({ attributes: {
    charge_pct: 65.2, battery_power_w: -1190,
    tariff_available: true, tariff_buy_price_per_kwh: 0.4906, tariff_sell_price_per_kwh: 0.07125,
    tariff_buy_period: 'PEAK', tariff_sell_period: 'ALL_DAY', tariff_currency: 'AUD',
    tariff_updated_at: '2025-06-12T04:30:00Z'
  } }));
  const tariff = page.locator('.tariffs');
  assert.equal(await tariff.isVisible(), true);
  const content = await tariff.textContent();
  for (const expected of ['Buy', 'Sell', '0.4906 AUD/kWh', '0.07125 AUD/kWh', 'PEAK', 'ALL DAY']) {
    assert.ok(content.includes(expected), `${expected} must be shown`);
  }
  assert.doesNotMatch(content, /2025-06-12|just updated|fresh/i);
  const accessible = await page.accessibility.snapshot({ root: await page.getByRole('button').elementHandle() });
  assert.match(accessible.description, /0.4906 AUD\/kWh/);
  assert.match(accessible.description, /0.07125 AUD\/kWh/);
});

test('labels inferred currency alongside live rates and clears the note when the source changes', async () => {
  await page.evaluate(() => host.update({ attributes: {
    tariff_available: true, tariff_buy_price_per_kwh: 0, tariff_sell_price_per_kwh: 0.005,
    tariff_currency: 'AUD', tariff_currency_source: 'site_timezone', tariff_currency_region: 'AU'
  } }));
  const note = page.getByText('Currency inferred from site timezone', { exact: true });
  assert.equal(await note.isVisible(), true);
  assert.equal(await page.locator('.tariff-buy .rate').textContent(), '0 AUD/kWh');
  assert.equal(await page.locator('.tariff-sell .rate').textContent(), '0.005 AUD/kWh');
  await page.getByRole('button').focus();
  const accessible = await page.accessibility.snapshot({ root: await page.getByRole('button').elementHandle() });
  assert.match(accessible.description, /Currency inferred from site timezone/);

  await page.evaluate(() => host.update({ available: false }));
  assert.equal(await note.isVisible(), false);
  assert.equal(await page.locator('.tariff-buy .rate').textContent(), '—');

  for (const source of ['tariff', undefined]) {
    await page.evaluate(source => host.update({ available: true, attributes: {
      tariff_available: true, tariff_buy_price_per_kwh: 0.25,
      tariff_currency: 'AUD', tariff_currency_source: source
    } }), source);
    assert.equal(await note.isVisible(), false);
    assert.equal(await page.locator('.tariff-buy .rate').textContent(), '0.25 AUD/kWh');
    assert.equal(await page.getByText('Sell rate unavailable', { exact: true }).isVisible(), true);
  }
});

test('preserves zero and negative rates without inventing missing currency or sell rates', async () => {
  await page.evaluate(() => host.update({ attributes: {
    tariff_available: true, tariff_buy_price_per_kwh: 0, tariff_sell_price_per_kwh: -0.0254,
    tariff_currency: 'unspecified'
  } }));
  const tariff = page.locator('.tariffs');
  assert.match(await tariff.textContent(), /0 \/kWh/);
  assert.match(await tariff.textContent(), /-0.0254 \/kWh/);
  assert.match(await tariff.textContent(), /Currency not supplied/);
  assert.doesNotMatch(await tariff.textContent(), /\$|AUD|USD|c\/kWh/);
  await page.evaluate(() => host.update({ attributes: {
    tariff_available: true, tariff_buy_price_per_kwh: -0.12, tariff_currency: 'AUD'
  } }));
  assert.match(await tariff.textContent(), /-0.12 AUD\/kWh/);
  assert.equal(await page.locator('.tariff-sell .rate').textContent(), '—');
  assert.match(await tariff.textContent(), /Sell rate unavailable/);
});

test('requires explicit tariff and device availability and never shows malformed rates', async () => {
  for (const availability of [undefined, false, 'true', 1]) {
    await page.evaluate(availability => host.update({ attributes: {
      tariff_available: availability, tariff_buy_price_per_kwh: 0.3789,
      tariff_sell_price_per_kwh: 0.0612, tariff_currency: 'AUD'
    } }), availability);
    assert.match(await page.locator('.tariffs').textContent(), /Tariff unavailable/);
    assert.doesNotMatch(await page.locator('.tariffs').textContent(), /0.3789|0.0612/);
  }
  for (const price of ['0.3789', null, Number.NaN, Number.POSITIVE_INFINITY]) {
    await page.evaluate(price => host.update({ attributes: {
      tariff_available: true, tariff_buy_price_per_kwh: price, tariff_currency: 'AUD'
    } }), price);
    assert.equal(await page.locator('.tariff-buy .rate').textContent(), '—');
  }
  await page.evaluate(() => host.update({ available: false, attributes: {
    tariff_available: true, tariff_buy_price_per_kwh: 0.3789, tariff_currency: 'AUD'
  } }));
  assert.match(await page.locator('.tariffs').textContent(), /Tariff unavailable/);
  assert.doesNotMatch(await page.locator('.tariffs').textContent(), /0.3789/);
  await page.evaluate(() => host.update({ available: true }));
  assert.match(await page.locator('.tariffs').textContent(), /0.3789 AUD\/kWh/);
});

test('shows bounded charger details without changing the reported Home total', async () => {
  assert.equal(await page.locator('.chargers').isVisible(), false);
  await page.evaluate(() => host.update({ attributes: {
    home_power_w: 4102, wall_connector_1_power_w: 3120, wall_connector_1_state: 2,
    wall_connector_2_power_w: 0, wall_connector_16_fault_state: 43,
    wall_connector_17_power_w: 1234, wall_connector_99999_power_w: 5678
  } }));
  const chargers = page.locator('.chargers');
  assert.equal(await chargers.isVisible(), true);
  assert.match(await page.locator('[data-charger="1"]').textContent(), /Charger 1.*3.12 kW.*Drawing/);
  assert.match(await page.locator('[data-charger="2"]').textContent(), /Charger 2.*0 kW.*Idle/);
  assert.match(await page.locator('[data-charger="16"]').textContent(), /Charger 16.*No reading/);
  assert.equal(await page.locator('[data-charger]').count(), 3);
  assert.equal(await page.locator('[data-node="home"] .value').textContent(), '4.1 kW');
  assert.doesNotMatch(await chargers.textContent(), /43|Connected|Unplugged|Charger 17|Charger 99999/);
  const accessible = await page.accessibility.snapshot({ root: await page.getByRole('button').elementHandle() });
  assert.match(accessible.description, /Charger 1.*3.12 kW.*Drawing/);
});

test('keeps unknown, reverse and unavailable charger readings distinct and removes withdrawn chargers', async () => {
  await page.evaluate(() => host.update({ attributes: {
    wall_connector_1_state: 2, wall_connector_2_power_w: -1200,
    wall_connector_3_power_w: '3120', wall_connector_4_power_w: Number.NaN
  } }));
  assert.match(await page.locator('[data-charger="1"]').textContent(), /No reading/);
  assert.match(await page.locator('[data-charger="2"]').textContent(), /-1.2 kW.*Reverse flow/);
  assert.match(await page.locator('[data-charger="3"]').textContent(), /No reading/);
  assert.match(await page.locator('[data-charger="4"]').textContent(), /No reading/);
  await page.evaluate(() => host.update({ available: false }));
  assert.match(await page.locator('.chargers').textContent(), /Unavailable/);
  assert.doesNotMatch(await page.locator('.chargers').textContent(), /1.2|3.12|Reverse flow|Drawing/);
  await page.evaluate(() => host.update({ available: true, attributes: {} }));
  assert.equal(await page.locator('.chargers').isVisible(), false);
  assert.equal(await page.locator('[data-charger]').count(), 0);
});

test('rates and charger details stay visible at phone width and render source labels as text', async () => {
  await page.evaluate(() => host.update({ attributes: {
    wall_connector_1_power_w: 7200, tariff_available: true,
    tariff_buy_price_per_kwh: 0.4906, tariff_sell_price_per_kwh: 0.07003,
    tariff_currency: 'AUD', tariff_buy_period: '<img src=x onerror="window.injected=true">'
  } }));
  assert.match(await page.locator('.tariffs').textContent(), /<img src=x/);
  assert.equal(await page.locator('hc-teslemetry-card img').count(), 0);
  for (const form of ['compact', 'standard', 'large']) {
    await page.evaluate(form => { card.form = form; }, form);
    assert.equal(await page.locator('.tariffs').isVisible(), true);
    assert.equal(await page.locator('.chargers').isVisible(), true);
    assert.equal(await page.evaluate(() => {
      const button = card.shadowRoot.querySelector('button');
      return button.scrollWidth <= button.clientWidth;
    }), true, `${form} tariff and charger footer must fit`);
  }
});

test('narrow dashboard columns replace the diagram with legible readings and a wider card restores it', async () => {
  await page.evaluate(() => host.update({ attributes: {
    charge_pct: 65.2, battery_power_w: -1190, solar_power_w: 5530,
    home_power_w: 4000, grid_power_w: -340, wall_connector_1_power_w: 3120,
    tariff_available: true, tariff_buy_price_per_kwh: 0.4906,
    tariff_sell_price_per_kwh: 0.05, tariff_currency: 'AUD', tariff_buy_period: 'PEAK'
  } }));
  for (const width of [190, 240]) {
    await page.evaluate(width => { document.querySelector('main').style.width = `${width}px`; }, width);
    assert.equal(await page.locator('.diagram').isVisible(), false);
    assert.equal(await page.locator('.compact-reading').isVisible(), true);
    assert.equal(await page.locator('.mini-readings').isVisible(), true);
    assert.match(await page.locator('.mini-readings').textContent(), /5.53 kW/);
    assert.match(await page.locator('.mini-readings').textContent(), /4 kW/);
    assert.match(await page.locator('.mini-readings').textContent(), /Exporting/);
    const measurements = await page.evaluate(() => {
      const root = card.shadowRoot;
      const button = root.querySelector('button');
      return { fits: button.scrollWidth <= button.clientWidth,
        reading: parseFloat(getComputedStyle(root.querySelector('.mini-reading')).fontSize),
        rate: parseFloat(getComputedStyle(root.querySelector('.rate')).fontSize) };
    });
    assert.equal(measurements.fits, true, `${width}px column must fit`);
    assert.ok(measurements.reading >= 12, 'narrow values must keep readable text size');
    assert.ok(measurements.rate >= 12, 'narrow rates must keep readable text size');
  }
  await page.evaluate(() => { document.querySelector('main').style.width = '360px'; card.form = 'large'; });
  assert.equal(await page.locator('.diagram').isVisible(), true);
  assert.equal(await page.locator('.mini-readings').isVisible(), false);
});

for (const withCharger of [false, true]) {
  for (const withTariff of [false, true]) {
    test(`optional data: ${withCharger ? 'with' : 'without'} charger, ${withTariff ? 'with' : 'without'} tariff`, async () => {
      await page.evaluate(({ withCharger, withTariff }) => {
        host.update({ attributes: {
          charge_pct: 65.2, battery_power_w: -1190, solar_power_w: 5530,
          home_power_w: 4000, grid_power_w: -340,
          ...(withCharger ? { wall_connector_1_power_w: 3120 } : {}),
          ...(withTariff ? { tariff_available: true, tariff_buy_price_per_kwh: 0.4906,
            tariff_sell_price_per_kwh: 0.05, tariff_currency: 'AUD',
            tariff_buy_period: 'PEAK', tariff_sell_period: 'ALL_DAY' } : {})
        } });
      }, { withCharger, withTariff });
      assert.equal(await page.locator('[data-node="home"] .value').textContent(), '4 kW');
      assert.equal(await page.locator('[data-node="solar"] .value').textContent(), '5.53 kW');
      assert.equal(await page.locator('.charge').textContent(), '65.2%');
      assert.equal(await page.locator('.status').textContent(), 'Live readings');
      assert.equal(await page.locator('.chargers').isVisible(), withCharger);
      assert.equal(await page.locator('[data-charger]').count(), withCharger ? 1 : 0);
      if (withCharger) assert.match(await page.locator('.chargers').textContent(), /3.12 kW.*Drawing/);
      assert.equal(await page.locator('.tariffs').isVisible(), true);
      assert.equal(await page.locator('.tariff-buy .rate').textContent(), withTariff ? '0.4906 AUD/kWh' : '—');
      assert.equal(await page.locator('.tariff-sell .rate').textContent(), withTariff ? '0.05 AUD/kWh' : '—');
      assert.equal(await page.locator('.tariff-note').isVisible(), !withTariff);
      if (!withTariff) assert.match(await page.locator('.tariff-note').textContent(), /Tariff unavailable/);
    });
  }
}

test('withdrawing both optional sections clears their previous values while live power continues', async () => {
  await page.evaluate(() => {
    const power = { charge_pct: 65.2, battery_power_w: -1190, solar_power_w: 5530,
      home_power_w: 4000, grid_power_w: -340 };
    host.update({ attributes: { ...power, wall_connector_1_power_w: 3120,
      tariff_available: true, tariff_buy_price_per_kwh: 0.4906,
      tariff_sell_price_per_kwh: 0.05, tariff_currency: 'AUD' } });
  });
  assert.match(await page.locator('.chargers').textContent(), /3.12 kW/);
  assert.match(await page.locator('.tariffs').textContent(), /0.4906 AUD\/kWh/);
  await page.evaluate(() => host.update({ attributes: {
    charge_pct: 65.3, battery_power_w: -1200, solar_power_w: 6000,
    home_power_w: 4200, grid_power_w: -600
  } }));
  assert.equal(await page.locator('.chargers').isVisible(), false);
  assert.equal(await page.locator('[data-charger]').count(), 0);
  assert.equal(await page.locator('.tariff-buy .rate').textContent(), '—');
  assert.equal(await page.locator('.tariff-sell .rate').textContent(), '—');
  assert.match(await page.locator('.tariff-note').textContent(), /Tariff unavailable/);
  assert.equal(await page.locator('.charge').textContent(), '65.3%');
  assert.equal(await page.locator('[data-node="home"] .value').textContent(), '4.2 kW');
  const accessible = await page.accessibility.snapshot({ root: await page.getByRole('button').elementHandle() });
  assert.doesNotMatch(accessible.description, /Charger 1|3.12|0.4906|0.05 AUD/);
  assert.match(accessible.description, /65.3%/);
});
