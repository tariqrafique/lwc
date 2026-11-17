# private-field-proxy

Shipped @lwc/compiler 9.5.3 lowers `#x` fields in loose mode (`_classPrivateFieldLooseKey` +
`Object.defineProperty(this, _x, { writable: true, value })`, access via `_classPrivateFieldLooseBase(o, _x)[_x]`,
`#x in o` via `Object.prototype.hasOwnProperty.call(_checkInRHS(o), _x)`) for BOTH `enablePrivateMethods`
values. Helpers are module-local (no `babelHelpers` global).

How to check: for each options object in `config.json`, `transformSync(input.js, 'foo.js', opts).code`,
append `driver.mjs`, run as an ES module (`node --input-type=module` or a `.mjs` file). stdout must be
exactly `expected-observation.txt` (`43 true 0`) and exit 0.

Why: native `#x` throws `TypeError: Cannot read private member #x from an object whose class did not declare it`
when `this` is a Proxy; loose keys are ordinary non-enumerable own props, so the Proxy forwards them.

Verified 2026-11-01: shipped 9.5.3 tarballs (both flag values) -> `43 true 0`; Rust lwc-js (both) -> `43 true 0`
(`crates/lwc-js/tests/eval_node.rs::private_fields_work_through_proxy`).
