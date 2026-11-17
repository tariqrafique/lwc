import { LightningElement } from 'lwc';

export default class Boom extends LightningElement {
    connectedCallback() {
        throw new Error('boom');
    }
}
