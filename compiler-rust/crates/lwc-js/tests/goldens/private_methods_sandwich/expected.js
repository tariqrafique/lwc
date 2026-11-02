import { registerDecorators as _registerDecorators } from "lwc";
import _tmpl from "./test.html";
import { registerComponent as _registerComponent } from "lwc";
import { LightningElement } from "lwc";
class Test extends LightningElement {
	constructor(...args) {
		super(...args);
		this.value = 1;
	}
	#helper(x) {
		return x + this.value;
	}
	async #load() {
		return this.#helper(2);
	}
	connectedCallback() {
		const f = this.#helper;
		this.#load().then(() => f.call(this, 3));
	}
	/*LWC compiler v9.5.3*/}
_registerDecorators(Test, { fields: ["value"] });
const __lwc_component_class_internal = _registerComponent(Test, {
	tmpl: _tmpl,
	sel: "lwc-test",
	apiVersion: 66,
	enablePrivateMethods: true
});
export default __lwc_component_class_internal;
