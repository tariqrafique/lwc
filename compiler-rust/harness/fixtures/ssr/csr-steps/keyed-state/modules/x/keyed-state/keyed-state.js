import { LightningElement, api, track } from 'lwc';

export default class KeyedState extends LightningElement {
    @api items;
    @track note = 'seed';
}
