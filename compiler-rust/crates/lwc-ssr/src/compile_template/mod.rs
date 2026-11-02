//! Port of ssr-compiler compile-template: sync text-static cut.
pub mod slot;
pub mod component;
mod bind_expr;
pub(crate) mod element;
pub(crate) mod ir_compat; // SHIM(template-gap:ir-compat-walk): module retires when every template-gap id is fixed in lwc-template
use element::ElView;
use lwc_shared::template_ir::{ChildNode, ElementDirective, ElseBranch, Expression, LiteralValue, Root, RootDirective, TextValue};
use lwc_shared::SsrMode;

#[derive(Debug, Default, Clone, Copy)]
pub struct TemplateOptions { pub preserve_comments: bool, pub mode: SsrMode }

/// Mode-specific statement shapes. JS authors asyncYield (`async function*` + `yield`/`yield*`) and `transmogrify`
/// rewrites it for sync/async: `yield x`/`yield* x` -> `__lwcYield += x` (sync) / `__lwcYield += await x` (async);
/// generator fns -> plain/async fns with `let __lwcYield = ''; ... return __lwcYield;`; `fallbackTmpl`/`renderAttrs`
/// -> the `NoYield` variants. Sync strings are byte-identical to the pre-mode emitter.
impl TemplateOptions {
    /// `yield x;` (plain value).
    pub(crate) fn y(&self, x: &str) -> String {
        match self.mode { SsrMode::Sync => format!("__lwcYield += {x};"), SsrMode::Async => format!("__lwcYield += await {x};"), SsrMode::AsyncYield => format!("yield {x};") }
    }
    /// `yield* x;` (delegation to another template/generator).
    pub(crate) fn yd(&self, x: &str) -> String {
        match self.mode { SsrMode::AsyncYield => format!("yield* {x};"), _ => self.y(x) }
    }
    /// Function keyword for generator-shaped fns: `function` | `async function` | `async function*`.
    pub(crate) fn func(&self) -> &'static str {
        match self.mode { SsrMode::Sync => "function", SsrMode::Async => "async function", SsrMode::AsyncYield => "async function*" }
    }
    /// Body prologue / epilogue of a transmogrified fn (empty in asyncYield).
    pub(crate) fn prologue(&self) -> &'static str { if self.mode == SsrMode::AsyncYield { "" } else { "let __lwcYield = ''; " } }
    pub(crate) fn epilogue(&self) -> &'static str { if self.mode == SsrMode::AsyncYield { "" } else { "return __lwcYield; " } }
    pub(crate) fn fallback_tmpl(&self) -> &'static str {
        if self.mode == SsrMode::AsyncYield { crate::runtime_abi::FALLBACK_TMPL } else { crate::runtime_abi::FALLBACK_TMPL_NO_YIELD }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ModuleMeta {
    pub filename: String,
    pub namespace: String,
    pub name: String,
}

/// Returns the sync (`__lwcYield +=`) body statements and needed runtime imports.
pub fn compile_body(root: &Root, opts: TemplateOptions) -> (Vec<String>, Vec<&'static str>) {
    let cx = compile_cx(root, opts);
    let mut body = cx.hoisted.clone(); body.extend(cx.body);
    (body, cx.imports)
}

/// Mutable transform context (TransformerContext subset): runtime imports, child-component module imports,
/// hoisted shadow-slot fns, and `isSlotted`.
#[derive(Debug, Default)]
pub struct Cx {
    pub imports: Vec<&'static str>,
    /// (local name, import path) default imports of child component modules.
    pub modules: Vec<(String, String)>,
    pub hoisted: Vec<String>,
    pub body: Vec<String>,
    pub(crate) slotted: bool,
    /// getShadowSlottedContent: reset isSlotted for *top-level* Element/ExternalComponent children.
    pub(crate) reset_top: bool,
    pub(crate) shadow_fns: Vec<(String, String)>,
    pub(crate) fn_id: usize,
}

