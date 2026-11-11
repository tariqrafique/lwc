import { LightningElement, api } from 'lwc';

let bornSeq = 0;

export default class Row extends LightningElement {
    @api value;
    born = ++bornSeq;

    connectedCallback() {}
}
