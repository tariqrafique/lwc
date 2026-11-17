var _Class;
import { LightningElement, registerDecorators as _registerDecorators } from 'lwc';
import { getFoo } from 'x/fooAdapter';
export const helper = n => n * 2;
export default _registerDecorators((_Class = class extends LightningElement {
  constructor(...args) {
    super(...args);
    this.label = 'hi';
    this.state = {
      a: 1
    };
    this.foo = void 0;
  }
  /*LWC compiler v9.5.3*/
}, _Class.flavor = 'x', _Class), {
  publicProps: {
    label: {
      config: 0
    }
  },
  track: {
    state: 1
  },
  wire: {
    foo: {
      adapter: getFoo,
      dynamic: ["id"],
      config: function ($cmp) {
        return {
          id: $cmp.label
        };
      }
    }
  }
});