var id = 0;
function _classPrivateFieldLooseKey(e) { return "__private_" + id++ + "_" + e; }
import { registerDecorators as _registerDecorators } from "lwc";
import _tmpl from "./test.html";
import { registerComponent as _registerComponent } from "lwc";
import { LightningElement } from "lwc";
var _p = /*#__PURE__*/ _classPrivateFieldLooseKey("p");
class Plain {
	constructor() {
		this.a = 1;
		this["b" + 1] = 2;
		Object.defineProperty(this, _p, {
			writable: true,
			value: 4
		});
	}
}
Plain.s = 3;
class Test extends LightningElement {
	constructor() {
		super();
		this.count = 0;
		this["quoted"] = "q";
		this.inner = class {
			constructor() {
				this.z = 9;
			}
		};
		this.init();
	}
	init() {}
	/*LWC compiler v9.5.3*/}
_registerDecorators(Test, { fields: ["count", "inner"] });
const __lwc_component_class_internal = _registerComponent(Test, {
	tmpl: _tmpl,
	sel: "lwc-test",
	apiVersion: 66
});
export default __lwc_component_class_internal;