pub fn compile_cx(root: &Root, mut opts: TemplateOptions) -> Cx {
    for d in &root.directives { if let RootDirective::PreserveComments(b) = d { opts.preserve_comments = *b; } }
    // SHIM(template-gap:ir-compat-walk): normalize_root entry; retires with lwc-directive-attrs, lwc-component-node, component-prop-split, clean-text-node, empty-ns-prefix
    let root = ir_compat::normalize_root(root); // SHIM(template-gap:ir-compat-walk)
    let mut cx = Cx::default();
    let mut out = Vec::new();
    emit_children(&root.children, opts, &mut out, &mut cx, &mut Vec::new());
    cx.body = out; cx
}

/// Full sync ESM module for a template.
pub fn compile_module(root: &Root, opts: TemplateOptions, meta: &ModuleMeta) -> String {
    let cx = compile_cx(root, opts);
    let mut body = cx.hoisted.clone(); body.extend(cx.body.iter().cloned());
    wrap_module_with(&body, &cx.imports, &cx.modules, meta, opts)
}

pub(crate) fn is_concat(n: &ChildNode, o: TemplateOptions) -> bool {
    matches!(n, ChildNode::Text(_)) || (matches!(n, ChildNode::Comment(_)) && !o.preserve_comments)
}

pub(crate) fn js_str(s: &str) -> String {
    let mut r = String::from("'");
    for c in s.chars() { match c {
        '\'' => r.push_str("\\'"), '\\' => r.push_str("\\\\"), '\n' => r.push_str("\\n"),
        '\r' => r.push_str("\\r"), '\t' => r.push_str("\\t"),
        '\u{2028}' => r.push_str("\\u2028"), '\u{2029}' => r.push_str("\\u2029"),
        c if (c as u32) < 0x20 || c == '\u{7f}' => r.push_str(&format!("\\u{:04x}", c as u32)),
        c => r.push(c) } }
    r.push('\''); r
}

/// Identifiers resolve to `instance.<name>` unless the root identifier is a for:each/for:of local in `sc` (expression.ts getScopedExpression).
pub(crate) fn expr(e: &Expression, sc: &[String]) -> String { expr_in(e, true, sc) }
fn expr_in(e: &Expression, top: bool, sc: &[String]) -> String {
    match e {
        Expression::Literal(l) => match &l.value { LiteralValue::String(s) => js_str(s), LiteralValue::Bool(b) => b.to_string() },
        Expression::Identifier(i) => if top && !sc.contains(&i.name) { format!("instance.{}", i.name) } else { i.name.clone() },
        Expression::Member(m) => if m.computed { format!("{}[{}]", expr(&m.object, sc), expr(&m.property, sc)) }
            else { format!("{}.{}", expr(&m.object, sc), expr_in(&m.property, false, sc)) },
        Expression::Complex(s) => bind_expr::bind(s, sc),
    }
}

fn text_value(v: &TextValue, sc: &[String]) -> String {
    match v { TextValue::Literal(s) => js_str(s), TextValue::Expression(e) => expr(e, sc), TextValue::Complex(s) => bind_expr::bind(s, sc) }
}

pub(crate) fn emit_children(sib: &[ChildNode], o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    let reset_top = std::mem::take(&mut cx.reset_top);
    for (i, n) in sib.iter().enumerate() {
        let saved = cx.slotted;
        if reset_top && matches!(n, ChildNode::Element(_) | ChildNode::ExternalComponent(_)) { cx.slotted = false; }
        emit_child(sib, i, n, o, out, cx, sc);
        cx.slotted = saved;
    }
}

