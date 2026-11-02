import { LightningElement } from 'lwc';
class Plain { a = 1; ['b' + 1] = 2; static s = 3; #p = 4; }
export default class Test extends LightningElement {
  count = 0;
  'quoted' = 'q';
  inner = class { z = 9; };
  constructor() {
    super();
    this.init();
  }
  init() {}
}
