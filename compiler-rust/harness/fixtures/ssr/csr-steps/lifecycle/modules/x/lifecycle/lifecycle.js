import { LightningElement } from 'lwc';
import tmpl from './lifecycle.html';

export default class Lifecycle extends LightningElement {
    caught = '';

    constructor() {
        super();
    }

    connectedCallback() {}

    render() {
        return tmpl;
    }

    renderedCallback() {}

    disconnectedCallback() {}

    errorCallback(error) {
        this.caught = error && error.message ? error.message : 'error';
    }
}
