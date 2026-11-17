import { LightningElement, registerDecorators as _registerDecorators } from 'lwc';
import { getFoo } from 'x/fooAdapter';
export const helper = n => n * 2;
class Foo extends LightningElement {
  constructor(...args) {
    super(...args);
    this.label = 'hi';
    this.state = {
      a: 1
    };
    this.foo = void 0;
  }
  get size() {
    return this._size;
  }
  set size(v) {
    this._size = v;
  }
  reset() {
    this.state = {
      a: 0
    };
  }
  /*LWC compiler v9.5.3*/
}
Foo.flavor = 'x';
_registerDecorators(Foo, {
  publicProps: {
    label: {
      config: 0
    },
    size: {
      config: 3
    }
  },
  publicMethods: ["reset"],
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
export { Foo as default };