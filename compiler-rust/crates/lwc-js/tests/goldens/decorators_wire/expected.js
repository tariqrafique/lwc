import { registerDecorators as _registerDecorators } from "lwc";
import _tmpl from "./test.html";
import { registerComponent as _registerComponent } from "lwc";
import { LightningElement } from "lwc";
import { getRecord } from "lightning/uiRecordApi";
class Test extends LightningElement {
	constructor(...args) {
		super(...args);
		this.record = void 0;
	}
	handler(r) {}
	/*LWC compiler v9.5.3*/}
_registerDecorators(Test, { wire: {
	record: {
		adapter: getRecord,
		dynamic: [
			"recordId",
			"deep",
			"odd"
		],
		config: function($cmp) {
			let v1 = $cmp.a;
			let v2 = $cmp["a"];
			return {
				fields: ["Account.Name"],
				recordId: $cmp.recordId,
				deep: v1 != null && (v1 = v1.b) != null ? v1.c : undefined,
				odd: v2 != null ? v2["b-c"] : undefined
			};
		}
	},
	handler: {
		adapter: getRecord,
		dynamic: [],
		method: 1,
		config: function($cmp) {
			return {};
		}
	}
} });
const __lwc_component_class_internal = _registerComponent(Test, {
	tmpl: _tmpl,
	sel: "lwc-test",
	apiVersion: 66
});
export default __lwc_component_class_internal;
