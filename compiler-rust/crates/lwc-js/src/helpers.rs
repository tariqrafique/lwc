//! Babel runtime helpers. oxc's transformer has no inline-helper mode, so lowering passes run in
//! External mode (`babelHelpers.x(...)`); [`localize`] then does what babel's inline helpers do in
//! shipped LWC 9.5.3: every `babelHelpers.x` becomes a module-local `_x` (babel `generateUid`, so
//! `objectSpread2` -> `_objectSpread`, collisions -> `_x2`), and module-local declarations with
//! babel-equivalent bodies are prepended for the dependency-closed set the code uses. No global
//! `babelHelpers` is read or written.
const HELPERS: &[(&str, &str)] = &[
    ("defineProperty", "function (e, r, t) { return (r = babelHelpers.toPropertyKey(r)) in e ? Object.defineProperty(e, r, { value: t, enumerable: !0, configurable: !0, writable: !0 }) : e[r] = t, e; }"),
    ("toPrimitive", "function (t, r) { if (\"object\" != typeof t || !t) return t; var e = t[Symbol.toPrimitive]; if (void 0 !== e) { var i = e.call(t, r || \"default\"); if (\"object\" != typeof i) return i; throw new TypeError(\"@@toPrimitive must return a primitive value.\"); } return (\"string\" === r ? String : Number)(t); }"),
    ("toPropertyKey", "function (t) { var i = babelHelpers.toPrimitive(t, \"string\"); return \"symbol\" == typeof i ? i : i + \"\"; }"),
    ("objectSpread2", "function (e) { function o(e, r) { var t = Object.keys(e); if (Object.getOwnPropertySymbols) { var s = Object.getOwnPropertySymbols(e); r && (s = s.filter(function (r) { return Object.getOwnPropertyDescriptor(e, r).enumerable; })), t.push.apply(t, s); } return t; } for (var r = 1; r < arguments.length; r++) { var t = null != arguments[r] ? arguments[r] : {}; r % 2 ? o(Object(t), !0).forEach(function (r) { babelHelpers.defineProperty(e, r, t[r]); }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e, Object.getOwnPropertyDescriptors(t)) : o(Object(t)).forEach(function (r) { Object.defineProperty(e, r, Object.getOwnPropertyDescriptor(t, r)); }); } return e; }"),
    ("objectWithoutPropertiesLoose", "function (r, e) { if (null == r) return {}; var t = {}; for (var n in r) if ({}.hasOwnProperty.call(r, n)) { if (-1 !== e.indexOf(n)) continue; t[n] = r[n]; } return t; }"),
    ("objectWithoutProperties", "function (e, t) { if (null == e) return {}; var o, r, i = babelHelpers.objectWithoutPropertiesLoose(e, t); if (Object.getOwnPropertySymbols) { var n = Object.getOwnPropertySymbols(e); for (r = 0; r < n.length; r++) o = n[r], -1 === t.indexOf(o) && {}.propertyIsEnumerable.call(e, o) && (i[o] = e[o]); } return i; }"),
    ("objectDestructuringEmpty", "function (t) { if (null == t) throw new TypeError(\"Cannot destructure \" + t); }"),
    ("extends", "function () { return Object.assign.apply(Object, arguments); }"),
    ("asyncToGenerator", "function (n) { function asyncGeneratorStep(n, t, e, r, o, a, c) { try { var i = n[a](c), u = i.value; } catch (n) { return void e(n); } i.done ? t(u) : Promise.resolve(u).then(r, o); } return function () { var t = this, e = arguments; return new Promise(function (r, o) { var a = n.apply(t, e); function _next(n) { asyncGeneratorStep(a, r, o, _next, _throw, \"next\", n); } function _throw(n) { asyncGeneratorStep(a, r, o, _next, _throw, \"throw\", n); } _next(void 0); }); }; }"),
    ("OverloadYield", "function (e, d) { this.v = e, this.k = d; }"),
    ("awaitAsyncGenerator", "function (e) { return new babelHelpers.OverloadYield(e, 0); }"),
    ("wrapAsyncGenerator", "(function () { function AsyncGenerator(e) { var t, n; function resume(t, n) { try { var r = e[t](n), o = r.value, u = o instanceof babelHelpers.OverloadYield; Promise.resolve(u ? o.v : o).then(function (n) { if (u) { var i = \"return\" === t && o.k ? t : \"next\"; if (!o.k || n.done) return resume(i, n); n = e[i](n).value; } settle(!!r.done, n); }, function (e) { resume(\"throw\", e); }); } catch (e) { settle(2, e); } } function settle(e, r) { 2 === e ? t.reject(r) : t.resolve({ value: r, done: e }), (t = t.next) ? resume(t.key, t.arg) : n = null; } this._invoke = function (e, r) { return new Promise(function (o, u) { var i = { key: e, arg: r, resolve: o, reject: u, next: null }; n ? n = n.next = i : (t = n = i, resume(e, r)); }); }, \"function\" != typeof e.return && (this.return = void 0); } AsyncGenerator.prototype[\"function\" == typeof Symbol && Symbol.asyncIterator || \"@@asyncIterator\"] = function () { return this; }, AsyncGenerator.prototype.next = function (e) { return this._invoke(\"next\", e); }, AsyncGenerator.prototype.throw = function (e) { return this._invoke(\"throw\", e); }, AsyncGenerator.prototype.return = function (e) { return this._invoke(\"return\", e); }; return function (e) { return function () { return new AsyncGenerator(e.apply(this, arguments)); }; }; })()"),
    ("asyncGeneratorDelegate", "function (t) { var e = {}, n = !1; function pump(e, r) { return n = !0, r = new Promise(function (n) { n(t[e](r)); }), { done: !1, value: new babelHelpers.OverloadYield(r, 1) }; } return e[\"undefined\" != typeof Symbol && Symbol.iterator || \"@@iterator\"] = function () { return this; }, e.next = function (t) { return n ? (n = !1, t) : pump(\"next\", t); }, \"function\" == typeof t.throw && (e.throw = function (t) { if (n) throw n = !1, t; return pump(\"throw\", t); }), \"function\" == typeof t.return && (e.return = function (t) { return n ? (n = !1, t) : pump(\"return\", t); }), e; }"),
    ("classPrivateFieldLooseBase", "function (e, t) { if (!{}.hasOwnProperty.call(e, t)) throw new TypeError(\"attempted to use private field on non-instance\"); return e; }"),
    // `ID` is the module-local counter declared next to it (`var id = 0;`, `_id` on collision)
    ("classPrivateFieldLooseKey", "function (e) { return \"__private_\" + ID++ + \"_\" + e; }"),
    ("checkInRHS", "function (e) { if (Object(e) !== e) throw TypeError(\"right-hand side of 'in' should be an object, got \" + (null !== e ? typeof e : \"null\")); return e; }"),
    ("asyncIterator", "(function () { function AsyncFromSyncIterator(r) { function AsyncFromSyncIteratorContinuation(r) { if (Object(r) !== r) return Promise.reject(new TypeError(r + \" is not an object.\")); var n = r.done; return Promise.resolve(r.value).then(function (r) { return { value: r, done: n }; }); } return AsyncFromSyncIterator = function (r) { this.s = r, this.n = r.next; }, AsyncFromSyncIterator.prototype = { s: null, n: null, next: function () { return AsyncFromSyncIteratorContinuation(this.n.apply(this.s, arguments)); }, return: function (r) { var n = this.s.return; return void 0 === n ? Promise.resolve({ value: r, done: !0 }) : AsyncFromSyncIteratorContinuation(n.apply(this.s, arguments)); }, throw: function (r) { var n = this.s.return; return void 0 === n ? Promise.reject(r) : AsyncFromSyncIteratorContinuation(n.apply(this.s, arguments)); } }, new AsyncFromSyncIterator(r); } return function (r) { var n, t, o, e = 2; for (\"undefined\" != typeof Symbol && (t = Symbol.asyncIterator, o = Symbol.iterator); e--;) { if (t && null != (n = r[t])) return n.call(r); if (o && null != (n = r[o])) return new AsyncFromSyncIterator(n.call(r)); t = \"@@asyncIterator\", o = \"@@iterator\"; } throw new TypeError(\"Object is not async iterable\"); }; })()"),
];

