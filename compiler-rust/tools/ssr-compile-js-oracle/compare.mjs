// Shared extractors and the token-stream compare for run.mjs and the self-test prove scripts.
// tokens() compares string literals by cooked value (D-0196, D-0257) and keeps token types distinct
// (template vs string, identifier vs quoted vs computed keys, shorthand vs longhand).
export const ABSENT_NAMES = ['props', 'internals', 'sigs', 'reg'];

export function makeCompare(acorn) {
  const tokens = (s) => { /*RULE:token-types*/
    return [...acorn.tokenizer(s, { ecmaVersion: 'latest', sourceType: 'module' })]
      .map((t) => (t.value === undefined ? t.type.label : `${t.type.label}:${JSON.stringify(t.value)}`)).join(' ');
  };

  const walk = (node, fn) => {
    if (!node || typeof node !== 'object') return;
    fn(node);
    for (const k of Object.keys(node)) {
      const v = node[k];
      if (Array.isArray(v)) for (const c of v) walk(c, fn);
      else if (v && typeof v.type === 'string') walk(v, fn);
    }
  };
  const parseModule = (code) => {
    try {
      return acorn.parse(code, { ecmaVersion: 'latest', sourceType: 'module' });
    } catch (e) {
      const err = new Error(`cannot parse compiler output: ${e.message}`);
      err.extractMissing = true;
      throw err;
    }
  };
  const slice = (code, node) => code.slice(node.start, node.end);
  const missing = (msg) => {
    const e = new Error(msg);
    e.extractMissing = true;
    throw e;
  };
  const absent = (c, name) => Array.isArray(c?.expectAbsent) && c.expectAbsent.includes(name);
  const validateAbsent = (c) => {
    const list = c?.expectAbsent;
    if (list == null) return;
    if (!Array.isArray(list)) missing('expectAbsent must be an array of names');
    for (const n of list) {
      if (/*RULE:absent-unknown*/ !ABSENT_NAMES.includes(n)) {
        missing(`unknown expectAbsent name ${JSON.stringify(n)}; allowed: ${ABSENT_NAMES.join(', ')}`);
      }
    }
  };
  const calleeName = (node) => {
    if (node.type !== 'CallExpression') return null;
    return node.callee.type === 'Identifier' ? node.callee.name : null;
  };
  const callsNamed = (ast, name) => {
    const out = [];
    walk(ast, (n) => { if (calleeName(n) === name) out.push(n); });
    return out;
  };
  // A located callee must be unique: more than one match is no-verdict, not a join or a silent first hit.
  const uniqueCall = (ast, name) => {
    const calls = callsNamed(ast, name);
    if (/*RULE:extract-dup*/ calls.length > 1) missing(`duplicate ${name} call`);
    return calls[0] || null;
  };
  const requireNode = (node, c, name, label) => {
    const declared = absent(c, name);
    if (!node && /*RULE:extract-missing*/ !declared) missing(`missing ${label}`);
    if (node && /*RULE:absent-present*/ declared) {
      missing(`expectAbsent "${name}" is stale or false: ${label} is present`);
    }
    return node;
  };
  const outside = (code) => code.replace(/import[^;]*;/g, '');

  const extractors = {
    // The public-property and __registerPublicProperties arrays, as the AST source slice of the unique
    // call's argument (handles nested arrays; the old /[^\]]*/ regex truncated at the first `]`).
    // Compared byte-exact (D-0256). Absence is allowed only via expectAbsent (closed set).
    props: (code, c) => {
      validateAbsent(c);
      const ast = parseModule(code);
      const siCall = uniqueCall(ast, '__setStaticInternals');
      const siArg = siCall?.arguments[2];
      requireNode(siArg ? siCall : null, c, 'props', '__setStaticInternals public-property array');
      const props = siArg ? /*RULE:extract-nested*/ slice(code, siArg) : '';
      const regCall = uniqueCall(ast, '__registerPublicProperties');
      const regArg = regCall?.arguments[1];
      requireNode(regArg ? regCall : null, c, 'reg', '__registerPublicProperties array');
      const reg = regArg ? slice(code, regArg) : '';
      return `props=${props} reg=${reg}${/@(api|track)\b/.test(outside(code)) ? ' DECORATOR-LEFT' : ''}`;
    },
    wire: (code, c) => {
      validateAbsent(c);
      const ast = parseModule(code);
      const si = uniqueCall(ast, '__setStaticInternals');
      requireNode(si, c, 'internals', '__setStaticInternals call');
      const call = si ? tokens(slice(code, si)) : '';
      const fields = [...code.matchAll(/^\s*(static\s+)?(g|s|p|m)\b[^\n]*$/gm)].map((m) => m[0].trim()).join(' | ');
      const chains = [...code.matchAll(/instance(?:\["(?:[^"\\\n]|\\.)*"\])+/g)].map((m) => m[0]);
      return `${call}${/@wire/.test(outside(code)) ? ' DECORATOR-LEFT' : ''} :: ${fields} :: ${JSON.stringify(chains)}`;
    },
    members: (code, c) => {
      validateAbsent(c);
      const sigs = [...code.matchAll(/(?:static\s+)?\[?constructor\]?\s*\(([^)]*)\)\s*\{|super\(([^)]*)\)|["']?\b(connectedCallback|renderedCallback|disconnectedCallback|errorCallback)\b["']?\s*\(/g)]
        .map((m) => tokens(m[0].replace(/\s*\{$/, '')));
      requireNode(sigs.length ? true : null, c, 'sigs', 'constructor, super(...) or lifecycle member');
      return sigs.join(' | ');
    },
    markup: tokens,
    dynamic: tokens,
  };

  return { tokens, extractors };
}
