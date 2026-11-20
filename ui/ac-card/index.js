/*
Selected Lucide icons, v0.552.0.
ISC License

Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2023 as part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2025.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

---

The MIT License (MIT) (for portions derived from Feather)

Copyright (c) 2013-2023 Cole Bemis

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

// homeCore plugin UI host interface v1. Each adapter serves the same module.
const modes = {
  off: 'Off', heat: 'Heat', cool: 'Cool', auto: 'Auto', dry: 'Dry', fan_only: 'Fan only'
};
const icons = {
  off: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="M12 2v10" /><path d="M18.4 6.6a9 9 0 1 1-12.77.04" /></svg>',
  heat: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="M12 3q1 4 4 6.5t3 5.5a1 1 0 0 1-14 0 5 5 0 0 1 1-3 1 1 0 0 0 5 0c0-2-1.5-3-1.5-5q0-2 2.5-4" /></svg>',
  cool: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="m10 20-1.25-2.5L6 18" /><path d="M10 4 8.75 6.5 6 6" /><path d="m14 20 1.25-2.5L18 18" /><path d="m14 4 1.25 2.5L18 6" /><path d="m17 21-3-6h-4" /><path d="m17 3-3 6 1.5 3" /><path d="M2 12h6.5L10 9" /><path d="m20 10-1.5 2 1.5 2" /><path d="M22 12h-6.5L14 15" /><path d="m4 10 1.5 2L4 14" /><path d="m7 21 3-6-1.5-3" /><path d="m7 3 3 6h4" /></svg>',
  auto: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="M12 2v2" /><path d="M14.837 16.385a6 6 0 1 1-7.223-7.222c.624-.147.97.66.715 1.248a4 4 0 0 0 5.26 5.259c.589-.255 1.396.09 1.248.715" /><path d="M16 12a4 4 0 0 0-4-4" /><path d="m19 5-1.256 1.256" /><path d="M20 12h2" /></svg>',
  dry: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7z" /></svg>',
  fan_only: '<svg aria-hidden="true" focusable="false" xmlns="http://www.w3.org/2000/svg" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ><path d="M10.827 16.379a6.082 6.082 0 0 1-8.618-7.002l5.412 1.45a6.082 6.082 0 0 1 7.002-8.618l-1.45 5.412a6.082 6.082 0 0 1 8.618 7.002l-5.412-1.45a6.082 6.082 0 0 1-7.002 8.618l1.45-5.412Z" /><path d="M12 12v.01" /></svg>'
};
const activities = {
  off: 'Off', idle: 'Idle', heating: 'Heating', cooling: 'Cooling',
  drying: 'Dehumidifying', fan: 'Fan only'
};
const airflow = [
  { key: 'fan_mode', label: 'Fan speed', name: 'Fan speed',
    values: { auto: 'Auto', quiet: 'Quiet', '1': 'Speed 1', '2': 'Speed 2', '3': 'Speed 3', '4': 'Speed 4' } },
  { key: 'vertical_vane', capability: 'has_vertical_vane', label: 'Vertical', name: 'Vertical vane',
    values: { auto: 'Auto', swing: 'Swing', '1': 'Position 1', '2': 'Position 2', '3': 'Position 3',
      '4': 'Position 4', '5': 'Position 5' } },
  { key: 'horizontal_vane', capability: 'has_horizontal_vane', label: 'Horizontal', name: 'Horizontal vane',
    values: { auto: 'Auto', swing: 'Swing', ll: 'Far left', l: 'Left', c: 'Centre', r: 'Right',
      rr: 'Far right', split: 'Split' } }
];
const diagnostics = [
  { key: 'outside_temperature', label: 'Outside temperature', unit: '°C' },
  { key: 'compressor_frequency', label: 'Compressor', unit: 'Hz' },
  { key: 'runtime', label: 'Runtime', unit: 'h' }
];

const style = `
  :host {
    display:block;min-width:0;container-type:inline-size;
    --ac-ink:var(--ink,#233030);--ac-muted:var(--ink-muted,#52625f);
    --ac-surface:var(--surface,#fff);--ac-line:var(--card-line,#d1dbd5);
    --ac-accent:var(--accent,#17634e);
    --ac-wash:color-mix(in srgb,var(--ac-accent) 10%,var(--ac-surface));
    color:var(--ac-ink);font:14px/1.45 var(--font,system-ui,sans-serif);
  }
  *{box-sizing:border-box}
  h3,p,dl,dd{margin:0}
  [hidden]{display:none!important}
  .card{background:var(--ac-surface);border:1px solid var(--ac-line);
    border-radius:var(--radius,17px);padding:15px;min-width:0;text-align:center}
  button,select{font:inherit;color:inherit;background:var(--ac-surface);
    border:1px solid var(--ac-line);border-radius:10px}
  button{cursor:pointer}
  :disabled{opacity:.55;cursor:default}
  :focus-visible{outline:2px solid var(--ac-accent);outline-offset:2px}
  .name{display:block;max-width:100%;min-height:34px;margin:0 auto;padding:0 4px;border:0;
    font-size:18px;font-weight:650;overflow-wrap:anywhere;background:none}
  .name:not(:disabled):hover{text-decoration:underline}
  .name:disabled{opacity:1}
  .room{padding:8px 8px 10px;margin:8px 0 10px;border-radius:12px;background:var(--ac-wash)}
  .room-label,small,dt,.mode-note{color:var(--ac-muted);font-size:12px}
  .dial{position:relative;width:min(198px,100%);aspect-ratio:220 / 155;margin:1px auto 0}
  .dial svg{display:block;width:100%;height:100%;overflow:visible}
  .dial path{fill:none;stroke-width:7;stroke-linecap:round}
  .track{stroke:color-mix(in srgb,var(--ac-accent) 24%,var(--ac-surface))}
  .range{stroke:var(--ac-accent)}
  .room-dot{fill:var(--ac-ink);stroke:var(--ac-surface);stroke-width:2}
  .target-dot{fill:var(--ac-surface);stroke:var(--ac-accent);stroke-width:3}
  .reading-wrap{position:absolute;inset:0;display:grid;place-items:center;pointer-events:none}
  .reading{font-size:clamp(26px,10cqi,34px);font-weight:650;letter-spacing:-.04em;white-space:nowrap}
  .reading,output,dd{font-variant-numeric:tabular-nums}
  .activity{display:block;color:var(--ac-accent);font-size:13px;font-weight:620}
  .offline .activity{color:var(--ac-muted)}
  .offline .dial{opacity:.65}
  .target{display:grid;grid-template-columns:44px minmax(0,108px) 44px;justify-content:center;
    align-items:center;gap:8px;margin:8px 0}
  .target button{width:44px;height:44px;padding:0;font-size:20px}
  .target-value{grid-column:2;display:grid;gap:1px}
  .target button:last-child{grid-column:3}
  output{font-size:17px;font-weight:620;white-space:nowrap}
  .modes,.airflow-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,72px),1fr));gap:6px}
  .modes{grid-template-columns:repeat(3,minmax(0,1fr));margin:8px 0}
  .modes button{display:grid;justify-items:center;align-content:center;gap:3px;min-height:52px;min-width:0;
    padding:5px 2px;font-size:12px;overflow-wrap:anywhere}
  .modes svg{display:block;width:20px;height:20px}
  .modes button[aria-pressed=true]{border-color:var(--ac-accent);background:var(--ac-wash);font-weight:700}
  .mode-note,.feedback{margin-top:8px}
  .feedback{font-size:13px}
  .feedback:empty{margin:0}
  .airflow,.diagnostics{margin-top:14px;padding-top:11px;border-top:1px solid var(--ac-line)}
  .airflow h3{font-size:14px;margin-bottom:8px}
  .airflow-field{display:grid;justify-items:center;align-content:start;gap:4px;min-width:0}
  .airflow-field select{width:100%;min-width:0;min-height:42px;padding-inline:10px;font-size:13px;
    text-align:center;text-align-last:center;appearance:none;background:var(--ac-surface)
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='m3 4.5 3 3 3-3' stroke='%2373807e' stroke-width='1.25' fill='none'/%3E%3C/svg%3E")
    right 2px center/10px no-repeat}
  .airflow-field dd{width:100%;font-weight:620;overflow-wrap:anywhere}
  .diagnostics summary{width:fit-content;margin-inline:auto;min-height:35px;cursor:pointer;font-weight:550}
  .diagnostics dl{display:grid;grid-template-columns:1fr auto;gap:6px 10px;text-align:left}
  .diagnostics dd{text-align:right}
  @container (max-width:225px){.card{padding:9px}}
  @media(forced-colors:active){.airflow-field select{appearance:auto;background:var(--ac-surface);padding-inline:3px}}
`;

const targetRow = (key, label, name) => `
  <div class="target" data-target="${key}" role="group" aria-label="${name}">
    <button type="button" data-step="-1" aria-label="Lower ${name.toLowerCase()}">−</button>
    <div class="target-value"><small>${label}</small><output aria-live="off"></output></div>
    <button type="button" data-step="1" aria-label="Raise ${name.toLowerCase()}">+</button>
  </div>`;

const template = `
  <style>${style}</style>
  <section class="card">
    <button class="name" type="button"></button>
    <div class="room">
      <div class="room-label">Room temperature</div>
      <div class="dial">
        <svg viewBox="0 0 220 155" aria-hidden="true">
          <path class="track"></path><path class="range"></path>
          <circle class="target-dot low" r="5"></circle><circle class="target-dot high" r="5"></circle>
          <circle class="room-dot" r="5"></circle>
        </svg>
        <div class="reading-wrap"><strong class="reading"></strong></div>
      </div>
      <span class="activity"></span>
    </div>
    ${targetRow('single', 'Target', 'Target')}
    ${targetRow('heat', 'Heat to', 'Heating target')}
    ${targetRow('cool', 'Cool to', 'Cooling target')}
    <p class="gap-note mode-note" hidden></p>
    <div class="modes" role="group" aria-label="Mode">
      ${Object.entries(modes).map(([key, label]) => `<button type="button" data-mode="${key}" aria-pressed="false">
        ${icons[key]}<span>${label}</span></button>`).join('')}
    </div>
    <p class="mode-note current-mode" hidden></p>
    <section class="airflow" aria-labelledby="airflow-title">
      <h3 id="airflow-title">Airflow</h3>
      <dl class="airflow-grid">${airflow.map(({ key, label, name }) => `
        <div class="airflow-field" data-airflow="${key}">
          <dt>${label}</dt>
          <dd><span class="reported"></span><select aria-label="${name}" hidden></select></dd>
        </div>`).join('')}</dl>
      <p class="mode-note airflow-note" hidden>Turn on the unit to adjust airflow.</p>
    </section>
    <p class="feedback" role="status"></p>
    <details class="diagnostics">
      <summary>Diagnostics</summary>
      <dl>${diagnostics.map(({ key, label }) =>
        `<dt data-diagnostic="${key}">${label}</dt><dd data-diagnostic="${key}"></dd>`).join('')}</dl>
    </details>
  </section>`;

// Keep the inputs typed before running any display or command rules.
const record = value => value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {};
const number = value => typeof value === 'number' && Number.isFinite(value) ? value : undefined;
const temperature = value => number(value) >= -100 && value <= 150 ? value : undefined;
const choice = value => typeof value === 'string' && /^[a-zA-Z0-9_. -]{1,40}$/.test(value) ? value : undefined;
const lookup = (table, key) => typeof key === "string" && Object.hasOwn(table, key) ? table[key] : undefined;
const show = (element, visible) => element.toggleAttribute('hidden', !visible);
const stateValue = (model, key) => model.targetsByKey[key] ?? model.reported[key];

function normalizeOption(option) {
  const value = choice(typeof option === 'string' ? option : option?.value);
  const label = typeof option?.label === 'string' ? option.label.slice(0, 64) : undefined;
  return value === undefined ? undefined : { value, label };
}

function normalizeSchema(input) {
  const attributes = record(record(input).attributes);
  return Object.fromEntries([
    'hvac_mode', 'setpoint', 'heat_setpoint', 'cool_setpoint', ...airflow.map(field => field.key)
  ].map(key => {
    const spec = record(attributes[key]);
    const options = Array.isArray(spec.options) ? spec.options.slice(0, 32).map(normalizeOption).filter(Boolean) : [];
    return [key, { writable: spec.writable === true, min: temperature(spec.min),
      max: temperature(spec.max), step: number(spec.step), options }];
  }));
}

function climateOf(device) {
  const a = record(device?.attributes);
  const mode = lookup(modes, a.hvac_mode) ? a.hvac_mode : undefined;
  const heat = temperature(a.heat_setpoint ?? a.heating_setpoint);
  const cool = temperature(a.cool_setpoint ?? a.cooling_setpoint);
  const single = temperature(a.setpoint);
  const dual = mode === 'auto' && heat !== undefined && cool !== undefined && heat <= cool;
  const controlsTemperature = mode === 'heat' || mode === 'cool' || mode === 'auto';
  const singleKey = single !== undefined ? 'setpoint' : mode === 'heat' ? 'heat_setpoint' :
    mode === 'cool' ? 'cool_setpoint' : 'setpoint';
  const targetsByKey = { setpoint: single, heat_setpoint: temperature(a.heat_setpoint),
    cool_setpoint: temperature(a.cool_setpoint) };
  const targets = dual ? { heat: { key: 'heat_setpoint', value: heat },
    cool: { key: 'cool_setpoint', value: cool } } : controlsTemperature ?
    { single: { key: singleKey, value: single ?? (mode === 'heat' ? heat : mode === 'cool' ? cool : undefined) } } : {};
  let activity = mode === 'off' ? 'Off' : lookup(activities, a.hvac_action);
  if (mode === 'heat' || mode === 'auto') {
    activity = lookup({ defrost: 'Defrosting', preheat: 'Preheating', standby: 'Standby' }, a.sub_mode) ?? activity;
  }
  return { available: device?.available === true, mode, activity, room: temperature(a.temperature),
    gap: number(a.min_setpoint_gap) >= 0 && a.min_setpoint_gap <= 30 ? a.min_setpoint_gap : 0,
    targets, targetsByKey, reported: { hvac_mode: mode,
      ...Object.fromEntries(airflow.map(({ key }) => [key, choice(a[key])])) },
    unsupportedAirflow: new Set(airflow.filter(({ capability }) => capability && a[capability] === false)
      .map(field => field.key)),
    diagnostics: Object.fromEntries(diagnostics.map(({ key }) => [key,
      key === 'outside_temperature' ? temperature(a[key]) : number(a[key])])) };
}

function nextTarget(target, spec, direction, model) {
  if (!spec?.writable || model.targetsByKey[target.key] === undefined ||
      spec.min === undefined || spec.max === undefined || !(spec.step > 0)) return undefined;
  const offset = (target.value - spec.min) / spec.step;
  const nearest = Math.round(offset);
  const tick = Math.abs(offset - nearest) < 1e-6 ? nearest + direction :
    direction > 0 ? Math.ceil(offset) : Math.floor(offset);
  const value = Number((spec.min + tick * spec.step).toFixed(6));
  if (!Number.isFinite(value) || value < spec.min || value > spec.max || value === target.value) return undefined;
  if (model.targets.heat && (target.key === 'heat_setpoint' ?
      value > model.targets.cool.value - model.gap : value < model.targets.heat.value + model.gap)) return undefined;
  return value;
}

function dialPoint(value, radius = 69) {
  const progress = Math.max(0, Math.min(1, (value - 16) / 14.5));
  const angle = (135 + 270 * progress) * Math.PI / 180;
  return [110 + radius * Math.cos(angle), 88 + radius * Math.sin(angle)];
}

function arc(from, to) {
  const [x, y] = dialPoint(from);
  const [endX, endY] = dialPoint(to);
  const large = Math.min(30.5, to) - Math.max(16, from) > 14.5 * 2 / 3 ? 1 : 0;
  return `M${x} ${y} A69 69 0 ${large} 1 ${endX} ${endY}`;
}

function mark(dot, value, radius) {
  show(dot, value !== undefined);
  if (value === undefined) return;
  const [x, y] = dialPoint(value, radius);
  dot.setAttribute('cx', x);
  dot.setAttribute('cy', y);
}

class AcCard extends HTMLElement {
  #host;
  #deviceId;
  #view;
  #stopDevices;
  #stopUnits;
  #schemaRequest;
  #schema;
  #available = false;
  #pending;
  #message = '';
  #commands = new Map();

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  set host(host) {
    if (host === this.#host) return;
    this.#stop();
    this.#host = host;
    if (this.isConnected) this.#listen();
  }

  set deviceId(id) {
    if (id === this.#deviceId) return;
    this.#resetDevice();
    this.#deviceId = typeof id === 'string' && id.length <= 256 ? id : undefined;
    this.#render();
  }

  connectedCallback() {
    if (!this.#view) this.#mount();
    this.#listen();
  }

  disconnectedCallback() {
    this.#stop();
  }

  #mount() {
    const root = this.shadowRoot;
    root.innerHTML = template;
    const get = selector => root.querySelector(selector);
    this.#view = {
      card: get('.card'), name: get('.name'), reading: get('.reading'), activity: get('.activity'),
      track: get('.track'), range: get('.range'), room: get('.room-dot'), low: get('.low'), high: get('.high'),
      modeGrid: get('.modes'), modeButtons: [...root.querySelectorAll('[data-mode]')], mode: get('.current-mode'),
      targets: [...root.querySelectorAll('[data-target]')].map(row => ({
        row, output: row.querySelector('output'), buttons: [...row.querySelectorAll('button')]
      })),
      gap: get('.gap-note'), airflow: get('.airflow'), airflowNote: get('.airflow-note'),
      fields: airflow.map(field => {
        const row = get(`[data-airflow="${field.key}"]`);
        return { ...field, row, select: row.querySelector('select'), reading: row.querySelector('.reported') };
      }),
      diagnostics: get('.diagnostics'), feedback: get('.feedback')
    };
    this.#view.track.setAttribute('d', arc(16, 30.5));
    this.#view.name.addEventListener('click', () => this.#host?.ui?.openDevice(this.#deviceId));
    root.addEventListener('click', event => {
      const command = this.#commands.get(event.target.closest('button'));
      if (command) void this.#send(...command);
    });
    for (const { key, select } of this.#view.fields) {
      select.addEventListener('change', () => {
        const values = this.#commands.get(select);
        if (values?.has(select.value)) void this.#send(key, select.value);
        else this.#render();
      });
    }
  }

  #listen() {
    const sheet = this.#host?.theme?.sheet;
    this.shadowRoot.adoptedStyleSheets = sheet instanceof CSSStyleSheet ? [sheet] : [];
    this.#stopDevices = this.#host?.devices.subscribe(() => this.#render());
    this.#stopUnits = this.#host?.units?.subscribe(() => this.#render());
    this.#render();
  }

  #stop() {
    this.#stopDevices?.();
    this.#stopUnits?.();
    this.#stopDevices = this.#stopUnits = undefined;
    this.#resetDevice();
  }

  #resetDevice() {
    this.#schemaRequest?.abort();
    this.#schemaRequest = undefined;
    this.#schema = undefined;
    this.#available = false;
    this.#finish('');
  }

  async #loadSchema() {
    if (typeof this.#host?.fetch !== 'function') return;
    const request = new AbortController();
    this.#schemaRequest = request;
    try {
      const response = await this.#host.fetch(`/devices/${encodeURIComponent(this.#deviceId)}/schema`, { signal: request.signal });
      const data = response.ok ? normalizeSchema(await response.json()) : undefined;
      if (this.#schemaRequest === request) this.#schema = data;
    } catch {
      // Devices remain readable while the schema endpoint is unavailable.
    } finally {
      if (this.#schemaRequest === request) {
        this.#schemaRequest = undefined;
        this.#render();
      }
    }
  }

  #format(value, unit = '°C') {
    if (value === undefined) return '—';
    const shown = this.#host?.units?.show(value, unit)?.text;
    return typeof shown === 'string' ? shown : `${value} ${unit}`;
  }

  #render() {
    if (!this.isConnected || !this.#view) return;
    const device = this.#deviceId ? this.#host?.devices.get(this.#deviceId) : undefined;
    const model = climateOf(device);
    const { available, mode, activity, room, targets } = model;
    if (available !== this.#available) {
      this.#available = available;
      if (available) void this.#loadSchema();
      else this.#resetDevice();
    }
    if (this.#pending?.sent && stateValue(model, this.#pending.key) === this.#pending.value) this.#finish('');
    const canWrite = available && (this.#host?.me?.can('devices:command') ||
      this.#host?.me?.can('devices:write')) && typeof this.#host?.command === 'function';
    const schema = canWrite ? this.#schema : undefined;
    this.#commands.clear();

    const view = this.#view;
    view.card.classList.toggle('offline', !available);
    const name = device && this.#host.devices.label(device);
    view.name.textContent = typeof name === 'string' && name ? name.slice(0, 256) : 'Air conditioner';
    view.name.disabled = !device || typeof this.#host.ui?.openDevice !== 'function';
    view.reading.textContent = this.#format(available ? room : undefined);
    view.activity.textContent = !device ? 'Waiting for device' : !available ? 'Unavailable' :
      activity ?? (room === undefined ? 'Waiting for readings' : '');
    const low = available ? targets.heat?.value ?? targets.single?.value : undefined;
    const high = available ? targets.cool?.value : undefined;
    view.range.setAttribute('d', high !== undefined && low < high ? arc(low, high) : '');
    mark(view.room, available ? room : undefined, 82);
    mark(view.low, low);
    mark(view.high, high === low ? undefined : high);

    const dualWritable = !!(schema?.heat_setpoint.writable || schema?.cool_setpoint.writable);
    show(view.gap, available && dualWritable && !!targets.heat && model.gap > 0 &&
      targets.cool.value - targets.heat.value <= model.gap);
    view.gap.textContent = 'Minimum temperature gap reached';
    for (const { row, output, buttons } of view.targets) {
      const target = targets[row.dataset.target];
      const visible = available && target?.value !== undefined;
      show(row, visible);
      output.textContent = this.#format(visible ? target.value : undefined);
      const spec = schema?.[target?.key];
      for (const button of buttons) {
        const value = visible ? nextTarget(target, spec, Number(button.dataset.step), model) : undefined;
        show(button, spec?.writable === true);
        button.disabled = value === undefined || !!this.#pending;
        if (!button.disabled) this.#commands.set(button, [target.key, value]);
      }
    }

    const writableModes = schema?.hvac_mode.writable ? new Set(schema.hvac_mode.options.map(option => option.value)) : new Set();
    let hasModeButtons = false;
    for (const button of view.modeButtons) {
      const value = button.dataset.mode;
      const visible = writableModes.has(value);
      show(button, visible);
      hasModeButtons ||= visible;
      button.setAttribute('aria-pressed', String(value === mode));
      button.disabled = !!this.#pending;
      if (visible && !button.disabled && value !== mode) this.#commands.set(button, ['hvac_mode', value]);
    }
    show(view.modeGrid, hasModeButtons);
    show(view.mode, available && !!mode && !writableModes.has(mode) && modes[mode] !== activity);
    view.mode.textContent = mode ? `Mode · ${modes[mode]}` : '';

    this.#renderAirflow(model, schema);
    const readings = diagnostics.filter(({ key }) => available && model.diagnostics[key] !== undefined);
    view.diagnostics.hidden = readings.length === 0;
    for (const { key, unit } of diagnostics) {
      const value = available ? model.diagnostics[key] : undefined;
      const [label, output] = view.diagnostics.querySelectorAll(`[data-diagnostic="${key}"]`);
      show(label, value !== undefined);
      show(output, value !== undefined);
      output.textContent = this.#format(value, unit);
    }
    if (view.feedback.textContent !== this.#message) view.feedback.textContent = this.#message;
  }

  #renderAirflow({ available, mode, reported, unsupportedAirflow }, schema) {
    let hasAirflow = false;
    let canAdjustAirflow = false;
    for (const { key, row, select, reading, values } of this.#view.fields) {
      const spec = schema?.[key];
      const supported = !unsupportedAirflow.has(key);
      const writable = supported && spec?.writable && spec.options.length > 0;
      const raw = reported[key];
      const optionLabel = option => option.label ?? lookup(values, option.value) ?? option.value;
      const visible = supported && (writable || (raw !== undefined && raw !== 'unknown'));
      show(row, visible);
      hasAirflow ||= visible;
      show(select, writable);
      show(reading, !writable);
      reading.textContent = available ? spec?.options.find(option => option.value === raw)?.label ??
        lookup(values, raw) ?? raw ?? '—' : '—';
      if (!writable) continue;
      const options = spec.options.map(option => [option.value, optionLabel(option)]);
      if (!options.some(([value]) => value === raw)) options.unshift(['', reading.textContent]);
      const serialized = JSON.stringify(options);
      if (select.dataset.options !== serialized) {
        select.replaceChildren(...options.map(([value, label]) => new Option(label, value)));
        select.dataset.options = serialized;
      }
      select.value = options.some(([value]) => value === raw) ? raw : '';
      select.disabled = mode === 'off' || mode === undefined || !!this.#pending;
      canAdjustAirflow = true;
      if (!select.disabled) {
        this.#commands.set(select, new Set(spec.options.map(option => option.value).filter(value => value !== raw)));
      }
    }
    show(this.#view.airflow, hasAirflow);
    show(this.#view.airflowNote, canAdjustAirflow && mode === 'off');
  }

  async #send(key, value) {
    const pending = { key, value, sent: false, timer: setTimeout(() => {
      this.#finish('Change not confirmed.');
      this.#render();
    }, 15000) };
    this.#pending = pending;
    this.#message = 'Waiting for the unit…';
    this.#render();
    try {
      const result = await this.#host.command(this.#deviceId, { [key]: value });
      if (this.#pending !== pending) return;
      if (result?.ok === true) pending.sent = true;
      else this.#finish('Could not send change.');
    } catch {
      if (this.#pending === pending) this.#finish('Could not send change.');
    }
    this.#render();
  }

  #finish(message) {
    clearTimeout(this.#pending?.timer);
    this.#pending = undefined;
    this.#message = message;
  }
}

if (!customElements.get('hc-ac-card')) customElements.define('hc-ac-card', AcCard);