fn emit_child(sib: &[ChildNode], i: usize, n: &ChildNode, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    {
        // ir-to-es irToEs: any node carrying a Dynamic directive throws at render time.
        let dirs: &[ElementDirective] = match n {
            ChildNode::Element(e) => &e.directives, ChildNode::Component(e) => &e.directives, ChildNode::ExternalComponent(e) => &e.directives,
            ChildNode::Slot(e) => &e.directives, ChildNode::LwcComponent(e) => &e.directives, _ => &[],
        };
        if dirs.iter().any(|d| matches!(d, ElementDirective::Dynamic(_))) {
            out.push("throw new Error('The lwc:dynamic directive is not supported for SSR. Use <lwc:component> instead.');".into());
            return;
        }
        // text.ts / comment.ts (non-preserving): the last node of a concatenated run renders the run's text.
        if is_concat(n, o) && !sib.get(i + 1).is_some_and(|s| is_concat(s, o)) {
            let mut parts = Vec::new();
            for s in sib[..=i].iter().rev() {
                if !is_concat(s, o) { break; }
                if let ChildNode::Text(t) = s { parts.insert(0, format!("normalizeTextContent({})", text_value(&t.value, sc))); }
            }
            if !parts.is_empty() {
                for im in ["normalizeTextContent", "renderTextContent"] { add(&mut cx.imports, im); }
                out.push(o.y(&format!("renderTextContent({})", parts.join(" + "))));
            }
        }
        match n {
            ChildNode::Element(e) => element::emit_element(&ElView { name: &e.name, namespace: &e.namespace, attributes: &e.attributes, properties: &e.properties,
                directives: &e.directives, children: &e.children, is_element: true }, o, out, cx, sc),
            ChildNode::ExternalComponent(e) => element::emit_element(&ElView { name: &e.name, namespace: &e.namespace, attributes: &e.attributes, properties: &e.properties,
                directives: &e.directives, children: &e.children, is_element: false }, o, out, cx, sc),
            // comment.ts: literal `<!--${value}-->`, only JS-string escaping.
            ChildNode::Comment(c) if o.preserve_comments => out.push(o.y(&js_str(&format!("<!--{}-->", c.value)))),
            // legacyIf.ts
            ChildNode::If(f) => {
                let c = expr(&f.condition, sc);
                out.push(if f.modifier == "true" { format!("if ({c}) {{") } else { format!("if (!{c}) {{") });
                emit_children(&f.children, o, out, cx, sc);
                out.push("}".into());
            }
            // lwcIf.ts (bookends suppressed when cx.slotted)
            ChildNode::IfBlock(b) => emit_if_chain(&b.condition, &b.children, b.else_.as_deref(), o, out, cx, sc),
            ChildNode::ElseifBlock(b) => emit_if_chain(&b.condition, &b.children, b.else_.as_deref(), o, out, cx, sc),
            // for-each.ts: `for (let [index, item] of Object.entries(expr ?? {}))`; index defaults to `__unused__`.
            ChildNode::ForEach(f) => {
                let it = expr(&f.expression, sc);
                let idx = f.index.as_ref().map_or("__unused__".to_string(), |i| i.name.clone());
                out.push(format!("for (let [{idx}, {}] of Object.entries({it} ?? {{}})) {{", f.item.name));
                let n = sc.len(); sc.push(f.item.name.clone()); sc.push(idx);
                emit_children(&f.children, o, out, cx, sc);
                sc.truncate(n);
                out.push("}".into());
            }
            // for-of.ts: `for (let it of toIteratorDirective(expr ?? []))`. A bare Identifier is always
            // `instance.<id>` (only a MemberExpression rooted at a local stays unprefixed).
            ChildNode::ForOf(f) => {
                add(&mut cx.imports, crate::runtime_abi::TO_ITERATOR_DIRECTIVE);
                let it = match &f.expression { Expression::Identifier(i) => format!("instance.{}", i.name), e => expr(e, sc) };
                out.push(format!("for (let {} of toIteratorDirective({it} ?? [])) {{", f.iterator.name));
                sc.push(f.iterator.name.clone());
                emit_children(&f.children, o, out, cx, sc);
                sc.pop();
                out.push("}".into());
            }
            ChildNode::Slot(s) => crate::compile_template::slot::emit_slot(s, o, out, cx, sc),
            ChildNode::Component(c) => crate::compile_template::component::emit_component(c, o, out, cx, sc),
            ChildNode::LwcComponent(c) => crate::compile_template::component::emit_lwc_component(c, o, out, cx, sc),
            // ScopedSlotFragment is only meaningful as a direct Component child (handled in slotted content).
            _ => {}
        }
    }
}

