const MAX_DISCOVERED_ACCESSORIES = 100;
const MAX_ACCESSORY_NAME_LENGTH = 128;

function validAccessory(item) {
  return typeof item?.id === 'string' && /^([0-9a-f]{2}:){5}[0-9a-f]{2}$/i.test(item.id)
    && typeof item.name === 'string' && item.name.length <= MAX_ACCESSORY_NAME_LENGTH
    && typeof item.paired === 'boolean';
}

function discoveredAccessories(result) {
  return result?.ok && Array.isArray(result.data?.accessories) ? result.data.accessories : null;
}

function pairingRequiresRecovery(result) {
  return result?.ok && result.data?.status === 'recovery_required';
}

class HomeKitSetup extends HTMLElement {
  #host;
  #generation=0;
  #recovery=false;
  constructor(){super();this.attachShadow({mode:'open'});}
  set host(value){this.#host=value;this.#generation++;if(this.isConnected)this.#render();}
  connectedCallback(){this.#render();}
  disconnectedCallback(){this.#generation++;}
  #render(){
    this.shadowRoot.innerHTML=`<style>
      :host{display:block;color:var(--ink,#20242b);font:inherit}
      section{padding:20px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}
      h2{margin:0 0 8px;font-size:20px}p{line-height:1.5;margin:0 0 16px}
      label{display:block;margin:16px 0 6px;font-weight:600}
      select,input,button{box-sizing:border-box;min-height:44px;max-width:100%;padding:9px 12px;border:1px solid var(--card-line,#dedede);border-radius:8px;background:var(--surface-2,#f5f5f5);color:var(--ink,#20242b);font:inherit}
      select,input{display:block;width:min(100%,320px)}button{cursor:pointer;font-weight:600;margin-top:16px}button:disabled{opacity:.5;cursor:default}
      :focus-visible{outline:2px solid var(--accent,#9a4a07);outline-offset:3px}.status,.paired{margin:16px 0 0}.status:empty,.paired:empty{display:none}
    </style><section><h2>Connect a HomeKit accessory</h2><p>Find an accessory on your network and enter its HomeKit setup code.</p>
      <button type="button" id="discover">Find accessories</button><p class="paired" aria-live="polite"></p>
      <label for="accessory">Accessory</label><select id="accessory"><option value="">Choose an accessory</option></select>
      <label for="pin">HomeKit setup code</label><input id="pin" type="password" inputmode="numeric" autocomplete="off" placeholder="123-45-678" maxlength="10">
      <button type="button" id="connect" disabled>Connect</button><p class="status" aria-live="polite"></p></section>`;
    const permitted=!!this.#host?.me?.can('plugins:write');
    this.shadowRoot.querySelector('#discover').disabled=!permitted;
    this.shadowRoot.querySelector('#discover').addEventListener('click',()=>void this.#discover());
    this.shadowRoot.querySelector('#accessory').addEventListener('change',()=>this.#update());
    this.shadowRoot.querySelector('#connect').addEventListener('click',()=>void this.#connect());
  }
  #update(){this.shadowRoot.querySelector('#connect').disabled=this.#recovery || !this.#host?.me?.can('plugins:write') || !this.shadowRoot.querySelector('#accessory').value;}
  async #discover(){
    if(!this.#host?.me?.can('plugins:write'))return;
    const generation=++this.#generation,button=this.shadowRoot.querySelector('#discover'),status=this.shadowRoot.querySelector('.status');
    button.disabled=true;status.textContent='Looking for accessories…';
    let result;try{result=await this.#host.action('discover_accessories');}catch{}
    if(generation!==this.#generation || !this.isConnected)return;
    button.disabled=false;
    const list=discoveredAccessories(result);
    if(!list){status.textContent='Could not find accessories. Try again.';return;}
    const select=this.shadowRoot.querySelector('#accessory');
    select.replaceChildren(new Option('Choose an accessory',''));
    const paired=[];
    for(const item of list.slice(0,MAX_DISCOVERED_ACCESSORIES)){
      if(!validAccessory(item))continue;
      if(item.paired){paired.push(`${item.name} is already paired.`);continue;}
      select.add(new Option(item.name,item.id));
    }
    this.shadowRoot.querySelector('.paired').textContent=paired.join(' ');
    status.textContent=select.options.length>1?'Choose an accessory to connect.':'No unpaired accessories found.';
    this.#update();
  }
  async #connect(){
    if(!this.#host?.me?.can('plugins:write'))return;
    const id=this.shadowRoot.querySelector('#accessory').value,pin=this.shadowRoot.querySelector('#pin'),status=this.shadowRoot.querySelector('.status');
    if(!id||!/^\d{3}-\d{2}-\d{3}$/.test(pin.value)){status.textContent='Enter a valid HomeKit setup code.';return;}
    const generation=++this.#generation,button=this.shadowRoot.querySelector('#connect');
    button.disabled=true;status.textContent='Connecting…';
    let result;try{result=await this.#host.action('pair_accessory',{id,pin:pin.value});}catch{}
    pin.value='';
    if(generation!==this.#generation || !this.isConnected)return;
    if(pairingRequiresRecovery(result)){
      this.#recovery=true;status.textContent='Pairing could not be confirmed. The accessory may need recovery before another attempt.';
    } else {
      status.textContent=result?.ok&&result.data?.status==='ok'?'Accessory connected.':'Could not connect the accessory. Check its setup code and try again.';
    }
    this.#update();
  }
}
customElements.define('hc-homekit-setup',HomeKitSetup);
