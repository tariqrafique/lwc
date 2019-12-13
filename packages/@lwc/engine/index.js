const { LightningElement, createElement } = require('./dist/engine.cjs');

class Test extends LightningElement {
    constructor() {
        super();
        console.log(this.getAttribute('foo'));
    }
}

const elm = createElement('x-test', { is: Test });
