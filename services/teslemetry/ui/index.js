// homeCore plugin UI host interface v1. The browser owns rendering and live data.
const powerwallStyle = `
  :host { display:block; min-width:0; color:var(--ink,#20242b); font-family:var(--font,system-ui,sans-serif); }
  * { box-sizing:border-box; }
  button { width:100%; min-height:148px; padding:14px; display:flex; flex-direction:column; gap:10px;
    text-align:left; font:inherit; color:inherit; background:var(--surface,#fff);
    border:1px solid var(--card-line,#dedede); border-radius:var(--radius,14px); cursor:pointer; }
  button:focus-visible { outline:2px solid var(--accent,#9a4a07); outline-offset:3px; }
  button:disabled { cursor:default; }
  .heading,.compact-reading { display:flex; justify-content:space-between; align-items:baseline; gap:10px; width:100%; min-width:0; }
  .name { font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .details { flex:none; font-size:12px; color:var(--ink-muted,#626870); }
  .number { font-family:var(--font-num,monospace); font-variant-numeric:tabular-nums; }
  .label,.status,.caption { font-size:12px; color:var(--ink-muted,#626870); }
  .diagram { width:100%; height:auto; display:block; overflow:visible; }
  .diagram text { text-anchor:middle; fill:var(--ink,#20242b); font:12px var(--font,system-ui,sans-serif); }
  .diagram .number { font-family:var(--font-num,monospace); font-size:15px; font-weight:600; }
  .diagram .charge { font-size:24px; }
  .diagram .direction { fill:var(--ink-muted,#626870); font-size:10px; }
  .node { fill:var(--surface,#fff); stroke:var(--card-line,#dedede); }
  .battery-node { fill:var(--surface-2,#eeeae0); }
  .connection { stroke:var(--card-line,#dedede); stroke-width:2; fill:none; }
  .flow { stroke:var(--accent,#9a4a07); stroke-width:2; fill:none; stroke-dasharray:4 7; animation:power-flow 1.8s linear infinite; }
  .arrow { fill:var(--accent,#9a4a07); }
  .bus { fill:var(--surface-2,#eeeae0); stroke:var(--card-line,#dedede); }
  .caption { text-align:center; width:100%; font-size:11px; }
  .status { min-height:16px; font-size:12px; }
  .compact-reading { display:none; }
  .compact { min-height:72px; padding:10px 12px; gap:4px; }
  .compact .diagram,.compact .caption { display:none; }
  .compact .compact-reading { display:flex; flex-wrap:wrap; font-size:13px; }
  .compact .compact-charge { font-size:19px; font-weight:600; }
  .compact .flow { animation:none; }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0; }
  @keyframes power-flow { to { stroke-dashoffset:-22; } }
  @media (prefers-reduced-motion:reduce) { .flow { animation:none; } }
  [hidden] { display:none !important; }
`;

// Each branch is the net reading at the common bus, not a source-to-load allocation.
const powerwallConnections = [
  { key: 'solar', field: 'solar_power_w', label: 'Solar', positive: 'Generating', negative: 'Consuming',
    paths: ['M170 73V132', 'M170 132V73'] },
  { key: 'grid', field: 'grid_power_w', label: 'Grid', positive: 'Importing', negative: 'Exporting',
    paths: ['M126 148H155', 'M155 148H126'] },
  { key: 'home', field: 'home_power_w', label: 'Home', positive: 'Using', negative: 'Supplying',
    paths: ['M185 148H214', 'M214 148H185'] },
  { key: 'battery', field: 'battery_power_w', label: 'Battery', positive: 'Discharging', negative: 'Charging',
    paths: ['M170 222V164', 'M170 164V222'] }
];

class TeslemetryCard extends HTMLElement {
  #host;
  #deviceId;
  #form = 'standard';
  #stopDevices;
  #stopUnits;
  #view;

