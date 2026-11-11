import { LightningElement, track } from 'lwc';

export default class Events extends LightningElement {
    @track clicks = 0;
    @track heard = 0;
    bound = false;

    connectedCallback() {
        console.debug('dbg-mount');
        console.trace('trace-mount');
    }

    renderedCallback() {
        if (this.bound) return;
        this.bound = true;
        this.template.querySelector('[data-listen]').addEventListener('notify', () => {
            this.heard += 1;
        });
    }

    onClick() {
        this.clicks += 1;
    }
}
