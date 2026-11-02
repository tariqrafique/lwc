import { LightningElement } from 'lwc';
export default class Test extends LightningElement {
  value = 1;
  #helper(x) { return x + this.value; }
  async #load() { return this.#helper(2); }
  connectedCallback() { const f = this.#helper; this.#load().then(() => f.call(this, 3)); }
}
