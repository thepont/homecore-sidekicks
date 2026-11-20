class XiaomiSetup extends HTMLElement {
  #host; #generation=0; #timer;
  constructor() { super(); this.attachShadow({mode:'open'}); }
  set host(host) { clearTimeout(this.#timer); this.#generation++; this.#host=host; if(this.isConnected)this.#render(); }
  connectedCallback() { this.#render(); }
  disconnectedCallback() { clearTimeout(this.#timer); this.#generation++; }
  #render() {
    this.shadowRoot.innerHTML='<style>:host{display:block;color:var(--ink,#20242b);font:inherit}.setup{padding:20px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}h2{margin:0 0 8px;font-size:20px}p{line-height:1.5;margin:0 0 20px}label{display:block;margin:0 0 6px;font-weight:600}select{box-sizing:border-box;display:block;width:min(100%,320px);min-height:44px;margin:0 0 16px;padding:8px 12px;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b);font:inherit}button{box-sizing:border-box;min-height:44px;max-width:100%;font:inherit;font-weight:600;padding:10px 16px;cursor:pointer;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b)}button:disabled{opacity:.5;cursor:default}button:focus-visible,select:focus-visible{outline:2px solid var(--accent,#9a4a07);outline-offset:3px}.status{margin:16px 0 0}.status:empty{display:none}img{box-sizing:border-box;display:block;width:min(260px,100%);height:auto;margin-top:16px;background:white;padding:8px}</style><section class="setup"><h2>Connect Xiaomi</h2><p>Sign in with Mi Home to find your gateway.</p><label for="region">Mi Home region</label> <select id="region"><option value="">Choose your Mi Home region</option><option value="cn">Mainland China</option><option value="sg">Singapore</option><option value="de">Europe</option><option value="us">United States</option><option value="i2">India</option><option value="ru">Russia</option></select> <button type="button">Sign in with Mi Home</button><p class="status" aria-live="polite"></p><div class="qr"></div></section>';
    const button=this.shadowRoot.querySelector('button');
    button.disabled=!this.#host?.me?.can('plugins:write');
    button.addEventListener('click',()=>void this.#begin());
    if(!button.disabled)void this.#restore(this.#generation);
  }
  async #restore(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{return;}
    if(generation!==this.#generation || !this.isConnected || !result?.ok)return;
    if(result.data?.setup_status==='gateway_saved') {
      this.shadowRoot.querySelector('button').hidden=true;
      this.shadowRoot.querySelector('.status').textContent='Gateway saved. Connection needs verification.';
    }
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
    const image=document.createElement('img');image.alt='Mi Home sign-in QR code';image.src=`data:image/png;base64,${data}`;
    this.shadowRoot.querySelector('.qr').replaceChildren(image);
    status.textContent='Scan this code with Mi Home.';
    this.#timer=setTimeout(()=>void this.#check(generation),1500);
  }
  async #check(generation) {
    let result;try{result=await this.#host.action('sign_in_status');}catch{result=undefined;}
    if(generation!==this.#generation || !this.isConnected)return;
    const status=this.shadowRoot.querySelector('.status'), button=this.shadowRoot.querySelector('button');
    if(result?.ok && result.data?.setup_status==='gateway_saved') {
      this.shadowRoot.querySelector('.qr').replaceChildren();button.hidden=true;status.textContent='Gateway saved. Connection needs verification.';return;
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
