import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
const ui = process.env.TESLEMETRY_TEST_UI || new URL('../../ui/index.js', import.meta.url);
test('estimates energy cost and rejects absent or invalid inputs', async () => {
  const source = await readFile(ui, 'utf8');
  const estimate = runInNewContext(`${source}\nestimateEnergyCost`, {
    HTMLElement: class {}, customElements: {define() {}}
  });
  assert.equal(estimate(3000, 0.3, 'aud'), 'AUD 0.90');
  assert.equal(estimate(0, 0.3, 'AUD'), 'AUD 0.00');
  assert.equal(estimate(5000, -0.1, 'AUD'), 'AUD -0.50');
  for (const [energy, rate, currency] of [[-1, 0.3, 'AUD'], [NaN, 0.3, 'AUD'], [1000, Infinity, 'AUD'], [1000, null, 'AUD'], [1000, 0.3, '???']]) {
    assert.equal(estimate(energy, rate, currency), undefined);
  }
});