  static accepts(device) {
    return device?.device_id === 'teslemetry_powerwall';
  }

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
    this.#deviceId = typeof id === 'string' ? id : undefined;
    this.#render();
  }

  set form(form) {
    this.#form = ['compact', 'standard', 'large'].includes(form) ? form : 'standard';
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
    // Only the fixed template is HTML; all device data is assigned as text.
    this.shadowRoot.innerHTML = `<style>${powerwallStyle}</style>
      <button type="button" class="hc-tile" aria-describedby="readings status">
        <span class="heading"><span class="name"></span><span class="details">Details ›</span></span>
        <svg class="diagram" viewBox="0 0 340 308" aria-hidden="true">
          <defs><marker id="flow-arrow" markerWidth="4" markerHeight="4" refX="3" refY="2" orient="auto"><path class="arrow" d="M0 0L4 2L0 4Z"/></marker></defs>
          <path class="connection" d="M170 70V226M124 148H216"/>
          <path class="flow" data-flow="solar" marker-end="url(#flow-arrow)"/>
          <path class="flow" data-flow="grid" marker-end="url(#flow-arrow)"/>
          <path class="flow" data-flow="home" marker-end="url(#flow-arrow)"/>
          <path class="flow" data-flow="battery" marker-end="url(#flow-arrow)"/>
          <circle class="bus" cx="170" cy="148" r="12"/>
          <g data-node="solar"><rect class="node" x="108" y="2" width="124" height="68" rx="12"/>
            <text x="170" y="21">Solar</text><text class="value number" x="170" y="42"></text><text class="direction" x="170" y="58"></text></g>
          <g data-node="grid"><rect class="node" x="2" y="113" width="122" height="70" rx="12"/>
            <text x="63" y="133">Grid</text><text class="value number" x="63" y="154"></text><text class="direction" x="63" y="172"></text></g>
          <g data-node="home"><rect class="node" x="216" y="113" width="122" height="70" rx="12"/>
            <text x="277" y="133">Home</text><text class="value number" x="277" y="154"></text><text class="direction" x="277" y="172"></text></g>
          <g data-node="battery"><rect class="node battery-node" x="92" y="226" width="156" height="80" rx="12"/>
            <text x="170" y="245">Powerwall</text><text class="charge number" x="170" y="272"></text><text class="direction" x="170" y="293"></text></g>
        </svg>
        <span class="compact-reading" aria-hidden="true">
          <span><span class="compact-charge number">—</span><span class="label"> charge</span></span>
          <span class="compact-power"></span>
        </span>
        <span class="caption">Net power at each connection</span>
        <span id="readings" class="sr-only"></span>
        <span id="status" class="status" aria-live="polite"></span>
      </button>`;
    const find = selector => this.shadowRoot.querySelector(selector);
    this.#view = {
      button: find('button'), name: find('.name'), charge: find('.charge'), status: find('.status'),
      compactCharge: find('.compact-charge'), compactPower: find('.compact-power'), summary: find('#readings'),
      nodes: powerwallConnections.map(connection => ({ ...connection,
        flow: find(`[data-flow="${connection.key}"]`),
        value: find(`[data-node="${connection.key}"] .value`),
        direction: find(`[data-node="${connection.key}"] .direction`)
      }))
    };
    this.#view.button.addEventListener('click', () => {
      if (this.#deviceId && this.#host?.devices.get(this.#deviceId)) {
        this.#host.ui.openDevice(this.#deviceId);
      }
    });
  }

  #listen() {
    this.#stop();
    const sheet = this.#host?.theme?.sheet;
    this.shadowRoot.adoptedStyleSheets = sheet instanceof CSSStyleSheet ? [sheet] : [];
    if (this.#host) {
      this.#stopDevices = this.#host.devices.subscribe(() => this.#render());
      this.#stopUnits = this.#host.units?.subscribe(() => this.#render());
    }
    this.#render();
  }

  #stop() {
    this.#stopDevices?.();
    this.#stopUnits?.();
    this.#stopDevices = undefined;
    this.#stopUnits = undefined;
  }

  #format(value, unit) {
    if (value === undefined) return '—';
    const shown = this.#host?.units?.show(value, unit)?.text;
    return typeof shown === 'string' ? shown : `${value}${unit === '%' ? '' : ' '}${unit}`;
  }

  #render() {
    if (!this.#view) return;
    const device = this.#deviceId ? this.#host?.devices.get(this.#deviceId) : undefined;
    const available = device?.available === true;
    const attributes = device?.attributes;
    const number = field => {
      const value = attributes?.[field];
      return available && typeof value === 'number' && Number.isFinite(value) ? value : undefined;
    };
    const rawCharge = number('charge_pct');
    const charge = rawCharge >= 0 && rawCharge <= 100 ? rawCharge : undefined;
    const label = device ? this.#host.devices.label(device) : 'Powerwall';
    const name = typeof label === 'string' ? label.slice(0, 256) : 'Powerwall';
    const view = this.#view;
    view.button.classList.toggle('compact', this.#form === 'compact');
    view.button.disabled = !device;
    view.button.setAttribute('aria-label', `Open ${name} details`);
    view.name.textContent = name;
    view.charge.textContent = this.#format(charge, '%');
    view.compactCharge.textContent = view.charge.textContent;
    const summary = [`Battery charge ${charge === undefined ? 'unavailable' : view.charge.textContent}.`];
    let known = charge === undefined ? 0 : 1;
    for (const node of view.nodes) {
      const watts = number(node.field);
      const magnitude = watts === undefined ? undefined : Math.abs(watts) / 1000;
      const kw = magnitude === undefined ? undefined : Number(magnitude.toFixed(magnitude < 0.1 ? 3 : 2));
      const power = this.#format(kw, 'kW');
      const direction = watts === undefined ? 'No reading'
        : watts === 0 ? 'Idle' : watts > 0 ? node.positive : node.negative;
      const description = watts === undefined ? 'No reading' : `${direction} · ${power}`;
      if (node.value) node.value.textContent = power;
      node.direction.textContent = node.key === 'battery' ? description : direction;
      node.flow.toggleAttribute('hidden', watts === undefined || watts === 0);
      node.flow.setAttribute('d', node.paths[watts < 0 ? 1 : 0]);
      if (node.key === 'battery') view.compactPower.textContent = description;
      if (watts !== undefined) known += 1;
      summary.push(`${node.label}: ${description}.`);
    }
    view.summary.textContent = summary.join(' ');
    view.status.textContent = !device ? 'Waiting for device'
      : !available ? 'Unavailable'
      : known === 0 ? 'Waiting for readings'
      : known < 5 ? 'Some readings unavailable' : 'Live readings';
  }
}