fn emit_bookended(children: &[ChildNode], o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    // lwcIf.ts: top-level slotted content drops bookends.
    if cx.slotted { emit_children(children, o, out, cx, sc); return; }
    out.push(o.y("'<!---->'"));
    emit_children(children, o, out, cx, sc);
    out.push(o.y("'<!---->'"));
}

fn emit_if_chain(cond: &Expression, children: &[ChildNode], else_: Option<&ElseBranch>, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    out.push(format!("if ({}) {{", expr(cond, sc)));
    emit_bookended(children, o, out, cx, sc);
    match else_ {
        None => out.push("}".into()),
        Some(ElseBranch::Else(e)) => { out.push("} else {".into()); emit_bookended(&e.children, o, out, cx, sc); out.push("}".into()); }
        Some(ElseBranch::Elseif(e)) => {
            let start = out.len();
            emit_if_chain(&e.condition, &e.children, e.else_.as_deref(), o, out, cx, sc);
            out[start] = format!("}} else {}", out[start]);
        }
    }
}

pub(crate) fn add(imports: &mut Vec<&'static str>, im: &'static str) { if !imports.contains(&im) { imports.push(im); } }

/// Wraps body lines in the sync ESM template module.
pub fn wrap_module(body: &[String], imports: &[&'static str]) -> String {
    wrap_module_with(body, imports, &[], &ModuleMeta::default(), TemplateOptions::default())
}

pub fn wrap_module_with(
    body: &[String],
    imports: &[&'static str],
    modules: &[(String, String)],
    meta: &ModuleMeta,
    o: TemplateOptions,
) -> String {
    // Only names the emitted code references (rollup UNUSED_EXTERNAL_IMPORT is fatal in the render gate).
    // index.ts: addImport(['renderStylesheets']) unconditionally (the header always references it).
    let mut ims: Vec<&str> = imports.iter().copied().filter(|s| *s != crate::runtime_abi::REGISTER_TEMPLATE).collect();
    if !ims.contains(&crate::runtime_abi::RENDER_STYLESHEETS) { ims.push(crate::runtime_abi::RENDER_STYLESHEETS); }
    let mut named = vec!["registerTemplate as __lwcRegisterTemplate".to_string()];
    named.extend(ims.iter().map(|s| (*s).to_string()));
    let mut c = format!(
        "import {{ {} }} from '{}';\n",
        named.join(", "),
        crate::runtime_abi::RUNTIME_MODULE
    );
    for (local, path) in modules {
        c.push_str(&format!("import {local} from '{path}';\n"));
    }
    // JS getStylesheetImports uses the HTML filename stem, not the component `name` option
    // (superclass fixtures: tmpl.html with name=fancyElement → ./tmpl.css).
    let stem = std::path::Path::new(&meta.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .filter(|s| *s != "unknown")
        .or_else(|| if !meta.name.is_empty() { Some(meta.name.clone()) } else { None })
        .unwrap_or_else(|| "unknown".into());
    c.push_str(&format!("import defaultStylesheets from './{stem}.css';\n"));
    c.push_str(&format!("import defaultScopedStylesheets from './{stem}.scoped.css?scoped=true';\n"));
    let tokens = lwc_template::generate_scope_tokens(
        if meta.filename.is_empty() { "unknown.html" } else { &meta.filename },
        Some(meta.namespace.as_str()).filter(|s| !s.is_empty()),
        Some(meta.name.as_str()).filter(|s| !s.is_empty()),
    );
    c.push_str(&format!(
        "const stylesheetScopeToken = '{}';\n",
        tokens.scope_token
    ));
    c.push_str("const hasScopedStylesheets = defaultScopedStylesheets !== undefined && defaultScopedStylesheets.length > 0;\n");
    // compile-template/index.ts: `export default async function* __lwcTmpl(...)`, transmogrified for sync/async.
    c.push_str(&format!("export default {} __lwcTmpl(shadowSlottedContent, lightSlottedContent, scopedSlottedContent, Cmp, instance, renderContext) {{\n", o.func()));
    if o.mode != lwc_shared::SsrMode::AsyncYield { c.push_str("  let __lwcYield = '';\n"); }
    c.push_str("  let textContentBuffer = '';\n  let didBufferTextContent = false;\n");
    c.push_str("  const slotAttributeValue = null;\n  const contextfulParent = instance;\n  const isLightDom = Cmp.renderMode === 'light';\n");
    c.push_str(&format!("  if (!isLightDom) {{\n    {}\n  }}\n", o.y("`<template shadowrootmode=\"open\"${Cmp.delegatesFocus ? ' shadowrootdelegatesfocus' : ''}>`")));
    // index.ts bExportTemplate: static + default stylesheets render before the body.
    c.push_str(&format!("  const {{ stylesheets: staticStylesheets }} = Cmp;\n  if (defaultStylesheets || defaultScopedStylesheets || staticStylesheets) {{\n    {}\n  }}\n",
        o.y("renderStylesheets(renderContext, defaultStylesheets, defaultScopedStylesheets, staticStylesheets, stylesheetScopeToken, Cmp, hasScopedStylesheets)")));
    for l in body {
        c.push_str("  ");
        c.push_str(l);
        c.push('\n');
    }
    c.push_str(&format!("  if (!isLightDom) {{\n    {}\n    if (shadowSlottedContent) {{\n      {}\n    }}\n  }}\n", o.y("'</template>'"), o.yd("shadowSlottedContent(contextfulParent)")));
    if o.mode != lwc_shared::SsrMode::AsyncYield { c.push_str("  return __lwcYield;\n"); }
    c.push_str("}\n");
    c.push_str("__lwcTmpl.hasScopedStylesheets = hasScopedStylesheets;\n");
    c.push_str("__lwcTmpl.stylesheetScopeToken = stylesheetScopeToken;\n");
    c.push_str("__lwcRegisterTemplate(__lwcTmpl);\n");
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwc_shared::template_ir::{Attribute, AttributeValue, Identifier, SourceLocation, Text};
    fn text(value: TextValue) -> ChildNode { ChildNode::Text(Text { value, raw: String::new(), location: SourceLocation::default() }) }
    #[test]
    fn text_static() {
        let root = Root { children: vec![text(TextValue::Literal("\n    Static text\n".into()))], ..Default::default() };
        let (body, imports) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body, vec!["__lwcYield += renderTextContent(normalizeTextContent('\\n    Static text\\n'));"]);
        assert_eq!(imports, vec!["normalizeTextContent", "renderTextContent"]);
        let code = crate::compile_template_for_ssr(&root, &crate::TemplateTransformOptions::default()).unwrap().code;
        for n in ["function __lwcTmpl", "let __lwcYield", "shadowrootmode=\"open\"", "Static text", "from '@lwc/ssr-runtime'", "__lwcYield +="] {
            assert!(code.contains(n), "missing {n}: {code}");
        }
        assert!(!code.contains("yield ") && !code.contains("async function"), "{code}");
    }
    #[test]
    fn adjacent_text_concat_skips_comment() {
        let t = |s: &str| text(TextValue::Expression(Expression::Identifier(Identifier { name: s.into() })));
        let root = Root { children: vec![t("a"), ChildNode::Comment(lwc_shared::template_ir::Comment { value: "c".into(), raw: String::new(), location: SourceLocation::default() }), t("b")], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body, vec!["__lwcYield += renderTextContent(normalizeTextContent(instance.a) + normalizeTextContent(instance.b));"]);
    }
    fn el(name: &str, attrs: Vec<(&str, AttributeValue)>, children: Vec<ChildNode>) -> ChildNode {
        ChildNode::Element(lwc_shared::template_ir::Element { name: name.into(), children, namespace: element::HTML_NAMESPACE.into(),
            attributes: attrs.into_iter().map(|(n, value)| Attribute { name: n.into(), value, location: SourceLocation::default() }).collect(), ..Default::default() })
    }
    fn lit(s: &str) -> AttributeValue { AttributeValue::Literal(lwc_shared::template_ir::Literal { value: LiteralValue::String(s.into()) }) }
    #[test]
    fn static_element() {
        let root = Root { children: vec![el("div", vec![("class", lit("x"))], vec![text(TextValue::Literal("hi".into()))])], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body[0], "__lwcYield += '<div';");
        assert!(body[2].contains("__lwcYield += '=\"' + 'x' + suffix + '\"';"), "{body:?}");
        assert_eq!(&body[3..], &["__lwcYield += '>';", "__lwcYield += renderTextContent(normalizeTextContent('hi'));", "__lwcYield += '</div>';"]);
    }
    #[test]
    fn dynamic_attr() {
        let e = AttributeValue::Expression(Expression::Identifier(Identifier { name: "foo".into() }));
        let root = Root { children: vec![el("p", vec![("title", e.clone()), ("class", e)], vec![])], ..Default::default() };
        let (body, imports) = compile_body(&root, TemplateOptions::default());
        let j = body.join("\n");
        assert!(j.contains("instance.foo") && j.contains("htmlEscape(String(attrValue), true)") && j.contains("normalizeClass"), "{j}");
        assert!(imports.contains(&"htmlEscape") && imports.contains(&"normalizeClass"));
    }
    #[test]
    fn void_no_close() {
        let root = Root { children: vec![el("img", vec![("src", lit("a\"b"))], vec![]), el("br", vec![], vec![])], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        // element.ts: literal values are yielded verbatim; void elements end with `>` and no close tag.
        assert_eq!(body[3], "__lwcYield += ' src=\"a\"b\"';");
        assert_eq!(body[4], "__lwcYield += '>';");
        assert_eq!(body.last().unwrap(), "__lwcYield += '>';");
        assert!(!body.join("").contains("</"), "{body:?}");
    }
    fn id(n: &str) -> Expression { Expression::Identifier(Identifier { name: n.into() }) }
    fn cmt(v: &str) -> ChildNode { ChildNode::Comment(lwc_shared::template_ir::Comment { value: v.into(), raw: String::new(), location: SourceLocation::default() }) }
    #[test]
    fn comment_preserved_vs_stripped() {
        let root = Root { children: vec![cmt(" it's ")], ..Default::default() };
        assert!(compile_body(&root, TemplateOptions::default()).0.is_empty());
        assert_eq!(compile_body(&root, TemplateOptions { preserve_comments: true, ..Default::default() }).0, vec!["__lwcYield += '<!-- it\\'s -->';"]);
        let root = Root { children: vec![cmt("x")], directives: vec![RootDirective::PreserveComments(true)], ..Default::default() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).0, vec!["__lwcYield += '<!--x-->';"]);
    }
    fn legacy_if(m: &str) -> ChildNode {
        ChildNode::If(lwc_shared::template_ir::If { modifier: m.into(), condition: id("show"), children: vec![text(TextValue::Literal("hi".into()))], location: Default::default(), directive_location: Default::default() })
    }
    #[test]
    fn if_true_text() {
        let root = Root { children: vec![legacy_if("true")], ..Default::default() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).0, vec!["if (instance.show) {", "__lwcYield += renderTextContent(normalizeTextContent('hi'));", "}"]);
    }
    #[test]
    fn if_false() {
        let root = Root { children: vec![legacy_if("false")], ..Default::default() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).0[0], "if (!instance.show) {");
    }
    #[test]
    fn lwc_if_chain() {
        use lwc_shared::template_ir::{ElseBlock, ElseifBlock, IfBlock};
        let t = |s: &str| vec![text(TextValue::Literal(s.into()))];
        let root = Root { children: vec![ChildNode::IfBlock(IfBlock { condition: id("a"), children: t("A"), location: Default::default(), directive_location: Default::default(),
            else_: Some(Box::new(ElseBranch::Elseif(ElseifBlock { condition: id("b"), children: t("B"), location: Default::default(), directive_location: Default::default(),
                else_: Some(Box::new(ElseBranch::Else(ElseBlock { children: t("C"), location: Default::default(), directive_location: Default::default() }))) }))) })], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        let bk = "__lwcYield += '<!---->';";
        let r = |s: &str| format!("__lwcYield += renderTextContent(normalizeTextContent('{s}'));");
        assert_eq!(body, vec!["if (instance.a) {".into(), bk.into(), r("A"), bk.into(), "} else if (instance.b) {".into(), bk.into(), r("B"), bk.into(), "} else {".into(), bk.into(), r("C"), bk.into(), "}".into()]);
    }
    fn mem(o: &str, p: &str) -> Expression { Expression::Member(lwc_shared::template_ir::MemberExpression { object: Box::new(id(o)), property: Box::new(id(p)), computed: false }) }
    fn for_each(coll: Expression, item: &str, index: Option<&str>, children: Vec<ChildNode>) -> ChildNode {
        ChildNode::ForEach(lwc_shared::template_ir::ForEach { expression: coll, item: Identifier { name: item.into() }, index: index.map(|n| Identifier { name: n.into() }), children, location: Default::default(), directive_location: Default::default() })
    }
    fn tv(e: Expression) -> ChildNode { text(TextValue::Expression(e)) }
    fn rt(s: &str) -> String { format!("__lwcYield += renderTextContent(normalizeTextContent({s}));") }
    #[test]
    fn for_each_item_local_text_and_attr() {
        let li = el("li", vec![("title", AttributeValue::Expression(mem("item", "id")))], vec![]);
        let root = Root { children: vec![for_each(id("items"), "item", None, vec![li, tv(mem("item", "name")), tv(id("other"))])], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body[0], "for (let [__unused__, item] of Object.entries(instance.items ?? {})) {");
        assert!(body[4].contains("let attrValue = item.id;"), "{body:?}");
        assert!(body.contains(&"__lwcYield += renderTextContent(normalizeTextContent(item.name) + normalizeTextContent(instance.other));".to_string()), "{body:?}");
        assert_eq!(body.last().unwrap(), "}");
        assert!(!body.join("\n").contains("instance.item."), "{body:?}");
    }
    #[test]
    fn for_each_with_index() {
        let root = Root { children: vec![for_each(id("items"), "item", Some("i"), vec![tv(id("i"))]), tv(id("i"))], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body, vec!["for (let [i, item] of Object.entries(instance.items ?? {})) {".to_string(), rt("i"), "}".into(), rt("instance.i")]);
    }
    #[test]
    fn for_of_iterator() {
        let root = Root { children: vec![ChildNode::ForOf(lwc_shared::template_ir::ForOf { expression: id("items"), iterator: Identifier { name: "it".into() }, children: vec![tv(Expression::Member(lwc_shared::template_ir::MemberExpression { object: Box::new(mem("it", "value")), property: Box::new(id("name")), computed: false }))], location: Default::default(), directive_location: Default::default() })], ..Default::default() };
        let (body, imports) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body, vec!["for (let it of toIteratorDirective(instance.items ?? [])) {".to_string(), rt("it.value.name"), "}".into()]);
        assert!(imports.contains(&"toIteratorDirective"));
        assert!(crate::compile_template_for_ssr(&root, &Default::default()).unwrap().code.contains("toIteratorDirective"));
    }
    #[test]
    fn nested_for_each_scopes() {
        let inner = for_each(mem("row", "cells"), "cell", None, vec![tv(mem("cell", "v")), tv(mem("row", "k"))]);
        let root = Root { children: vec![for_each(id("rows"), "row", None, vec![inner, tv(id("cell"))])], ..Default::default() };
        let (body, _) = compile_body(&root, TemplateOptions::default());
        assert_eq!(body, vec![
            "for (let [__unused__, row] of Object.entries(instance.rows ?? {})) {".to_string(),
            "for (let [__unused__, cell] of Object.entries(row.cells ?? {})) {".into(),
            "__lwcYield += renderTextContent(normalizeTextContent(cell.v) + normalizeTextContent(row.k));".into(),
            "}".into(), rt("instance.cell"), "}".into()]);
    }
}
