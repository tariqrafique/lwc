import { LightningElement, api } from 'lwc';
import PanelA from 'x/panel-a';
import PanelB from 'x/panel-b';

export default class DynamicCtor extends LightningElement {
    @api ctor = PanelA;
    @api alt = PanelB;
}
