class XiaomiSetup extends HTMLElement {
  #host; #generation=0; #timer;
  constructor() { super(); this.attachShadow({mode:'open'}); }
  set host(host) { clearTimeout(this.#timer); this.#generation++; this.#host=host; if(this.isConnected)this.#render(); }
  connectedCallback() { this.#render(); }
  disconnectedCallback() { clearTimeout(this.#timer); this.#generation++; }
  #render() {
    this.shadowRoot.innerHTML='<style>:host{display:block;color:var(--ink,#20242b);font:inherit}.setup{padding:20px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}h2{margin:0 0 8px;font-size:20px}p{line-height:1.5;margin:0 0 20px}label{display:block;margin:0 0 6px;font-weight:600}select{box-sizing:border-box;display:block;width:min(100%,320px);min-height:44px;margin:0 0 16px;padding:8px 12px;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b);font:inherit}button{box-sizing:border-box;min-height:44px;max-width:100%;font:inherit;font-weight:600;padding:10px 16px;cursor:pointer;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b)}button:disabled{opacity:.5;cursor:default}button:focus-visible,select:focus-visible{outline:2px solid var(--accent,#9a4a07);outline-offset:3px}.status{margin:16px 0 0}.status:empty{display:none}ul{padding-inline-start:20px;margin:16px 0 0}li{margin-block:8px;overflow-wrap:anywhere}li small{display:block;color:var(--muted,#57616c)}.access-actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:8px}.access-feedback:empty{display:none}img{box-sizing:border-box;display:block;width:min(260px,100%);height:auto;margin-top:16px;background:white;padding:8px}</style><section class="setup"><h2>Connect Xiaomi</h2><p>Sign in with Mi Home to find your Xiaomi devices.</p><label for="region">Mi Home region</label> <select id="region"><option value="">Choose your Mi Home region</option><option value="cn">Mainland China</option><option value="sg">Singapore</option><option value="de">Europe</option><option value="us">United States</option><option value="i2">India</option><option value="ru">Russia</option></select> <button type="button">Sign in with Mi Home</button><p class="status" aria-live="polite"></p><div class="qr"></div><ul class="devices" aria-label="Discovered Xiaomi devices" hidden></ul><section class="access" hidden><h3 id="requests-heading">Xiaomi access requests</h3><ul aria-labelledby="requests-heading"></ul></section><section class="known" hidden><h3 id="known-heading">Saved Xiaomi devices</h3><ul aria-labelledby="known-heading"></ul></section><p class="access-feedback" role="status"></p></section>';
    const button=this.shadowRoot.querySelector('button');
    button.disabled=!this.#host?.me?.can('plugins:write');
    button.addEventListener('click',()=>void this.#begin());
    if(!button.disabled)void this.#restore(this.#generation);
  }
  async #restore(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{return;}
    if(generation!==this.#generation || !this.isConnected || !result?.ok)return;
    this.#showAccess(result.data);
    const qr=result.data?.setup_status==='waiting' ? this.#qrImage(result.data.qr_png) : null;
    if(qr) {
      this.#showQr(qr,generation);
      return;
    }
    if(result.data?.setup_status==='gateway_saved') {
      this.shadowRoot.querySelector('button').textContent='Sign in again';
      this.shadowRoot.querySelector('.status').textContent='Gateway saved. Connection needs verification.';
    }
    if(result.data?.setup_status==='devices_found') this.#showDevices(result.data.devices);
  }
  #qrImage(qr) {
    if(typeof qr!=='string' || qr.length>262144 || !qr.startsWith('iVBORw0KGgo') || !/^[A-Za-z0-9+/]+={0,2}$/.test(qr)) return null;
    const image=document.createElement('img');
    image.alt='Mi Home sign-in QR code';
    image.src=`data:image/png;base64,${qr}`;
    return image;
  }
  #showQr(image,generation) {
    this.shadowRoot.querySelector('.qr').replaceChildren(image);
    const button=this.shadowRoot.querySelector('button');
    if(!button.disabled) button.disabled=true;
    this.shadowRoot.querySelector('.status').textContent='Scan this code with Mi Home.';
    this.#timer=setTimeout(()=>void this.#check(generation),1500);
  }
  #showAccess(data) {
    const devices=Array.isArray(data?.known_devices) ? data.known_devices.filter(device=>
      typeof device?.device_ref==='string' && typeof device.name==='string' &&
      typeof device.model==='string' && Array.isArray(device.adapters)) : [];
    this.#showRequests(data?.access_requests,devices);
    this.#showKnownDevices(devices);
  }
  #showRequests(requests,devices) {
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
      const actions=document.createElement('div');
      actions.className='access-actions';
      for(const device of devices) {
        if(device.model!==request.model || !device.adapters.includes(request.plugin_id)) continue;
        actions.append(this.#accessButton('Allow '+device.name,'approve_access',
          {plugin_id:request.plugin_id,device_ref:device.device_ref}));
      }
      actions.append(this.#accessButton('Deny','deny_access',
        {plugin_id:request.plugin_id,model:request.model}));
      row.append(actions);
      list.append(row);
    }
    section.hidden=list.children.length===0;
  }
  #showKnownDevices(devices) {
    const section=this.shadowRoot.querySelector('.known');
    const list=section.querySelector('ul');
    list.replaceChildren();
    for(const device of devices) {
      const row=document.createElement('li');
      const name=document.createElement('strong');
      name.textContent=device.name;
      row.append(name);
      const actions=document.createElement('div');
      actions.className='access-actions';
      for(const adapter of device.adapters) {
        if(typeof adapter!=='string') continue;
        const details=document.createElement('small');
        const granted=Array.isArray(device.granted_adapters) && device.granted_adapters.includes(adapter);
        details.textContent=adapter+(granted?' · Access approved':' · Eligible');
        row.append(details);
        if(granted) actions.append(this.#accessButton('Revoke access for '+adapter,'revoke_access',
          {plugin_id:adapter,device_ref:device.device_ref}));
      }
      if(actions.children.length) row.append(actions);
      list.append(row);
    }
    section.hidden=list.children.length===0;
  }
  #accessButton(label,action,params) {
    const button=document.createElement('button');
    button.type='button';
    button.textContent=label;
    button.addEventListener('click',()=>void this.#changeAccess(button,action,params));
    return button;
  }
  async #changeAccess(button,action,params) {
    const generation=this.#generation,feedback=this.shadowRoot.querySelector('.access-feedback');
    button.disabled=true;
    let result;
    try { result=await this.#host.action(action,params); } catch { result=undefined; }
    if(generation!==this.#generation || !this.isConnected) return;
    button.disabled=false;
    if(!result?.ok || result.data?.status!=='ok') {
      feedback.textContent='Could not update Xiaomi access. Try again.';
      return;
    }
    feedback.textContent={
      approve_access:'Access approved.',
      deny_access:'Request denied.',
      revoke_access:'Access revoked.',
    }[action];
    try { result=await this.#host.action('sign_in_status'); } catch { return; }
    if(generation===this.#generation && this.isConnected && result?.ok) this.#showAccess(result.data);
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
    const qr=result?.ok ? this.#qrImage(result.data?.qr_png) : null;
    if(!qr) {
      status.textContent='Could not start sign-in. Try again.';button.disabled=false;return;
    }
    this.shadowRoot.querySelector('.devices').hidden=true;
    this.#showQr(qr,generation);
  }
  async #check(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{result=undefined;}
    if(generation!==this.#generation || !this.isConnected)return;
    if(result?.ok) this.#showAccess(result.data);
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