fn refs(text: &str) -> impl Iterator<Item = &'static str> + '_ {
    HELPERS.iter().map(|(n, _)| *n).filter(move |n| {
        let pat = format!("babelHelpers.{n}");
        text.match_indices(&pat).any(|(i, _)| !text[i + pat.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$'))
    })
}

/// Prelude of module-local helper declarations plus the (`babelHelpers` name -> local name) map,
/// or `None` when the program uses no helper. Call sites in printed output are rewritten by
/// [`post_codegen_edits`] (oxc transformer nodes often have a 0-span, so we cannot remap them).
pub fn localize(program: &oxc::ast::ast::Program<'_>) -> Option<(String, Vec<(&'static str, String)>)> {
    use oxc::ast::ast::{Expression, StaticMemberExpression};
    use oxc::ast_visit::{walk, Visit};
    struct C { hits: Vec<(u32, u32, &'static str)> }
    impl<'a> Visit<'a> for C {
        fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
            if let Expression::Identifier(o) = &it.object {
                if o.name == "babelHelpers" {
                    if let Some((n, _)) = HELPERS.iter().find(|(n, _)| *n == it.property.name.as_str()) {
                        self.hits.push((it.span.start, it.span.end, n));
                    }
                }
            }
            walk::walk_static_member_expression(self, it);
        }
    }
    let mut c = C { hits: vec![] };
    c.visit_program(program);
    if c.hits.is_empty() { return None; }
    let mut used: Vec<&str> = Vec::new();
    for (_, _, n) in &c.hits { if !used.contains(n) { used.push(n); } }
    let mut i = 0;
    while i < used.len() {
        let body = HELPERS.iter().find(|(n, _)| *n == used[i]).unwrap().1;
        for d in refs(body) { if !used.contains(&d) { used.push(d); } }
        i += 1;
    }
    let mut uids = crate::transforms::hoist::Uids::new(program);
    let order: Vec<&str> = HELPERS.iter().map(|(n, _)| *n).filter(|n| used.contains(n)).collect();
    let locals: Vec<(&str, String)> = order.iter().map(|n| (*n, uids.generate(n))).collect();
    let mut pre = String::new();
    for (n, l) in &locals {
        let mut body = HELPERS.iter().find(|(h, _)| h == n).unwrap().1.to_string();
        body = subst(&body, &|d| locals.iter().find(|(h, _)| *h == d).map(|(_, l)| l.clone()));
        if *n == "classPrivateFieldLooseKey" {
            let id = uids.reserve("id");
            body = body.replace("ID", &id);
            pre += &format!("var {id} = 0;\n");
        }
        match body.strip_prefix("function ") {
            Some(rest) => pre += &format!("function {l}{rest}\n"),
            None => pre += &format!("var {l} = {body};\n"),
        }
    }
    Some((pre, locals))
}

/// Replace every `babelHelpers.<ident>` in `text` with `f(ident)` (left alone when `f` gives None).
fn subst(text: &str, f: &dyn Fn(&str) -> Option<String>) -> String {
    const P: &str = "babelHelpers.";
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(P) {
        out += &rest[..i];
        let after = &rest[i + P.len()..];
        let n = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).unwrap_or(after.len());
        match f(&after[..n]) {
            Some(l) => out += &l,
            None => { out += P; out += &after[..n]; }
        }
        rest = &after[n..];
    }
    out + rest
}

/// Version-stamp + helper-localize edits in `input` coordinates, remapped onto `output` via oxc's
/// codegen map (D-0262: no re-parse of printed JS). Prelude inserts stay at 0.
/// SHIM(D-0262 P1 post_codegen_edits, expires D-0265): find+replace babelHelpers.* on printed codegen output (D-0264 residual 2).
pub fn post_codegen_edits(
    program: &oxc::ast::ast::Program<'_>,
    input: &str,
    output: &str,
    map: &oxc_sourcemap::SourceMap<'_>,
    filename: &str,
) -> Result<Vec<lwc_shared::edits::Edit>, lwc_shared::CompileError> {
    let mut stamps = Vec::new();
    crate::transforms::version_stamp::run(program, &mut stamps);
    if input != output && !stamps.is_empty() {
        let pts = codegen_points(map, input, output);
        stamps = stamps.into_iter().map(|e| remap_via_map(&pts, e, filename)).collect::<Result<Vec<_>, _>>()?;
    }
    if let Some((pre, locals)) = localize(program) {
        for (name, local) in &locals {
            let pat = format!("babelHelpers.{name}");
            let mut i = 0;
            while let Some(rel) = output[i..].find(&pat) {
                let at = i + rel;
                let after = at + pat.len();
                if output[after..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                    i = after;
                    continue;
                }
                stamps.push(lwc_shared::edits::Edit::replace(at as u32, after as u32, local.clone()));
                i = after;
            }
        }
        stamps.push(lwc_shared::edits::Edit::insert(0, pre));
    }
    Ok(stamps)
}

/// Remap edits through a subsequent span-edit (kept text shifts; inserts at 0 stay at 0).
pub fn remap_edits(edits: Vec<lwc_shared::edits::Edit>, pieces: &[lwc_shared::edits::Piece]) -> Vec<lwc_shared::edits::Edit> {
    edits.into_iter().map(|e| {
        if e.start == 0 && e.end == 0 { return e; }
        lwc_shared::edits::Edit {
            start: forward_piece(pieces, e.start).unwrap_or_else(|| panic!("post-codegen edit start {} left the kept text", e.start)),
            end: forward_piece(pieces, e.end).unwrap_or_else(|| panic!("post-codegen edit end {} left the kept text", e.end)),
            text: e.text,
        }
    }).collect()
}

fn codegen_points(map: &oxc_sourcemap::SourceMap<'_>, input: &str, output: &str) -> Vec<(u32, u32)> {
    let (li, lo) = (lwc_shared::LineIndex::new(input), lwc_shared::LineIndex::new(output));
    let mut pts: Vec<(u32, u32)> = map.get_tokens().filter_map(|t| {
        Some((li.to_offset(t.get_src_line(), t.get_src_col())?, lo.to_offset(t.get_dst_line(), t.get_dst_col())?))
    }).collect();
    pts.sort_by_key(|p| p.0);
    pts.dedup_by_key(|p| p.0);
    assert!(!pts.is_empty(), "codegen map produced no tokens");
    pts
}

fn map_gen(pts: &[(u32, u32)], off: u32) -> Result<u32, u32> {
    match pts.binary_search_by_key(&off, |p| p.0) {
        Ok(i) => Ok(pts[i].1),
        Err(_) => Err(off),
    }
}

fn remap_via_map(
    pts: &[(u32, u32)],
    e: lwc_shared::edits::Edit,
    filename: &str,
) -> Result<lwc_shared::edits::Edit, lwc_shared::CompileError> {
    let miss = |off| lwc_shared::CompileError::new(
        format!("internal: codegen map has no token at source offset {off}"),
        filename,
    );
    Ok(lwc_shared::edits::Edit {
        start: map_gen(pts, e.start).map_err(miss)?,
        end: map_gen(pts, e.end).map_err(miss)?,
        text: e.text,
    })
}

fn forward_piece(p: &[lwc_shared::edits::Piece], i: u32) -> Option<u32> {
    let idx = p.partition_point(|x| x.in_start <= i);
    let x = p[..idx].iter().rev().find(|x| x.kept)?;
    let len = x.out_end - x.out_start;
    (i <= x.in_start + len).then(|| x.out_start + (i - x.in_start))
}

#[cfg(test)]
mod map_gen_tests {
    #[test]
    fn map_gen_requires_an_exact_source_offset_hit() {
        let pts = [(10, 20), (15, 26)];
        assert_eq!(super::map_gen(&pts, 10), Ok(20));
        assert_eq!(super::map_gen(&pts, 15), Ok(26));
        assert_eq!(super::map_gen(&pts, 12), Err(12), "no extrapolation");
        assert_eq!(super::map_gen(&pts, 0), Err(0), "no fallback to pts[0]");
        assert_eq!(super::map_gen(&pts, 99), Err(99));
    }

    #[test]
    fn unmapped_offset_is_an_internal_error() {
        let pts = [(10, 20), (15, 26)];
        let e = lwc_shared::edits::Edit { start: 12, end: 12, text: "/*x*/".into() };
        let err = super::remap_via_map(&pts, e, "t.js").expect_err("unmapped offset must not extrapolate");
        assert!(err.message.contains("internal: codegen map has no token at source offset 12"), "{err:?}");
        assert_eq!(err.filename.as_deref(), Some("t.js"));
    }
}
