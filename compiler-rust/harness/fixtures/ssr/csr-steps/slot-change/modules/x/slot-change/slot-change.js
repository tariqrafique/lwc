import { LightningElement, track } from 'lwc';

export default class SlotChange extends LightningElement {
    @track n = 0;

    onSlot() {
        this.n += 1;
    }
}
