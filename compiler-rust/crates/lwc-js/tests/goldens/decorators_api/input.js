import { LightningElement, api } from 'lwc';
export default class Test extends LightningElement {
  @api title = 'hi';
  plain = 1;
  _v;
  @api get value() { return this._v; }
  set value(v) { this._v = v; }
  @api focus() {}
}
