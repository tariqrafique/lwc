//! Babel runtime helpers. oxc's transformer has no inline-helper mode, so lowering passes run in
//! External mode (`babelHelpers.x(...)`); [`localize`] then does what babel's inline helpers do in
//! shipped LWC 9.5.3: every helper member becomes a module-local `_x` (babel `generateUid`, so
//! `objectSpread2` -> `_objectSpread`, collisions -> `_x2`), and module-local declarations with
//! babel-equivalent bodies are prepended for the dependency-closed set the code uses. No global
//! `babelHelpers` is read or written.
//!
//! Call sites (including oxc transformer nodes with a 0-span) are rewritten on the Program by
//! AST shape before print (D-0265). Helper bodies are the static templates below; internal
//! helper members and the `ID` identifier are localized from recorded AST spans (verified by
//! `tests/helper_templates.rs`) so the prelude text stays byte-identical without scanning
//! printed JS.
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

/// Source spans of `StaticMemberExpression` nodes whose object is the identifier
/// `babelHelpers` and whose property is a known helper (AST shape, not text search).
/// Verified against a parse of each template in `tests/helper_templates.rs`.
fn helper_members(name: &str) -> &'static [(u32, u32, &'static str)] {
    match name {
        "defineProperty" => &[(33, 59, "toPropertyKey")],
        "toPropertyKey" => &[(23, 47, "toPrimitive")],
        "objectSpread2" => &[(408, 435, "defineProperty")],
        "objectWithoutProperties" => &[(58, 99, "objectWithoutPropertiesLoose")],
        "awaitAsyncGenerator" => &[(26, 52, "OverloadYield")],
        "wrapAsyncGenerator" => &[(131, 157, "OverloadYield")],
        "asyncGeneratorDelegate" => &[(142, 168, "OverloadYield")],
        _ => &[],
    }
}

/// Source spans of the `ID` identifier in `classPrivateFieldLooseKey` (`ID++`).
fn helper_id_spans(name: &str) -> &'static [(u32, u32)] {
    match name {
        "classPrivateFieldLooseKey" => &[(37, 39)],
        _ => &[],
    }
}

fn helper_prop<'a>(it: &oxc::ast::ast::StaticMemberExpression<'a>) -> Option<&'static str> {
    let oxc::ast::ast::Expression::Identifier(o) = &it.object else {
        return None;
    };
    if o.name != "babelHelpers" {
        return None;
    }
    HELPERS
        .iter()
        .find(|(n, _)| *n == it.property.name.as_str())
        .map(|(n, _)| *n)
}

fn collect_used(program: &oxc::ast::ast::Program<'_>) -> Vec<&'static str> {
    use oxc::ast::ast::StaticMemberExpression;
    use oxc::ast_visit::{walk, Visit};
    struct C {
        hits: Vec<&'static str>,
    }
    impl<'a> Visit<'a> for C {
        fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
            if let Some(n) = helper_prop(it) {
                if !self.hits.contains(&n) {
                    self.hits.push(n);
                }
            }
            walk::walk_static_member_expression(self, it);
        }
    }
    let mut c = C { hits: vec![] };
    c.visit_program(program);
    c.hits
}

fn rewrite_call_sites<'a>(
    alloc: &'a oxc::allocator::Allocator,
    program: &mut oxc::ast::ast::Program<'a>,
    locals: &[(&'static str, String)],
) {
    use oxc::ast::ast::Expression;
    use oxc::ast::builder::AstBuilder;
    use oxc::ast_visit::{walk_mut, VisitMut};
    use oxc::span::GetSpan;
    use oxc::str::Str;
    struct R<'a, 'b> {
        ast: AstBuilder<'a>,
        locals: &'b [(&'static str, String)],
    }
    impl<'a> VisitMut<'a> for R<'a, '_> {
        fn visit_expression(&mut self, expr: &mut Expression<'a>) {
            let hit = if let Expression::StaticMemberExpression(m) = &*expr {
                helper_prop(m).and_then(|n| {
                    self.locals
                        .iter()
                        .find(|(h, _)| *h == n)
                        .map(|(_, l)| (expr.span(), l.clone()))
                })
            } else {
                None
            };
            if let Some((span, local)) = hit {
                *expr = Expression::new_identifier(
                    span,
                    Str::from_str_in(&local, &self.ast),
                    &self.ast,
                );
                return;
            }
            walk_mut::walk_expression(self, expr);
        }
    }
    R {
        ast: AstBuilder::new(alloc),
        locals,
    }
    .visit_program(program);
}

