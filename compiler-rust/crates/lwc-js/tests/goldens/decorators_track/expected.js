import { registerDecorators as _registerDecorators } from "lwc";
import _tmpl from "./test.html";
import { registerComponent as _registerComponent } from "lwc";
import { LightningElement } from "lwc";
class Test extends LightningElement {
	constructor(...args) {
		super(...args);
		this.state = { a: 1 };
	}
	/*LWC compiler v9.5.3*/}
_registerDecorators(Test, { track: { state: 1 } });
const __lwc_component_class_internal = _registerComponent(Test, {
	tmpl: _tmpl,
	sel: "lwc-test",
	apiVersion: 66
});
export default __lwc_component_class_internal;
