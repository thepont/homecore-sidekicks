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
    --ac-step:var(--surface-2,#edf1ee);
    color:var(--ac-ink);font:14px/1.45 var(--font,system-ui,sans-serif);
  }
  *{box-sizing:border-box}
  h3,p,dl,dd{margin:0}
  [hidden]{display:none!important}
  :host(hc-ac-panel){--ac-dial-size:236px}
  :host(hc-ac-panel) .card{padding:0;border:0;background:transparent}
  .card{background:var(--ac-surface);border:1px solid var(--ac-line);
    border-radius:var(--radius,17px);padding:15px;min-width:0;text-align:center}
  button,select{font:inherit;color:inherit;background:var(--ac-surface);
    border:1px solid var(--ac-line);border-radius:10px}
  button{cursor:pointer}
  :disabled{opacity:.55;cursor:default}
  :focus-visible{outline:2px solid var(--ac-accent);outline-offset:2px}
  .name{display:block;max-width:100%;min-height:44px;margin:3px auto 5px;padding:0 4px;border:0;
    font-size:18px;font-weight:650;overflow-wrap:anywhere;background:none}
  .name:not(:disabled):hover{text-decoration:underline}
  .name:disabled{opacity:1}
  .room{margin:0 0 5px}
  .readout{position:absolute;left:50%;top:51.28%;transform:translate(-50%,-50%);
    display:grid;justify-items:center;gap:2px;width:57%;line-height:1.2}
  button.readout{border:0;background:none;padding:3px 0;min-height:44px;border-radius:12px}
  button.readout:disabled{opacity:1}
  .open-label{display:block;color:var(--ac-accent);font-weight:620;font-size:12px;
    text-underline-offset:2px}
  .name:hover .open-label,.name:focus-visible .open-label{text-decoration:underline}
  .room-label,small,dt,.mode-note{color:var(--ac-muted);font-size:12px}
  .dial{position:relative;container-type:inline-size;width:min(var(--ac-dial-size,168px),100%);
    aspect-ratio:168 / 156;margin:0 auto}
  .dial[data-editable]{touch-action:none;user-select:none;cursor:grab}
  .dial[data-dragging]{cursor:grabbing}
  .dial svg{display:block;width:100%;height:100%;overflow:visible}
  .dial-face{stroke:var(--ac-line);stroke-width:1}
  .face-center{stop-color:var(--ac-surface)}
  .face-edge{stop-color:color-mix(in srgb,var(--ac-surface) 54%,var(--bg,var(--ac-step)))}
  .dial path{fill:none;stroke-width:13}
  .track{stroke:var(--track-off,var(--ac-step))}
  .range-start{stop-color:var(--track-on,#ffb45b)}
  .range-end{stop-color:var(--ac-accent)}
  .card[data-cooling] .range-start{stop-color:color-mix(in srgb,var(--cool,#5aa9e6) 68%,var(--ac-ink))}
  .card[data-cooling] .range-end{stop-color:var(--cool,#5aa9e6)}
  .room-dot{fill:var(--ac-ink);stroke:var(--ac-surface);stroke-width:2}
  .target-dot{fill:var(--ac-surface);stroke:var(--ac-accent);stroke-width:3}
  .target-dot[role="slider"]{r:6.5;fill:var(--ac-accent);stroke:var(--ac-surface)}
  .target-dot[role="slider"]:focus-visible{outline:none;stroke:var(--ac-ink);stroke-width:4}
  .room-current{display:flex;flex-direction:column;align-items:center;justify-content:center}
  .reading,.dial-target{font-size:clamp(17px,17cqw,42px);font-weight:680;line-height:1.08;white-space:nowrap}
  .reading,.dial-target,output,dd{font-variant-numeric:tabular-nums}
  .room-label,.activity{font-size:clamp(10px,7.5cqw,15px);text-wrap:balance}
  .activity{color:var(--ac-muted)}
  .dial[data-has-target] .room-current{flex-direction:row;gap:.3em}
  .dial:not([data-has-target]) .reading{order:-1}
  .dial[data-has-target] .reading{font-size:clamp(10px,7.5cqw,15px);font-weight:500;color:var(--ac-muted)}
  .offline .dial svg{opacity:.65}
  .target{display:grid;grid-template-columns:44px minmax(0,108px) 44px;justify-content:center;
    align-items:center;gap:clamp(4px,2.5cqi,8px);margin:8px 0}
  .target button{width:44px;height:44px;padding:0;border-radius:50%;font-size:22px;line-height:1;
    background:var(--ac-step)}
  .target-value{grid-column:2;display:grid;gap:1px}
  .target button:last-child{grid-column:3}
  output{font-size:clamp(13px,9cqi,17px);font-weight:620;white-space:nowrap}
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
  @container (max-width:225px){
    .card{padding:9px}
    output{font-size:clamp(13px,7.5cqi,17px)}
  }
  @media (any-pointer:coarse) {.target-dot[role="slider"]{r:8}}
  @media(forced-colors:active){
    .airflow-field select{appearance:auto;background:var(--ac-surface);padding-inline:3px}
    .dial-face{fill:Canvas;stroke:CanvasText}
    .track{stroke:GrayText}
    .range{stroke:Highlight}
    .target-dot[role="slider"]{fill:Highlight;stroke:Canvas}
    .target-dot[role="slider"]:focus-visible{stroke:CanvasText}
  }
`;

const targetRow = (key, label, name) => `
  <div class="target" data-target="${key}" role="group" aria-label="${name}">
    <button type="button" data-step="-1" aria-label="Lower ${name.toLowerCase()}">−</button>
    <div class="target-value"><small>${label}</small>${key === 'single' ? '' : '<output aria-live="off"></output>'}</div>
    <button type="button" data-step="1" aria-label="Raise ${name.toLowerCase()}">+</button>
  </div>`;

const template = panel => {
  const readoutTag = panel ? 'div' : 'button';
  return `
  <style>${style}</style>
  <section class="card">
    <div class="room">
      <div class="dial">
        <svg viewBox="26 8 168 156" role="group" aria-label="Temperature dial" aria-hidden="true">
          <defs>
            <radialGradient id="ac-face"><stop class="face-center" offset="38%"/><stop class="face-edge" offset="100%"/></radialGradient>
            <linearGradient id="ac-range" gradientUnits="userSpaceOnUse" x1="62" y1="138" x2="158" y2="37">
              <stop class="range-start" offset="0%"/><stop class="range-end" offset="100%"/>
            </linearGradient>
          </defs>
          <circle class="dial-face" cx="110" cy="88" r="62" fill="url(#ac-face)" aria-hidden="true"></circle>
          <path class="track" aria-hidden="true"></path>
          <path class="range" stroke="url(#ac-range)" aria-hidden="true"></path>
          <circle class="target-dot low" r="5" aria-hidden="true"></circle><circle class="target-dot high" r="5" aria-hidden="true"></circle>
          <circle class="room-dot" r="5" aria-hidden="true"></circle>
        </svg>
        <${readoutTag} class="readout"${panel ? '' : ' type="button" aria-haspopup="dialog"'}>
          <span class="activity" id="ac-dial-activity"></span>
          <strong class="dial-target" id="ac-dial-target" hidden></strong>
          <span class="room-current" id="ac-dial-room"><span class="room-label">Room temp</span> <strong class="reading"></strong></span>
        </${readoutTag}>
      </div>
    </div>
    ${targetRow('single', 'Target', 'Target')}
    ${targetRow('heat', 'Heat to', 'Heating target')}
    ${targetRow('cool', 'Cool to', 'Cooling target')}
    ${panel ? '' : '<button class="name" type="button" aria-haspopup="dialog"><span class="name-label"></span><span class="open-label" aria-hidden="true">Open controls</span></button>'}
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
};

// Keep the inputs typed before running any display or command rules.
const MIN_VALID_TEMPERATURE = -100;
const MAX_VALID_TEMPERATURE = 150;
const record = value => value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {};
const number = value => typeof value === 'number' && Number.isFinite(value) ? value : undefined;
const temperature = value => number(value) >= MIN_VALID_TEMPERATURE && value <= MAX_VALID_TEMPERATURE ? value : undefined;
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

function readTemperatureTargets(attributes, mode) {
  const { heat_setpoint: heatSetpoint, cool_setpoint: coolSetpoint } = attributes;
  const single = temperature(attributes.setpoint);
  const targetsByKey = {
    setpoint: single,
    heat_setpoint: temperature(heatSetpoint),
    cool_setpoint: temperature(coolSetpoint)
  };
  const heat = heatSetpoint == null ? temperature(attributes.heating_setpoint) : targetsByKey.heat_setpoint;
  const cool = coolSetpoint == null ? temperature(attributes.cooling_setpoint) : targetsByKey.cool_setpoint;

  if (mode === 'auto' && heat !== undefined && cool !== undefined && heat <= cool) {
    return { targetsByKey, targets: {
      heat: { key: 'heat_setpoint', value: heat },
      cool: { key: 'cool_setpoint', value: cool }
    } };
  }

  if (mode !== 'heat' && mode !== 'cool' && mode !== 'auto') {
    return { targetsByKey, targets: {} };
  }

  if (single !== undefined || mode === 'auto') {
    return { targetsByKey, targets: { single: { key: 'setpoint', value: single } } };
  }

  const target = mode === 'heat'
    ? { key: 'heat_setpoint', value: heat }
    : { key: 'cool_setpoint', value: cool };
  return { targetsByKey, targets: { single: target } };
}

function climateOf(device) {
  const attributes = record(device?.attributes);
  const mode = lookup(modes, attributes.hvac_mode) ? attributes.hvac_mode : undefined;
  const room = temperature(attributes.temperature);
  const { targets, targetsByKey } = readTemperatureTargets(attributes, mode);
  let activity = mode === 'off' ? 'Off' : lookup(activities, attributes.hvac_action);
  if (mode === 'heat' || mode === 'auto') {
    activity = lookup({ defrost: 'Defrosting', preheat: 'Preheating', standby: 'Standby' }, attributes.sub_mode) ?? activity;
  }
  if (room !== undefined) activity ??= lookup(modes, mode);
  return { available: device?.available === true, mode, activity, room,
    gap: number(attributes.min_setpoint_gap) >= 0 && attributes.min_setpoint_gap <= 30 ? attributes.min_setpoint_gap : 0,
    targets, targetsByKey, reported: { hvac_mode: mode,
      ...Object.fromEntries(airflow.map(({ key }) => [key, choice(attributes[key])])) },
    unsupportedAirflow: new Set(airflow.filter(({ capability }) => capability && attributes[capability] === false)
      .map(field => field.key)),
    diagnostics: Object.fromEntries(diagnostics.map(({ key }) => [key,
      key === 'outside_temperature' ? temperature(attributes[key]) : number(attributes[key])])) };
}

function targetControl(target, spec, model) {
  if (!target || !spec?.writable || model.targetsByKey[target.key] === undefined ||
      spec.min === undefined || spec.max === undefined || !(spec.step > 0)) return undefined;
  let min = spec.min;
  let max = spec.max;
  if (model.targets.heat) {
    if (target.key === 'heat_setpoint') max = Math.min(max, model.targets.cool.value - model.gap);
    else min = Math.max(min, model.targets.heat.value + model.gap);
  }
  const offset = { origin: spec.min, step: spec.step };
  const first = atTick(offset, Math.ceil((min - spec.min) / spec.step - 1e-6));
  const last = atTick(offset, Math.floor((max - spec.min) / spec.step + 1e-6));
  if (!Number.isFinite(first) || !Number.isFinite(last) || first > last) return undefined;
  return { ...target, ...offset, min: first, max: last };
}

function atTick(control, tick) {
  return Number((control.origin + tick * control.step).toFixed(6));
}

function nextTarget(control, direction) {
  if (!control) return undefined;
  const offset = (control.value - control.origin) / control.step;
  const nearest = Math.round(offset);
  const tick = Math.abs(offset - nearest) < 1e-6 ? nearest + direction :
    direction > 0 ? Math.ceil(offset) : Math.floor(offset);
  const value = atTick(control, tick);
  return value >= control.min && value <= control.max && value !== control.value ? value : undefined;
}

function snapTarget(control, value) {
  const tick = Math.round((value - control.origin) / control.step);
  return Math.max(control.min, Math.min(control.max, atTick(control, tick)));
}

const dialGeometry = { x: 110, y: 88, radius: 69, start: 135, sweep: 270 };
const defaultScale = { min: 16, max: 30.5 };
const dialLabels = { single: 'Target temperature', heat: 'Heating target', cool: 'Cooling target' };

function dialPoint(value, scale, radius = dialGeometry.radius) {
  const progress = Math.max(0, Math.min(1, (value - scale.min) / (scale.max - scale.min)));
  const angle = (dialGeometry.start + dialGeometry.sweep * progress) * Math.PI / 180;
  return [dialGeometry.x + radius * Math.cos(angle), dialGeometry.y + radius * Math.sin(angle)];
}

function arc(from, to, scale) {
  const [x, y] = dialPoint(from, scale);
  const [endX, endY] = dialPoint(to, scale);
  const { radius, sweep } = dialGeometry;
  const span = Math.min(scale.max, to) - Math.max(scale.min, from);
  const large = span / (scale.max - scale.min) * sweep > 180 ? 1 : 0;
  return `M${x} ${y} A${radius} ${radius} 0 ${large} 1 ${endX} ${endY}`;
}

function mark(dot, value, scale, radius) {
  show(dot, value !== undefined);
  if (value === undefined) return;
  const [x, y] = dialPoint(value, scale, radius);
  dot.setAttribute('cx', x);
  dot.setAttribute('cy', y);
}

function dialPointer(svg, event) {
  const screen = svg.getScreenCTM();
  if (!screen) return undefined;
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(screen.inverse());
  const x = point.x - dialGeometry.x;
  const y = point.y - dialGeometry.y;
  const degrees = Math.atan2(y, x) * 180 / Math.PI;
  return { x: point.x, y: point.y, radius: Math.hypot(x, y),
    angle: (degrees - dialGeometry.start + 720) % 360, pixelsPerUnit: Math.hypot(screen.a, screen.b) };
}

function updateAirflowOptions({ select, reading, values }, choices, reported) {
  const options = choices.map(({ value, label }) => [value, label ?? lookup(values, value) ?? value]);
  const hasReportedOption = options.some(([value]) => value === reported);
  if (!hasReportedOption) options.unshift(['', reading.textContent]);
  const serialized = JSON.stringify(options);
  if (select.dataset.options !== serialized) {
    select.replaceChildren(...options.map(([value, label]) => new Option(label, value)));
    select.dataset.options = serialized;
  }
  select.value = hasReportedOption ? reported : '';
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
  #dialState;
  #drag;

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
    root.innerHTML = template(this.localName === 'hc-ac-panel');
    const get = selector => root.querySelector(selector);
    const diagnosticsPanel = get('.diagnostics');
    this.#view = {
      card: get('.card'), name: get('.name'), nameLabel: get('.name-label'), readout: get('.readout'),
      openLabel: get('.open-label'), roomLabel: get('.room-label'), dialTarget: get('.dial-target'),
      reading: get('.reading'), activity: get('.activity'),
      dial: get('.dial'), svg: get('.dial svg'), track: get('.track'), range: get('.range'), room: get('.room-dot'), low: get('.low'), high: get('.high'),
      modeGrid: get('.modes'), modeButtons: [...root.querySelectorAll('[data-mode]')], mode: get('.current-mode'),
      targets: [...root.querySelectorAll('[data-target]')].map(row => ({
        row, output: row.querySelector('output'), buttons: [...row.querySelectorAll('button')]
      })),
      gap: get('.gap-note'), airflow: get('.airflow'), airflowNote: get('.airflow-note'),
      fields: airflow.map(field => {
        const row = get(`[data-airflow="${field.key}"]`);
        return { ...field, row, select: row.querySelector('select'), reading: row.querySelector('.reported') };
      }),
      diagnosticsPanel,
      diagnosticFields: diagnostics.map(({ key, unit }) => {
        const [label, output] = diagnosticsPanel.querySelectorAll(`[data-diagnostic="${key}"]`);
        return { key, unit, label, output };
      }),
      feedback: get('.feedback')
    };
    this.#listenDial();
    if (this.#view.name) {
      const open = () => this.#host?.ui?.openDevice(this.#deviceId);
      this.#view.name.addEventListener('click', open);
      this.#view.readout.addEventListener('click', open);
    }
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
    this.#cancelDrag();
    this.#dialState = undefined;
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

  #roomReading(value) {
    if (value === undefined) return '—';
    const shown = this.#host?.units?.show(value, '°C');
    if (number(shown?.value) !== undefined && (shown.unit === '°C' || shown.unit === '°F')) {
      return `${Math.round(shown.value)}${shown.unit}`;
    }
    return `${Math.round(value)}°C`;
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
    const { available, activity, room } = model;
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
    view.card.toggleAttribute('data-cooling', model.mode === 'cool' || activity === 'Cooling');
    this.#renderIdentity(device);
    view.reading.textContent = this.#roomReading(available ? room : undefined);
    view.activity.textContent = !device ? 'Waiting for device' : !available ? 'Unavailable' :
      activity ?? (room === undefined ? 'Waiting for readings' : '');
    this.#renderTargets(model, schema);

    this.#renderModes(model, schema);
    this.#renderAirflow(model, schema);
    this.#renderDiagnostics(model);
    if (view.feedback.textContent !== this.#message) view.feedback.textContent = this.#message;
  }

  #renderIdentity(device) {
    const view = this.#view;
    if (view.name) {
      const label = device && this.#host.devices.label(device);
      const name = typeof label === 'string' && label ? label.slice(0, 256) : 'Air conditioner';
      const canOpen = !!device && typeof this.#host.ui?.openDevice === 'function';
      view.nameLabel.textContent = name;
      view.name.disabled = !canOpen;
      view.readout.disabled = !canOpen;
      view.readout.setAttribute('aria-label', `Open controls for ${name}`);
      show(view.openLabel, canOpen);
    }
  }

  #renderDiagnostics({ available, diagnostics: readings }) {
    const { diagnosticsPanel, diagnosticFields } = this.#view;
    show(diagnosticsPanel, available && diagnosticFields.some(({ key }) => readings[key] !== undefined));
    for (const { key, unit, label, output } of diagnosticFields) {
      const value = available ? readings[key] : undefined;
      const visible = value !== undefined;
      show(label, visible);
      show(output, visible);
      output.textContent = this.#format(value, unit);
    }
  }

  #renderModes({ available, mode, activity }, schema) {
    const view = this.#view;
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
  }

  #listenDial() {
    const { dial } = this.#view;
    dial.addEventListener('pointerdown', event => this.#beginDrag(event));
    dial.addEventListener('pointermove', event => this.#moveDrag(event));
    dial.addEventListener('pointerup', event => this.#endDrag(event));
    for (const name of ['pointercancel', 'lostpointercapture']) {
      dial.addEventListener(name, event => {
        if (this.#drag?.pointer !== event.pointerId) return;
        this.#cancelDrag();
        this.#drawTargets();
      });
    }
    dial.addEventListener('keydown', event => this.#dialKey(event));
  }

  #endDrag(event) {
    if (this.#drag?.pointer !== event.pointerId) return;
    this.#moveDrag(event);
    const { control, value } = this.#drag;
    this.#cancelDrag();
    if (Number.isFinite(value) && value !== control.value) void this.#send(control.key, value);
    else this.#drawTargets();
  }

  #dialMarkers(lowSlot) {
    return [[this.#view.low, lowSlot], [this.#view.high, 'cool']];
  }

  #beginDrag(event) {
    const state = this.#dialState;
    const touch = event.pointerType === 'touch';
    if (!state || this.#drag || this.#pending || !event.isPrimary || (!touch && event.button !== 0)) return;
    const point = dialPointer(this.#view.svg, event);
    if (!point) return;
    const handles = this.#dialMarkers(state.lowSlot).map(([marker, slot]) => {
      const control = state.controls[slot];
      const distance = Math.hypot(point.x - Number(marker.getAttribute('cx')),
        point.y - Number(marker.getAttribute('cy')));
      return { marker, slot, control, distance };
    });
    const nearest = handles.reduce((closest, candidate) => {
      if (!candidate.control) return closest;
      return !closest?.control || candidate.distance < closest.distance ? candidate : closest;
    }, undefined);
    if (!nearest?.control) return;
    // A touch can grab the arc in a 44 px band, even away from the target dot.
    const inGrabArea = touch
      ? point.angle <= dialGeometry.sweep && Math.abs(point.radius - dialGeometry.radius) * point.pixelsPerUnit <= 22
      : nearest.distance <= 34;
    if (!inGrabArea) return;
    event.preventDefault();
    if (!touch) nearest.marker.focus({ preventScroll: true });
    const { control, slot } = nearest;
    this.#drag = { pointer: event.pointerId, slot, control, value: control.value,
      startX: event.clientX, startY: event.clientY, moved: false };
    this.#view.dial.setPointerCapture(event.pointerId);
    this.#view.dial.toggleAttribute('data-dragging', true);
  }

  #moveDrag(event) {
    const drag = this.#drag;
    if (drag?.pointer !== event.pointerId) return;
    if (!drag.moved && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 3) return;
    drag.moved = true;
    const point = dialPointer(this.#view.svg, event);
    if (!point || point.radius < 15 || point.angle > dialGeometry.sweep) return;
    const { min, max } = this.#dialState.scale;
    const target = min + (max - min) * point.angle / dialGeometry.sweep;
    const value = snapTarget(drag.control, target);
    if (!Number.isFinite(value) || value === drag.value) return;
    drag.value = value;
    this.#drawTargets();
  }

  #cancelDrag() {
    if (!this.#drag) return;
    const { pointer } = this.#drag;
    this.#drag = undefined;
    const { dial } = this.#view;
    dial.toggleAttribute('data-dragging', false);
    if (dial.hasPointerCapture(pointer)) dial.releasePointerCapture(pointer);
  }

  #dialKey(event) {
    if (event.key === 'Escape' && this.#drag) {
      event.preventDefault();
      this.#cancelDrag();
      this.#drawTargets();
      return;
    }
    const marker = event.target;
    const control = this.#dialState?.controls[marker.dataset?.targetSlider];
    if (!control || this.#pending || this.#drag) return;
    const direction = lookup({ ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1 }, event.key);
    const edge = lookup({ Home: control.min, End: control.max }, event.key);
    if (direction === undefined && edge === undefined) return;
    event.preventDefault();
    const value = direction === undefined ? edge : nextTarget(control, direction);
    if (value !== undefined && value !== control.value) void this.#send(control.key, value);
  }

  #renderTargets(model, schema) {
    const { available, targets, gap, room } = model;
    const view = this.#view;
    const controls = {};
    const scale = { ...defaultScale };
    for (const { row, buttons } of view.targets) {
      const slot = row.dataset.target;
      const target = targets[slot];
      const visible = available && target?.value !== undefined;
      const spec = schema?.[target?.key];
      show(row, visible && (slot !== 'single' || spec?.writable === true));
      const control = visible ? targetControl(target, spec, model) : undefined;
      if (control) {
        controls[slot] = control;
        scale.min = Math.min(scale.min, spec.min);
        scale.max = Math.max(scale.max, spec.max);
      }
      for (const button of buttons) {
        const value = nextTarget(control, Number(button.dataset.step));
        show(button, spec?.writable === true);
        button.disabled = value === undefined || !!this.#pending;
        if (!button.disabled) this.#commands.set(button, [target.key, value]);
      }
    }
    this.#dialState = { room: available ? room : undefined, targets: available ? targets : {},
      restingTarget: available && model.mode === 'off' ? model.targetsByKey.setpoint : undefined,
      scale, controls, lowSlot: targets.heat ? 'heat' : 'single' };
    if (this.#drag) {
      const active = controls[this.#drag.slot];
      const original = this.#drag.control;
      if (!active || ['key', 'value', 'min', 'max', 'origin', 'step'].some(field => active[field] !== original[field])) {
        this.#cancelDrag();
      }
    }
    const editable = Object.keys(controls).length > 0;
    view.dial.toggleAttribute('data-editable', editable && !this.#pending);
    view.svg.setAttribute('aria-hidden', String(!editable));
    view.track.setAttribute('d', arc(scale.min, scale.max, scale));
    show(view.gap, available && !!(controls.heat || controls.cool) && !!targets.heat && gap > 0 &&
      targets.cool.value - targets.heat.value <= gap);
    view.gap.textContent = 'Minimum temperature gap reached';
    this.#drawTargets();
  }

  #drawTargets() {
    const { targets, room, restingTarget, scale, controls, lowSlot } = this.#dialState;
    const view = this.#view;
    const value = slot => this.#drag?.slot === slot ? this.#drag.value : targets[slot]?.value;
    const low = value(lowSlot);
    const high = value('cool');
    const faceTarget = lowSlot === 'single' ? low ?? restingTarget : undefined;
    const hasTarget = faceTarget !== undefined;
    const rangeStart = high === undefined ? scale.min : low;
    const rangeEnd = high ?? faceTarget;
    view.range.setAttribute('d', rangeEnd > rangeStart ? arc(rangeStart, rangeEnd, scale) : '');
    view.dial.toggleAttribute('data-has-target', hasTarget);
    view.dialTarget.textContent = this.#format(faceTarget);
    show(view.dialTarget, hasTarget);
    view.roomLabel.textContent = hasTarget ? 'Now' : 'Room temp';
    if (view.name) {
      view.readout.setAttribute('aria-describedby', hasTarget
        ? 'ac-dial-activity ac-dial-target ac-dial-room' : 'ac-dial-activity ac-dial-room');
    }
    mark(view.room, room, scale, 82);
    mark(view.low, low, scale);
    const sharedPoint = high === low && high !== undefined;
    mark(view.high, sharedPoint && !controls.cool ? undefined : high, scale,
      sharedPoint ? dialGeometry.radius + 12 : undefined);

    for (const [marker, slot] of this.#dialMarkers(lowSlot)) {
      const control = controls[slot];
      marker.setAttribute('aria-hidden', String(!control));
      if (!control) {
        marker.removeAttribute('role');
        marker.removeAttribute('tabindex');
        continue;
      }
      marker.dataset.targetSlider = slot;
      marker.setAttribute('role', 'slider');
      marker.setAttribute('tabindex', '0');
      marker.setAttribute('aria-label', dialLabels[slot]);
      marker.setAttribute('aria-valuemin', String(control.min));
      marker.setAttribute('aria-valuemax', String(control.max));
      marker.setAttribute('aria-valuenow', String(Math.max(control.min, Math.min(control.max, value(slot)))));
      marker.setAttribute('aria-valuetext', this.#format(value(slot)));
      marker.setAttribute('aria-disabled', String(!!this.#pending));
    }
    for (const { row, output } of view.targets) {
      if (output) output.textContent = this.#format(value(row.dataset.target));
    }
  }

  #renderAirflow({ available, mode, reported, unsupportedAirflow }, schema) {
    let hasAirflow = false;
    let canAdjustAirflow = false;
    for (const field of this.#view.fields) {
      const { key, row, select, reading, values } = field;
      const spec = schema?.[key];
      const supported = !unsupportedAirflow.has(key);
      const writable = supported && spec?.writable && spec.options.length > 0;
      const raw = reported[key];
      const visible = supported && (writable || (raw !== undefined && raw !== 'unknown'));
      show(row, visible);
      hasAirflow ||= visible;
      show(select, writable);
      show(reading, !writable);
      reading.textContent = available ? spec?.options.find(option => option.value === raw)?.label ??
        lookup(values, raw) ?? raw ?? '—' : '—';
      if (!writable) continue;
      updateAirflowOptions(field, spec.options, raw);
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
    this.#cancelDrag();
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
if (!customElements.get('hc-ac-panel')) customElements.define('hc-ac-panel', class extends AcCard {});
