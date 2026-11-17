import { LightningElement, wire } from 'lwc';
import { getRecord } from 'lightning/uiRecordApi';
export default class Test extends LightningElement {
  @wire(getRecord, { recordId: '$recordId', fields: ['Account.Name'], deep: '$a.b.c', odd: '$a.b-c' })
  record;
  @wire(getRecord, {}) handler(r) {}
}