customElements.define('hc-teslemetry-card', TeslemetryCard);

class TeslemetrySolar extends HTMLElement {
  #host; #stop; #details = false; #history; #generation = 0;
  constructor() { super(); this.attachShadow({ mode: 'open' }); }
  set host(host) { this.#stop?.(); this.#host = host; if (this.isConnected) this.#listen(); }
  connectedCallback() { this.#listen(); }
  disconnectedCallback() { this.#generation++; this.#stop?.(); this.#stop = undefined; }
  #listen() {
    this.#stop?.();
    this.#stop = this.#host?.devices.subscribe(() => this.#render());
    this.#render();
  }
  async #showHistory() {
    this.#details = true;
    this.#history = undefined;
    const generation = ++this.#generation;
    this.#render();
    let result;
    try { result = await this.#host?.devices.history('teslemetry_powerwall', 'solar_power_w', { limit: 1000 }); }
    catch { result = undefined; }
    if (generation !== this.#generation || !this.isConnected) return;
    this.#history = result?.ok && Array.isArray(result.data) ? result.data.slice(0, 1000) : null;
    this.#render();
  }
  #render() {
    const device = this.#host?.devices.get('teslemetry_powerwall');
    const watts = device?.available === true ? device.attributes?.solar_power_w : undefined;
    const reading = typeof watts === 'number' && Number.isFinite(watts) ? `${Number((watts / 1000).toFixed(2))} kW` : 'Unavailable';
    this.shadowRoot.innerHTML = '<style>:host{display:block;font:inherit;color:var(--ink,#20242b)}.card{padding:16px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}strong{display:block;font-size:22px;margin:8px 0}button{font:inherit;cursor:pointer}svg{width:100%;height:100px}polyline{stroke:var(--accent,#9a4a07);stroke-width:2;fill:none;vector-effect:non-scaling-stroke}</style><div class="card"><span>Solar</span><strong></strong><div class="history"></div><button type="button"></button></div>';
    this.shadowRoot.querySelector('strong').textContent = reading;
    const button = this.shadowRoot.querySelector('button');
    button.textContent = this.#details ? 'Back to solar' : 'Solar details';
    button.addEventListener('click', () => {
      if (this.#details) { this.#generation++; this.#details = false; this.#render(); }
      else void this.#showHistory();
    });
    if (!this.#details) return;
    const target = this.shadowRoot.querySelector('.history');
    const points = (this.#history ?? []).filter(p => typeof p?.value === 'number' && Number.isFinite(p.value) && Number.isFinite(Date.parse(p.at))).sort((a,b) => Date.parse(a.at) - Date.parse(b.at));
    if (points.length < 2) { target.textContent = this.#history === undefined ? 'Loading history…' : 'Not enough history yet'; return; }
    const values = points.map(p => p.value), min = Math.min(...values), max = Math.max(...values);
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 300 100'); svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', 'Solar power history');
    const line = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
    line.setAttribute('points', values.map((v, i) => `${(Date.parse(points[i].at) - Date.parse(points[0].at)) * 300 / (Date.parse(points.at(-1).at) - Date.parse(points[0].at) || 1)},${90 - (v - min) / (max - min || 1) * 80}`).join(' '));
    svg.append(line); target.append(svg);
  }
}
customElements.define('hc-teslemetry-solar', TeslemetrySolar);

// This is an estimate at the current rate, not a time-of-use bill.
function estimateEnergyCost(energyWh, rate, currency) {
  if (typeof energyWh !== 'number' || !Number.isFinite(energyWh) || energyWh < 0 ||
      typeof rate !== 'number' || !Number.isFinite(rate) ||
      typeof currency !== 'string' || !/^[A-Za-z]{3}$/.test(currency)) return undefined;
  const amount = energyWh / 1000 * rate;
  if (!Number.isFinite(amount)) return undefined;
  return `${currency.toUpperCase()} ${amount.toFixed(2)}`;
}

class TeslemetryGrid extends HTMLElement {
  #host; #stop; #details = false; #history; #generation = 0;
  constructor() { super(); this.attachShadow({ mode: 'open' }); }
  set host(host) { this.#stop?.(); this.#host = host; if (this.isConnected) this.#listen(); }
  connectedCallback() { this.#listen(); }
  disconnectedCallback() { this.#generation++; this.#stop?.(); this.#stop = undefined; }
  #listen() {
    this.#stop?.();
    this.#stop = this.#host?.devices.subscribe(() => this.#render());
    this.#render();
  }
  async #showHistory() {
    this.#details = true; this.#history = undefined;
    const generation = ++this.#generation; this.#render();
    let result;
    try { result = await this.#host?.devices.history('teslemetry_powerwall', 'grid_power_w', {limit:1000}); }
    catch { result = undefined; }
    if (generation !== this.#generation || !this.isConnected) return;
    this.#history = result?.ok && Array.isArray(result.data) ? result.data.slice(0, 1000) : null;
    this.#render();
  }
  #render() {
    const device = this.#host?.devices.get('teslemetry_powerwall');
    const a = device?.available === true ? device.attributes : undefined;
    const watts = a?.grid_power_w;
    const reading = typeof watts === 'number' && Number.isFinite(watts) ? `${Number((watts / 1000).toFixed(2))} kW` : 'Unavailable';
    this.shadowRoot.innerHTML = '<style>:host{display:block;font:inherit;color:var(--ink,#20242b)}.card{padding:16px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}strong{display:block;font-size:22px;margin:8px 0}button{font:inherit;cursor:pointer}svg{width:100%;height:100px}polyline{stroke:var(--accent,#9a4a07);stroke-width:2;fill:none;vector-effect:non-scaling-stroke}.cost{margin:8px 0;font-size:13px}</style><div class="card"><span>Grid</span><strong></strong><div class="history"></div><div class="cost"></div><div class="cost-note"></div><button type="button"></button></div>';
    this.shadowRoot.querySelector('strong').textContent = reading;
    const button = this.shadowRoot.querySelector('button');
    button.textContent = this.#details ? 'Back to grid' : 'Grid details';
    button.addEventListener('click', () => {
      if (this.#details) { this.#generation++; this.#details = false; this.#render(); }
      else void this.#showHistory();
    });
    if (!this.#details) return;
    const cost = a?.tariff_available === true ? estimateEnergyCost(a.grid_energy_imported_wh, a.tariff_buy_price_per_kwh, a.tariff_currency) : undefined;
    this.shadowRoot.querySelector('.cost').textContent = cost ? `Estimated import cost: ${cost}` : 'Import cost unavailable';
    this.shadowRoot.querySelector('.cost-note').textContent = cost ? 'Daily import at the current rate; excludes fees.' : '';
    const target = this.shadowRoot.querySelector('.history');
    const points = (this.#history ?? []).filter(p => typeof p?.value === 'number' && Number.isFinite(p.value) && Number.isFinite(Date.parse(p.at))).sort((a,b) => Date.parse(a.at) - Date.parse(b.at));
    if (points.length < 2) { target.textContent = this.#history === undefined ? 'Loading history…' : 'Not enough history yet'; return; }
    const values = points.map(p => p.value), min = Math.min(...values), max = Math.max(...values);
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox','0 0 300 100'); svg.setAttribute('role','img'); svg.setAttribute('aria-label','Grid power history');
    const line = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
    line.setAttribute('points', values.map((v,i) => `${(Date.parse(points[i].at) - Date.parse(points[0].at)) * 300 / (Date.parse(points.at(-1).at) - Date.parse(points[0].at) || 1)},${90 - (v - min) / (max - min || 1) * 80}`).join(' '));
    svg.append(line); target.append(svg);
  }
}
customElements.define('hc-teslemetry-grid', TeslemetryGrid);

class TeslemetryBattery extends HTMLElement {
  #host; #stop;
  constructor() { super(); this.attachShadow({mode:'open'}); }
  set host(host) { this.#stop?.(); this.#host = host; if (this.isConnected) this.#listen(); }
  connectedCallback() { this.#listen(); }
  disconnectedCallback() { this.#stop?.(); this.#stop = undefined; }
  #listen() { this.#stop?.(); this.#stop = this.#host?.devices.subscribe(() => this.#render()); this.#render(); }
  #render() {
    const d = this.#host?.devices.get('teslemetry_powerwall');
    const charge = d?.available === true ? d.attributes?.charge_pct : undefined;
    const value = typeof charge === 'number' && Number.isFinite(charge) && charge >= 0 && charge <= 100 ? `${charge}%` : 'Unavailable';
    this.shadowRoot.innerHTML = '<style>:host{display:block;font:inherit;color:var(--ink,#20242b)}.card{padding:16px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}strong{display:block;font-size:22px;margin-top:8px}</style><div class="card"><span>Battery</span><strong></strong></div>';
    this.shadowRoot.querySelector('strong').textContent = value;
  }
}
customElements.define('hc-teslemetry-battery', TeslemetryBattery);
