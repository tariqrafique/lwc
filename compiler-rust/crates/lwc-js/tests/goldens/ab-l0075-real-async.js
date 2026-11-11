function _asyncToGenerator(n) { function asyncGeneratorStep(n, t, e, r, o, a, c) { try { var i = n[a](c), u = i.value; } catch (n) { return void e(n); } i.done ? t(u) : Promise.resolve(u).then(r, o); } return function () { var t = this, e = arguments; return new Promise(function (r, o) { var a = n.apply(t, e); function _next(n) { asyncGeneratorStep(a, r, o, _next, _throw, "next", n); } function _throw(n) { asyncGeneratorStep(a, r, o, _next, _throw, "throw", n); } _next(void 0); }); }; }
function _classPrivateFieldLooseBase(e, t) { if (!{}.hasOwnProperty.call(e, t)) throw new TypeError("attempted to use private field on non-instance"); return e; }
var id = 0;
function _classPrivateFieldLooseKey(e) { return "__private_" + id++ + "_" + e; }
import _tmpl from "./foo.html";
import { registerComponent as _registerComponent } from "lwc";
var _p = /*#__PURE__*/ _classPrivateFieldLooseKey("p");
class A extends LightningElement {
	constructor(...args) {
		super(...args);
		Object.defineProperty(this, _p, {
			writable: true,
			value: 1
		});
	}
	m() {
		var _this = this;
		return _asyncToGenerator(function* () {
			return _classPrivateFieldLooseBase(_this, _p)[_p];
		})();
	}
	/*LWC compiler v9.5.3*/}
const __lwc_component_class_internal = _registerComponent(A, {
	tmpl: _tmpl,
	sel: "x-foo",
	apiVersion: 66
});
export default __lwc_component_class_internal;
