// Appended after the compiled module (ESM). No `babelHelpers` global may be defined or needed.
;if (typeof babelHelpers !== 'undefined') throw new Error('babelHelpers global');
const p = new Proxy(new C(), {});
p.x = p.x;
console.log(p.x, new C().has(p), Object.keys(new C()).length);
