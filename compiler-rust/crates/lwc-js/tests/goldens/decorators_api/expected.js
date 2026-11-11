import { registerDecorators as _registerDecorators } from "lwc";
import _tmpl from "./test.html";
import { registerComponent as _registerComponent } from "lwc";
import { LightningElement } from "lwc";
class Test extends LightningElement {
	constructor(...args) {
		super(...args);
		this.title = "hi";
		this.plain = 1;
		this._v = void 0;
	}
	get value() {
		return this._v;
	}
	set value(v) {
		this._v = v;
	}
	focus() {}
	/*LWC compiler v9.5.3*/}
_registerDecorators(Test, {
	publicProps: {
		title: { config: 0 },
		value: { config: 3 }
	},
	publicMethods: ["focus"],
	fields: ["plain", "_v"]
});
const __lwc_component_class_internal = _registerComponent(Test, {
	tmpl: _tmpl,
	sel: "lwc-test",
	apiVersion: 66
});
export default __lwc_component_class_internal;