fn rewrite_template(
    body: &str,
    name: &str,
    locals: &[(&'static str, String)],
    id: Option<&str>,
) -> String {
    let mut edits: Vec<(u32, u32, String)> = helper_members(name)
        .iter()
        .filter_map(|(s, e, dep)| {
            locals
                .iter()
                .find(|(h, _)| h == dep)
                .map(|(_, l)| (*s, *e, l.clone()))
        })
        .collect();
    if let Some(id) = id {
        for (s, e) in helper_id_spans(name) {
            edits.push((*s, *e, id.to_string()));
        }
    }
    edits.sort_by_key(|(s, _, _)| std::cmp::Reverse(*s));
    let mut out = body.to_string();
    for (s, e, text) in edits {
        out.replace_range(s as usize..e as usize, &text);
    }
    out
}

/// Helpers rewritten on a Program, with the local names assigned to them.
/// Does not include the prelude text (AB-L0075 / D-0265): [`prelude`] emits that
/// once from the union of every localize pass.
#[derive(Clone, Debug, Default)]
pub struct LocalizedHelpers {
    pub locals: Vec<(&'static str, String)>,
    /// `generateUid`-reserved counter name for `classPrivateFieldLooseKey`, if that helper was used.
    pub loose_key_id: Option<String>,
}

fn close_deps(used: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < used.len() {
        for (_, _, d) in helper_members(used[i]) {
            if !used.contains(d) {
                used.push(d);
            }
        }
        i += 1;
    }
}

/// Rewrite helper members on `program` (shape, including 0-span transformer nodes) and
/// return the helpers used with their assigned local names. Does not emit a prelude
/// (AB-L0075 / D-0265): printed JS is never scanned for helper names.
pub fn localize<'a>(
    alloc: &'a oxc::allocator::Allocator,
    program: &mut oxc::ast::ast::Program<'a>,
) -> LocalizedHelpers {
    let mut used = collect_used(program);
    if used.is_empty() {
        return LocalizedHelpers::default();
    }
    close_deps(&mut used);
    let mut uids = crate::transforms::hoist::Uids::new(program);
    let order: Vec<&str> = HELPERS
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| used.contains(n))
        .collect();
    let locals: Vec<(&str, String)> = order.iter().map(|n| (*n, uids.generate(n))).collect();
    rewrite_call_sites(alloc, program, &locals);
    let loose_key_id = if locals
        .iter()
        .any(|(n, _)| *n == "classPrivateFieldLooseKey")
    {
        Some(uids.reserve("id"))
    } else {
        None
    };
    LocalizedHelpers {
        locals,
        loose_key_id,
    }
}

/// Emit one prelude from the union of localize sets, in [`HELPERS`] order.
/// Deps are closed in [`localize`]; this function never mints a name (AB-L0075 r2).
pub fn prelude(sets: &[LocalizedHelpers]) -> Option<String> {
    let mut names: Vec<(&str, String)> = Vec::new();
    let mut loose_key_id = None;
    for s in sets {
        for (n, l) in &s.locals {
            assert!(!l.is_empty(), "internal: helper {n} has no local ident");
            if !names.iter().any(|(h, _)| h == n) {
                names.push((*n, l.clone()));
            }
        }
        if s.loose_key_id.is_some() && loose_key_id.is_none() {
            loose_key_id = s.loose_key_id.clone();
        }
    }
    if names.is_empty() {
        return None;
    }
    let mut pre = String::new();
    for (n, _) in HELPERS {
        let Some(l) = names
            .iter()
            .find(|(h, _)| *h == *n)
            .map(|(_, l)| l.as_str())
        else {
            continue;
        };
        let id = if *n == "classPrivateFieldLooseKey" {
            Some(
                loose_key_id
                    .as_deref()
                    .expect("internal: classPrivateFieldLooseKey has no loose_key_id"),
            )
        } else {
            None
        };
        if let Some(id) = id {
            pre += &format!("var {id} = 0;\n");
        }
        let body = rewrite_template(
            HELPERS.iter().find(|(h, _)| h == n).unwrap().1,
            n,
            &names,
            id,
        );
        match body.strip_prefix("function ") {
            Some(rest) => pre += &format!("function {l}{rest}\n"),
            None => pre += &format!("var {l} = {body};\n"),
        }
    }
    Some(pre)
}

/// Tables for `tests/helper_templates.rs`: name, body, member spans, `ID` spans.
pub type HelperAstTemplate = (
    &'static str,
    &'static str,
    &'static [(u32, u32, &'static str)],
    &'static [(u32, u32)],
);

