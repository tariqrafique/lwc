// Cluster CSR render-parity failures from a report written by test/csr.render.test.ts (see CSR_RENDER.md).
// Usage: node scripts/csr-render-clusters.mjs .csr-render-out/rust-ssr/report-ssr.json [cluster-to-dump] [n]
// Heuristic: the first matching rule wins; "other" is everything left.
import fs from 'node:fs';
const r=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));const fails=r.rows.filter(x=>!x.pass);
const J=o=>JSON.stringify(o);
const rules=[
 ["vnode", "render throws `Assert Violation: [object Object],… is not a vnode` (for:each/iterator/if child arrays not flattened into the vnode list)", x=>/is not a vnode/.test(J(x.got))],
 ["babel-frame", "JS-transform error text lacks Babel's `SyntaxError: <file>: … + code frame` wrapper (same LWC code/message)", x=>x.got.status==="compile-error"&&x.want.status==="compile-error"&&/^SyntaxError: \//.test(x.want.error)&&!/^SyntaxError/.test(x.got.error)],
 ["diag-format", "template diagnostic emitted without `(line:col)` location and/or `LWCnnnn:` code prefix", x=>x.got.status==="compile-error"&&x.want.status==="compile-error"&&/\(\d+:\d+\)|LWC\d+:/.test(x.want.error)&&!/LWC\d+:/.test(x.got.error)],
 ["warn-format", "compile warnings emitted without `(line:col)` location and `LWCnnnn:` code prefix (otherwise same text)", x=>{const st=a=>J((a||[]).map(w=>w.replace(/ \(\d+:\d+\)/,"").replace(/LWC\d+: /,"")));return J(x.got.warnings)!==J(x.want.warnings)&&st(x.got.warnings)===st(x.want.warnings)}],
 ["warn-count", "compile warnings missing or extra vs JS (e.g. no `Invalid HTML syntax` recovery warning, extra `for is not valid attribute`)", x=>x.got.status!=="compile-error"&&x.want.status!=="compile-error"&&(x.got.warnings||[]).length!==(x.want.warnings||[]).length],
 ["missing-diag", "JS rejects (compile error/warning) but rust compiles cleanly", x=>x.want.status==="compile-error"&&x.got.status!=="compile-error"],
 ["extra-diag", "rust emits a warning/error JS does not (e.g. `formnovalidate is not valid attribute for div`)", x=>x.got.status==="compile-error"&&x.want.status!=="compile-error"],
 ["complex-expr", "complex template expressions (experimentalComplexExpressions) left as literal `{…}` text/attribute instead of compiled", x=>{const c=s=>((s||"").match(/\{[^}<"]*[^\w\s}][^}<"]*\}/g)||[]).length;return c(x.got.html)>c(x.want.html)}],
 ["prop-name", "attribute→property name mapping differs (`noValidate`, `htmlFor`, `hyphenSSpecial`, …) in engine `Unknown public property` warnings", x=>/Unknown public property/.test(J(x.want.logs))&&J(x.got.logs)!==J(x.want.logs)&&x.got.html===x.want.html],
 ["directive-attr", "template directive rendered as a literal DOM attribute (`lwc:spread`, `lwc:on`, `lwc:dom`, `lwc:inner-html`, …) instead of compiled", x=>/ lwc:[\w-]+="/.test(x.got.html||"")&&!/ lwc:[\w-]+="/.test(x.want.html||"")],
 ["slot-name", "`<slot name=…>` name attribute dropped (all slots become default slots)", x=>((x.got.html||"").match(/<slot name=/g)||[]).length<((x.want.html||"").match(/<slot name=/g)||[]).length],
 ["render-mode", "renderMode / template render-mode metadata mismatch (`Light DOM components can't render shadow DOM templates` or reverse)", x=>/can't render (shadow|light) DOM templates|Shadow DOM components template|Light DOM components/.test(J(x.got.logs||[]).concat(J(x.want.logs||[])))&&J(x.got.logs)!==J(x.want.logs)],
 ["dyn-comp", "lwc:component / lwc:is rendered as a literal `<lwc:component>` element (dynamic component not compiled)", x=>/<lwc:component/.test(x.got.html||"")&&!/<lwc:component/.test(x.want.html||"")],
 ["comments", "comment nodes dropped (lwc:preserve-comments or `<!---->` anchors)", x=>{const c=s=>(s||"").match(/<!--/g)?.length||0;return c(x.got.html)<c(x.want.html)}],
 ["attr-order", "same attributes, different order on the element (e.g. `id` before/after `x1…`, aria/global attrs first)", x=>{const at=s=>((s||"").match(/<[^>]+>/g)||[]).map(t=>(t.match(/ [\w:-]+="[^"]*"/g)||[]).sort().join("")).join("|");const sk=s=>(s||"").replace(/ [\w:-]+="[^"]*"/g,"");return x.got.html!==x.want.html&&sk(x.got.html)===sk(x.want.html)&&at(x.got.html)===at(x.want.html)}],
 ["attr-values", "same elements/text, attribute set or values differ (`checked` emitted as attribute instead of property, `spellcheck=\"true\"`, scoped-style class tokens, `class=\"[object Object]\"`, missing `tabindex`/`exportparts`)", x=>{const sk=s=>(s||"").replace(/ [\w:-]+="[^"]*"/g,"");return x.got.html!==x.want.html&&sk(x.got.html)===sk(x.want.html)}],
 ["whitespace", "text whitespace differs (e.g. `Default slot: ` → `Default slot:`)", x=>(x.got.html||"").replace(/\s/g,"")===(x.want.html||"").replace(/\s/g,"")&&x.got.html!==x.want.html],
];
const cl={};for(const x of fails){const rule=rules.find(([,,f])=>{try{return f(x)}catch{return false}});const k=rule?rule[0]:"other";(cl[k]=cl[k]||{d:rule?rule[1]:"other",f:[]}).f.push(x)}
console.log("pass",r.rows.length-fails.length,"fail",fails.length);
for(const [k,v] of Object.entries(cl).sort((a,b)=>b[1].f.length-a[1].f.length))console.log(`\n[${v.f.length}] ${k}: ${v.d}\n   `+v.f.slice(0,10).map(x=>x.fixture).join(", "));
if(process.argv[3]){for(const x of (cl[process.argv[3]]||{f:[]}).f.slice(0,+process.argv[4]||8)){const a=J(x.got),b=J(x.want);let i=0;while(a[i]===b[i])i++;console.log("--",x.fixture,"\n got ",a.slice(Math.max(0,i-80),i+120),"\n want",b.slice(Math.max(0,i-80),i+120))}}
