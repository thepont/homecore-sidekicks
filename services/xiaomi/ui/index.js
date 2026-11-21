class XiaomiSetup extends HTMLElement {
  #host; #generation=0; #timer;
  constructor() { super(); this.attachShadow({mode:'open'}); }
  set host(host) { clearTimeout(this.#timer); this.#generation++; this.#host=host; if(this.isConnected)this.#render(); }
  connectedCallback() { this.#render(); }
  disconnectedCallback() { clearTimeout(this.#timer); this.#generation++; }
  #render() {
    this.shadowRoot.innerHTML='<style>:host{display:block;color:var(--ink,#20242b);font:inherit}.setup{padding:20px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}h2{margin:0 0 8px;font-size:20px}p{line-height:1.5;margin:0 0 20px}label{display:block;margin:0 0 6px;font-weight:600}select{box-sizing:border-box;display:block;width:min(100%,320px);min-height:44px;margin:0 0 16px;padding:8px 12px;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b);font:inherit}button{box-sizing:border-box;min-height:44px;max-width:100%;font:inherit;font-weight:600;padding:10px 16px;cursor:pointer;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b)}button:disabled{opacity:.5;cursor:default}button:focus-visible,select:focus-visible{outline:2px solid var(--accent,#9a4a07);outline-offset:3px}.status{margin:16px 0 0}.status:empty{display:none}ul{padding-inline-start:20px;margin:16px 0 0}li{margin-block:8px;overflow-wrap:anywhere}li small{display:block;color:var(--muted,#57616c)}img{box-sizing:border-box;display:block;width:min(260px,100%);height:auto;margin-top:16px;background:white;padding:8px}</style><section class="setup"><h2>Connect Xiaomi</h2><p>Sign in with Mi Home to find your Xiaomi devices.</p><label for="region">Mi Home region</label> <select id="region"><option value="">Choose your Mi Home region</option><option value="cn">Mainland China</option><option value="sg">Singapore</option><option value="de">Europe</option><option value="us">United States</option><option value="i2">India</option><option value="ru">Russia</option></select> <button type="button">Sign in with Mi Home</button><p class="status" aria-live="polite"></p><div class="qr"></div><ul class="devices" aria-label="Discovered Xiaomi devices" hidden></ul><section class="access" hidden><h3 id="requests-heading">Xiaomi access requests</h3><ul aria-labelledby="requests-heading"></ul></section></section>';
    const button=this.shadowRoot.querySelector('button');
    button.disabled=!this.#host?.me?.can('plugins:write');
    button.addEventListener('click',()=>void this.#begin());
    if(!button.disabled)void this.#restore(this.#generation);
  }
  async #restore(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{return;}
    if(generation!==this.#generation || !this.isConnected || !result?.ok)return;
    this.#showRequests(result.data?.access_requests);
    const qr=result.data?.qr_png;
    if(result.data?.setup_status==='waiting' && typeof qr==='string' && qr.length<=262144 && qr.startsWith('iVBORw0KGgo') && /^[A-Za-z0-9+/]+={0,2}$/.test(qr)) {
      const image=document.createElement('img');image.alt='Mi Home sign-in QR code';image.src=`data:image/png;base64,${qr}`;
      this.shadowRoot.querySelector('.qr').replaceChildren(image);
      this.shadowRoot.querySelector('button').disabled=true;
      this.shadowRoot.querySelector('.status').textContent='Scan this code with Mi Home.';
      this.#timer=setTimeout(()=>void this.#check(generation),1500);return;
    }
    if(result.data?.setup_status==='gateway_saved') {
      this.shadowRoot.querySelector('button').textContent='Sign in again';
      this.shadowRoot.querySelector('.status').textContent='Gateway saved. Connection needs verification.';
    }
    if(result.data?.setup_status==='devices_found') this.#showDevices(result.data.devices);
  }
  #showRequests(requests) {
    const section=this.shadowRoot.querySelector('.access');
    const list=section.querySelector('ul');
    list.replaceChildren();
    for(const request of Array.isArray(requests) ? requests : []) {
      if(request?.status!=='pending' || typeof request.plugin_id!=='string' || typeof request.model!=='string') continue;
      const row=document.createElement('li');
      const plugin=document.createElement('strong');
      const model=document.createElement('small');
      const status=document.createElement('small');
      plugin.textContent=request.plugin_id;
      model.textContent=request.model;
      status.textContent='Awaiting approval';
      row.append(plugin,model,status);
      list.append(row);
    }
    section.hidden=list.children.length===0;
  }
  #showDevices(devices) {
    const list=this.shadowRoot.querySelector('.devices');
    list.replaceChildren();
    for(const device of Array.isArray(devices) ? devices : []) {
      if(typeof device?.name!=='string' || !Array.isArray(device.adapters)) continue;
      const row=document.createElement('li');
      const name=document.createElement('strong');
      name.textContent=device.name;
      row.append(name);
      for(const pluginId of device.adapters) {
        if(typeof pluginId!=='string') continue;
        const adapter=document.createElement('small');
        adapter.textContent=pluginId;
        row.append(adapter);
      }
      list.append(row);
    }
    list.hidden=list.children.length===0;
    this.shadowRoot.querySelector('.qr').replaceChildren();
    const button=this.shadowRoot.querySelector('button');
    button.disabled=false;
    button.textContent='Sign in again';
    this.shadowRoot.querySelector('.status').textContent=list.hidden
      ? 'No devices matched the registered Xiaomi adapters.'
      : 'Matching devices found.';
  }
  async #begin() {
    if(!this.#host?.me?.can('plugins:write'))return;
    if(!this.shadowRoot.querySelector('#region').value) {
      this.shadowRoot.querySelector('.status').textContent='Choose your Mi Home region first.';
      this.shadowRoot.querySelector('#region').focus();return;
    }
    const generation=++this.#generation, button=this.shadowRoot.querySelector('button'),status=this.shadowRoot.querySelector('.status');
    button.disabled=true;status.textContent='Preparing sign-in…';
    let result;try{result=await this.#host.action('begin_sign_in',{region:this.shadowRoot.querySelector('#region').value});}catch{result=undefined;}
    if(generation!==this.#generation || !this.isConnected)return;
    const data=result?.ok ? result.data?.qr_png : undefined;
    if(typeof data!=='string' || data.length>262144 || !data.startsWith('iVBORw0KGgo') || !/^[A-Za-z0-9+/]+={0,2}$/.test(data)) {
      status.textContent='Could not start sign-in. Try again.';button.disabled=false;return;
    }
    this.shadowRoot.querySelector('.devices').hidden=true;
    const image=document.createElement('img');image.alt='Mi Home sign-in QR code';image.src=`data:image/png;base64,${data}`;
    this.shadowRoot.querySelector('.qr').replaceChildren(image);
    status.textContent='Scan this code with Mi Home.';
    this.#timer=setTimeout(()=>void this.#check(generation),1500);
  }
  async #check(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{result=undefined;}
    if(generation!==this.#generation || !this.isConnected)return;
    if(result?.ok) this.#showRequests(result.data?.access_requests);
    const status=this.shadowRoot.querySelector('.status'), button=this.shadowRoot.querySelector('button');
    if(result?.ok && result.data?.setup_status==='devices_found') {
      this.#showDevices(result.data.devices);
      return;
    }
    if(result?.ok && result.data?.setup_status==='gateway_saved') {
      this.shadowRoot.querySelector('.qr').replaceChildren();button.disabled=false;button.textContent='Sign in again';status.textContent='Gateway saved. Connection needs verification.';return;
    }
    if(result?.ok && result.data?.setup_status==='connected') {
      this.shadowRoot.querySelector('.qr').replaceChildren();button.hidden=true;status.textContent='Xiaomi connected';return;
    }
    if(result?.ok && result.data?.setup_status==='waiting') {
      this.#timer=setTimeout(()=>void this.#check(generation),1500);return;
    }
    this.shadowRoot.querySelector('.qr').replaceChildren();button.disabled=false;
    status.textContent='Sign-in could not be completed. Try again.';
  }
}
customElements.define('hc-xiaomi-setup',XiaomiSetup);