pub fn helper_ast_templates() -> Vec<HelperAstTemplate> {
    HELPERS
        .iter()
        .map(|(n, b)| (*n, *b, helper_members(n), helper_id_spans(n)))
        .collect()
}

/// Version-stamp edits in `input` coordinates, remapped onto `output` via oxc's codegen map
/// (D-0262 / D-0265 / AB-L0075: no re-parse and no find+replace of printed JS; the helper
/// prelude is inserted once by the caller via [`prelude`]).
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
        stamps = stamps
            .into_iter()
            .map(|e| remap_via_map(&pts, e, filename))
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(stamps)
}

/// Remap edits through a subsequent span-edit (kept text shifts; inserts at 0 stay at 0).
pub fn remap_edits(
    edits: Vec<lwc_shared::edits::Edit>,
    pieces: &[lwc_shared::edits::Piece],
) -> Vec<lwc_shared::edits::Edit> {
    edits
        .into_iter()
        .map(|e| {
            if e.start == 0 && e.end == 0 {
                return e;
            }
            lwc_shared::edits::Edit {
                start: forward_piece(pieces, e.start).unwrap_or_else(|| {
                    panic!("post-codegen edit start {} left the kept text", e.start)
                }),
                end: forward_piece(pieces, e.end).unwrap_or_else(|| {
                    panic!("post-codegen edit end {} left the kept text", e.end)
                }),
                text: e.text,
            }
        })
        .collect()
}

fn codegen_points(
    map: &oxc_sourcemap::SourceMap<'_>,
    input: &str,
    output: &str,
) -> Vec<(u32, u32)> {
    let (li, lo) = (
        lwc_shared::LineIndex::new(input),
        lwc_shared::LineIndex::new(output),
    );
    let mut pts: Vec<(u32, u32)> = map
        .get_tokens()
        .filter_map(|t| {
            Some((
                li.to_offset(t.get_src_line(), t.get_src_col())?,
                lo.to_offset(t.get_dst_line(), t.get_dst_col())?,
            ))
        })
        .collect();
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
    let miss = |off| {
        lwc_shared::CompileError::new(
            format!("internal: codegen map has no token at source offset {off}"),
            filename,
        )
    };
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
        let e = lwc_shared::edits::Edit {
            start: 12,
            end: 12,
            text: "/*x*/".into(),
        };
        let err = super::remap_via_map(&pts, e, "t.js")
            .expect_err("unmapped offset must not extrapolate");
        assert!(
            err.message
                .contains("internal: codegen map has no token at source offset 12"),
            "{err:?}"
        );
        assert_eq!(err.filename.as_deref(), Some("t.js"));
    }
}
