class XiaomiSetup extends HTMLElement {
  #host;
  constructor() { super(); this.attachShadow({mode:'open'}); }
  set host(host) { this.#host=host; if(this.isConnected)this.#render(); }
  connectedCallback() { this.#render(); }
  #render() {
    this.shadowRoot.innerHTML='<style>:host{display:block;color:var(--ink,#20242b);font:inherit}.setup{padding:20px;border:1px solid var(--card-line,#dedede);border-radius:14px;background:var(--surface,#fff)}h2{margin:0 0 8px;font-size:20px}p{line-height:1.5}button{font:inherit;padding:10px 16px;cursor:pointer;border-radius:8px}button:focus-visible{outline:2px solid var(--accent,#9a4a07);outline-offset:3px}</style><section class="setup"><h2>Connect Xiaomi</h2><p>Sign in with Mi Home to find your gateway.</p><button type="button">Sign in with Mi Home</button></section>';
    this.shadowRoot.querySelector('button').disabled=!this.#host?.me?.can('plugins:write');
  }
}
customElements.define('hc-xiaomi-setup',XiaomiSetup);
