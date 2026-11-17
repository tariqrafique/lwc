import { LightningElement, api, track, wire } from 'lwc';
import { getFoo } from 'x/fooAdapter';
export const helper = (n) => n * 2;
export default class extends LightningElement {
  @api label = 'hi';
  @track state = { a: 1 };
  @wire(getFoo, { id: '$label' }) foo;
  static flavor = 'x';
}
