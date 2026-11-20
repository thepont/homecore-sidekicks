// homeCore plugin UI host interface v1. The browser owns rendering and live data.
const powerwallStyle = `
  :host { display:block; min-width:0; container-type:inline-size; color:var(--ink,#20242b); font-family:var(--font,system-ui,sans-serif); }
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
  .diagram text { text-anchor:middle; fill:var(--ink,#20242b); font:14px var(--font,system-ui,sans-serif); }
  .diagram .number { font-family:var(--font-num,monospace); font-size:17px; font-weight:600; }
  .diagram .charge { font-size:24px; }
  .diagram .direction { fill:var(--ink-muted,#626870); font-size:12px; }
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
  .mini-readings { display:none; width:100%; }
  .mini-reading { display:flex; justify-content:space-between; flex-wrap:wrap; gap:3px 8px; font-size:12px; }
  .chargers,.tariffs { width:100%; border-top:1px solid var(--card-line,#dedede); padding-top:10px; }
  .footer-title { display:block; font-size:12px; font-weight:600; margin-bottom:7px; }
  .charger-list { display:flex; flex-direction:column; gap:7px; }
  .charger-row { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr) auto; gap:6px; font-size:12px; align-items:baseline; }
  .charger-state { color:var(--ink-muted,#626870); }
  .charger-power { text-align:right; overflow-wrap:anywhere; }
  .tariff-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; }
  .tariff-buy,.tariff-sell { display:flex; flex-direction:column; gap:3px; min-width:0; }
  .rate { font-size:13px; overflow-wrap:anywhere; }
  .period,.tariff-note { color:var(--ink-muted,#626870); font-size:11px; overflow-wrap:anywhere; }
  .tariff-note { display:block; margin-top:6px; }
  .compact .chargers,.compact .tariffs { padding-top:6px; }
  .compact .footer-title { margin-bottom:4px; }
  @container (max-width:319px) {
    button { min-height:72px; padding:10px 12px; gap:7px; }
    .diagram,.caption { display:none; }
    .compact-reading { display:flex; flex-wrap:wrap; font-size:13px; }
    .compact-charge { font-size:19px; font-weight:600; }
    .mini-readings { display:flex; flex-direction:column; gap:5px; }
    .flow { animation:none; }
  }
  @container (max-width:239px) {
    .tariff-grid { grid-template-columns:1fr; gap:7px; }
    .tariff-buy,.tariff-sell { display:grid; grid-template-columns:30px minmax(0,1fr); gap:2px 8px; }
    .period { grid-column:2; }
    .charger-row { grid-template-columns:1fr auto; }
    .charger-state { grid-column:2; }
  }
  .compact .mini-readings { display:none; }
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
        <span class="mini-readings" aria-hidden="true">
          <span class="mini-reading"><span>Solar</span><span data-reading="solar"></span></span>
          <span class="mini-reading"><span>Home</span><span data-reading="home"></span></span>
          <span class="mini-reading"><span>Grid</span><span data-reading="grid"></span></span>
        </span>
        <span class="caption">Net power at each connection</span>
        <span class="chargers" hidden>
          <span class="footer-title">Home · chargers</span>
          <span class="charger-list"></span>
        </span>
        <span class="tariffs">
          <span class="footer-title">Tariff</span>
          <span class="tariff-grid">
            <span class="tariff-buy"><span class="label">Buy</span><span class="rate number">—</span><span class="period"></span></span>
            <span class="tariff-sell"><span class="label">Sell</span><span class="rate number">—</span><span class="period"></span></span>
          </span>
          <span class="tariff-note"></span>
        </span>
        <span id="readings" class="sr-only"></span>
        <span id="status" class="status" aria-live="polite"></span>
      </button>`;
    const find = selector => this.shadowRoot.querySelector(selector);
    this.#view = {
      button: find('button'), name: find('.name'), charge: find('.charge'), status: find('.status'),
      compactCharge: find('.compact-charge'), compactPower: find('.compact-power'), summary: find('#readings'),
      chargers: find('.chargers'), chargerList: find('.charger-list'), tariffNote: find('.tariff-note'),
      tariffs: ['buy', 'sell'].map(side => ({ side,
        rate: find(`.tariff-${side} .rate`), period: find(`.tariff-${side} .period`)
      })),
      nodes: powerwallConnections.map(connection => ({ ...connection,
        flow: find(`[data-flow="${connection.key}"]`),
        mini: find(`[data-reading="${connection.key}"]`),
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

  #renderChargers(attributes, available) {
    const rows = [];
    const descriptions = [];
    // Match the adapter's bounded connector array; status codes do not identify a car.
    for (let index = 1; index <= 16; index += 1) {
      const prefix = `wall_connector_${index}_`;
      if (!['power_w', 'state', 'fault_state', 'ocpp_status', 'powershare_state']
        .some(suffix => Object.hasOwn(attributes ?? {}, prefix + suffix))) continue;
      const raw = attributes?.[`${prefix}power_w`];
      const watts = available && typeof raw === 'number' && Number.isFinite(raw) ? raw : undefined;
      const power = this.#format(watts === undefined ? undefined : watts / 1000, 'kW');
      const state = !available ? 'Unavailable' : watts === undefined ? 'No reading'
        : watts === 0 ? 'Idle' : watts > 0 ? 'Drawing' : 'Reverse flow';
      const name = `Charger ${index}`;
      const row = document.createElement('span');
      row.className = 'charger-row';
      row.dataset.charger = String(index);
      for (const [className, text] of [
        ['charger-name', name], ['charger-power number', power], ['charger-state', state]
      ]) {
        const part = document.createElement('span');
        part.className = className;
        part.textContent = text;
        row.append(part);
      }
      rows.push(row);
      descriptions.push(`${name}: ${power}, ${state}.`);
    }
    this.#view.chargerList.replaceChildren(...rows);
    this.#view.chargers.hidden = rows.length === 0;
    return descriptions;
  }

  #renderTariffs(attributes, available) {
    const active = available && attributes?.tariff_available === true;
    const rawCurrency = attributes?.tariff_currency;
    const currency = typeof rawCurrency === 'string' && /^[A-Za-z]{3}$/.test(rawCurrency)
      ? rawCurrency.toUpperCase() : undefined;
    const descriptions = [];
    const missing = [];
    for (const tariff of this.#view.tariffs) {
      const raw = attributes?.[`tariff_${tariff.side}_price_per_kwh`];
      const price = active && typeof raw === 'number' && Number.isFinite(raw) ? raw : undefined;
      // Rates are already currency units/kWh. Preserve source precision, including zero and negatives.
      tariff.rate.textContent = price === undefined ? '—' : `${price} ${currency ?? ''}/kWh`;
      const period = attributes?.[`tariff_${tariff.side}_period`];
      tariff.period.textContent = price !== undefined && typeof period === 'string'
        ? period.replaceAll('_', ' ').replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, 80) : '';
      if (price === undefined) missing.push(tariff.side === 'buy' ? 'Buy' : 'Sell');
      descriptions.push(`Tariff ${tariff.side}: ${tariff.rate.textContent} ${tariff.period.textContent}.`);
    }
    const notes = [];
    if (missing.length === 2) notes.push('Tariff unavailable');
    else if (missing.length === 1) notes.push(`${missing[0]} rate unavailable`);
    if (missing.length < 2) {
      if (!currency) notes.push('Currency not supplied');
      else if (attributes?.tariff_currency_source === 'site_timezone') {
        notes.push('Currency inferred from site timezone');
      }
    }
    this.#view.tariffNote.textContent = notes.join(' · ');
    this.#view.tariffNote.hidden = notes.length === 0;
    descriptions.push(...notes);
    return descriptions;
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
      if (node.mini) node.mini.textContent = description;
      if (node.value) node.value.textContent = power;
      node.direction.textContent = node.key === 'battery' ? description : direction;
      node.flow.toggleAttribute('hidden', watts === undefined || watts === 0);
      node.flow.setAttribute('d', node.paths[watts < 0 ? 1 : 0]);
      if (node.key === 'battery') view.compactPower.textContent = description;
      if (watts !== undefined) known += 1;
      summary.push(`${node.label}: ${description}.`);
    }
    summary.push(...this.#renderChargers(attributes, available), ...this.#renderTariffs(attributes, available));
    view.summary.textContent = summary.join(' ');
    view.status.textContent = !device ? 'Waiting for device'
      : !available ? 'Unavailable'
      : known === 0 ? 'Waiting for readings'
      : known < 5 ? 'Some readings unavailable' : 'Live readings';
  }
}

customElements.define('hc-teslemetry-card', TeslemetryCard);
