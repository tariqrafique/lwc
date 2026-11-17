import { LightningElement, api, track, wire } from 'lwc';
import { getFoo } from 'x/fooAdapter';
export const helper = (n) => n * 2;
export default class Foo extends LightningElement {
  @api label = 'hi';
  @api get size() { return this._size; }
  set size(v) { this._size = v; }
  @track state = { a: 1 };
  @wire(getFoo, { id: '$label' }) foo;
  @api reset() { this.state = { a: 0 }; }
  static flavor = 'x';
}
