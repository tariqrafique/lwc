//! Port of ssr-compiler compile-template: sync text-static cut.
pub mod slot;
pub mod component;
pub(crate) mod element;
use element::ElView;
use lwc_shared::template_ir::{ChildNode, ElementDirective, ElseBranch, Expression, LiteralValue, Root, RootDirective, TextValue};
use lwc_shared::{CompileError, SsrMode};

/// Emit outcome. `Err` is a compile-time throw from a transformer (ssr-compiler throws a plain `Error`,
/// which `compileTemplateForSSR` does not catch), e.g. element.ts:234-235.
pub(crate) type Emit = Result<(), CompileError>;

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
pub fn compile_body(root: &Root, opts: TemplateOptions) -> Result<(Vec<String>, Vec<&'static str>), CompileError> {
    let cx = compile_cx(root, opts)?;
    let mut body = cx.hoisted.clone(); body.extend(cx.body);
    Ok((body, cx.imports))
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

pub fn compile_cx(root: &Root, mut opts: TemplateOptions) -> Result<Cx, CompileError> {
    for d in &root.directives { if let RootDirective::PreserveComments(b) = d { opts.preserve_comments = *b; } }
    let mut cx = Cx::default();
    let mut out = Vec::new();
    emit_children(&root.children, opts, &mut out, &mut cx, &mut Vec::new())?;
    cx.body = out; Ok(cx)
}

/// Full sync ESM module for a template.
pub fn compile_module(root: &Root, opts: TemplateOptions, meta: &ModuleMeta) -> Result<String, CompileError> {
    let cx = compile_cx(root, opts)?;
    let mut body = cx.hoisted.clone(); body.extend(cx.body.iter().cloned());
    Ok(wrap_module_with(&body, &cx.imports, &cx.modules, meta, opts))
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
/// Complex expressions (AB-D0004 S5) bind and print the parsed `.ast`; they are not re-parsed from `.source`.
pub(crate) fn expr(e: &Expression, sc: &[String]) -> Result<String, CompileError> { expr_in(e, true, sc) }
fn expr_in(e: &Expression, top: bool, sc: &[String]) -> Result<String, CompileError> {
    Ok(match e {
        Expression::Literal(l) => match &l.value { LiteralValue::String(s) => js_str(s), LiteralValue::Bool(b) => b.to_string() },
        Expression::Identifier(i) => if top && !sc.contains(&i.name) { format!("instance.{}", i.name) } else { i.name.clone() },
        Expression::Member(m) => if m.computed { format!("{}[{}]", expr(&m.object, sc)?, expr(&m.property, sc)?) }
            else { format!("{}.{}", expr(&m.object, sc)?, expr_in(&m.property, false, sc)?) },
        Expression::Complex(c) => print_complex(c, sc)?,
    })
}

/// astring `NEEDS_PARENTHESES` (17): Object/Arrow/Function/Class need parens in insertion sites.
fn wrap_printed(js: lwc_template::printer::Js) -> String {
    if js.precedence() <= 17 { format!("({})", js.code) } else { js.code }
}

fn print_complex(c: &lwc_shared::template_ir::ComplexExpression, sc: &[String]) -> Result<String, CompileError> {
    let plan = lwc_template::expr::bind(c, &|n| sc.iter().any(|s| s == n)).map_err(bind_err)?;
    let js = lwc_template::printer::print_expr(c.ast, &plan, "instance", 0).map_err(print_err)?;
    Ok(wrap_printed(js))
}

fn bind_err(e: lwc_template::expr::BindError) -> CompileError {
    let ce = e.compiler_error();
    CompileError {
        message: ce.message.clone(),
        filename: None,
        diagnostics: vec![lwc_shared::CompilerDiagnostic::new(ce.code, ce.message, ce.level, None, ce.location)],
        name: None,
        line: ce.location.map(|l| l.line),
        column: ce.location.map(|l| l.column),
    }
}

fn print_err(e: lwc_template::printer::PrintError) -> CompileError {
    CompileError {
        message: format!("internal error printing template expression: {}", e.0),
        filename: None, diagnostics: vec![], name: None, line: None, column: None,
    }
}

fn text_value(v: &TextValue, sc: &[String]) -> Result<String, CompileError> {
    match v { TextValue::Literal(s) => Ok(js_str(s)), TextValue::Expression(e) => expr(e, sc) }
}

pub(crate) fn emit_children(sib: &[ChildNode], o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) -> Emit {
    let reset_top = std::mem::take(&mut cx.reset_top);
    for (i, n) in sib.iter().enumerate() {
        let saved = cx.slotted;
        if reset_top && matches!(n, ChildNode::Element(_) | ChildNode::ExternalComponent(_)) { cx.slotted = false; }
        emit_child(sib, i, n, o, out, cx, sc)?;
        cx.slotted = saved;
    }
    Ok(())
}

fn emit_child(sib: &[ChildNode], i: usize, n: &ChildNode, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) -> Emit {
    {
        // ir-to-es irToEs: any node carrying a Dynamic directive throws at render time.
        let dirs: &[ElementDirective] = match n {
            ChildNode::Element(e) => &e.directives, ChildNode::Component(e) => &e.directives, ChildNode::ExternalComponent(e) => &e.directives,
            ChildNode::Slot(e) => &e.directives, ChildNode::LwcComponent(e) => &e.directives, _ => &[],
        };
        if dirs.iter().any(|d| matches!(d, ElementDirective::Dynamic(_))) {
            out.push("throw new Error('The lwc:dynamic directive is not supported for SSR. Use <lwc:component> instead.');".into());
            return Ok(());
        }
        // text.ts / comment.ts (non-preserving): the last node of a concatenated run renders the run's text.
        if is_concat(n, o) && !sib.get(i + 1).is_some_and(|s| is_concat(s, o)) {
            let mut parts = Vec::new();
            for s in sib[..=i].iter().rev() {
                if !is_concat(s, o) { break; }
                if let ChildNode::Text(t) = s { parts.insert(0, format!("normalizeTextContent({})", text_value(&t.value, sc)?)); }
            }
            if !parts.is_empty() {
                for im in ["normalizeTextContent", "renderTextContent"] { add(&mut cx.imports, im); }
                out.push(o.y(&format!("renderTextContent({})", parts.join(" + "))));
            }
        }
        match n {
            ChildNode::Element(e) => element::emit_element(&ElView { name: &e.name, namespace: &e.namespace, attributes: &e.attributes, properties: &e.properties,
                directives: &e.directives, children: &e.children, is_element: true }, o, out, cx, sc)?,
            ChildNode::ExternalComponent(e) => element::emit_element(&ElView { name: &e.name, namespace: &e.namespace, attributes: &e.attributes, properties: &e.properties,
                directives: &e.directives, children: &e.children, is_element: false }, o, out, cx, sc)?,
            // comment.ts: literal `<!--${value}-->`, only JS-string escaping.
            ChildNode::Comment(c) if o.preserve_comments => out.push(o.y(&js_str(&format!("<!--{}-->", c.value)))),
            // legacyIf.ts
            ChildNode::If(f) => {
                let c = expr(&f.condition, sc)?;
                out.push(if f.modifier == "true" { format!("if ({c}) {{") } else { format!("if (!{c}) {{") });
                emit_children(&f.children, o, out, cx, sc)?;
                out.push("}".into());
            }
            // lwcIf.ts (bookends suppressed when cx.slotted)
            ChildNode::IfBlock(b) => emit_if_chain(&b.condition, &b.children, b.else_.as_deref(), o, out, cx, sc)?,
            // ir-to-es.ts defaultTransformer: lwc:elseif/lwc:else are reached only through their IfBlock, and a
            // `<template lwc:slot-data>` only as a direct component child (slotted-content.ts). Met anywhere else
            // (e.g. under lwc:if inside a component), upstream throws `Unimplemented IR node: ${inspect(node)}`.
            ChildNode::ElseifBlock(_) => return Err(unimplemented_ir_node("ElseifBlock")),
            ChildNode::ElseBlock(_) => return Err(unimplemented_ir_node("ElseBlock")),
            ChildNode::ScopedSlotFragment(_) => return Err(unimplemented_ir_node("ScopedSlotFragment")),
            // for-each.ts: `for (let [index, item] of Object.entries(expr ?? {}))`; index defaults to `__unused__`.
            ChildNode::ForEach(f) => {
                let it = expr(&f.expression, sc)?;
                let idx = f.index.as_ref().map_or("__unused__".to_string(), |i| i.name.clone());
                out.push(format!("for (let [{idx}, {}] of Object.entries({it} ?? {{}})) {{", f.item.name));
                let n = sc.len(); sc.push(f.item.name.clone()); sc.push(idx);
                emit_children(&f.children, o, out, cx, sc)?;
                sc.truncate(n);
                out.push("}".into());
            }
            // for-of.ts: `for (let it of toIteratorDirective(expr ?? []))`. A bare Identifier is always
            // `instance.<id>` (only a MemberExpression rooted at a local stays unprefixed).
            ChildNode::ForOf(f) => {
                add(&mut cx.imports, crate::runtime_abi::TO_ITERATOR_DIRECTIVE);
                let it = match &f.expression { Expression::Identifier(i) => format!("instance.{}", i.name), e => expr(e, sc)? };
                out.push(format!("for (let {} of toIteratorDirective({it} ?? [])) {{", f.iterator.name));
                sc.push(f.iterator.name.clone());
                emit_children(&f.children, o, out, cx, sc)?;
                sc.pop();
                out.push("}".into());
            }
            ChildNode::Slot(s) => crate::compile_template::slot::emit_slot(s, o, out, cx, sc)?,
            ChildNode::Component(c) => crate::compile_template::component::emit_component(c, o, out, cx, sc)?,
            ChildNode::LwcComponent(c) => crate::compile_template::component::emit_lwc_component(c, o, out, cx, sc)?,
            ChildNode::Text(_) | ChildNode::Comment(_) => {}
        }
    }
    Ok(())
}

/// ir-to-es.ts `defaultTransformer` throws a plain Error (no code or location). Its message ends with Node's
/// `util.inspect` dump of the IR node; only the node type is reproduced (D-0247).
fn unimplemented_ir_node(kind: &str) -> lwc_shared::CompileError {
    lwc_shared::CompileError {
        message: format!("Unimplemented IR node: {{ type: '{kind}' }}"),
        filename: None, diagnostics: vec![], name: None, line: None, column: None,
    }
}

fn emit_bookended(children: &[ChildNode], o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) -> Emit {
    // lwcIf.ts: top-level slotted content drops bookends.
    if cx.slotted { return emit_children(children, o, out, cx, sc); }
    out.push(o.y("'<!---->'"));
    emit_children(children, o, out, cx, sc)?;
    out.push(o.y("'<!---->'"));
    Ok(())
}

fn emit_if_chain(cond: &Expression, children: &[ChildNode], else_: Option<&ElseBranch>, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) -> Emit {
    out.push(format!("if ({}) {{", expr(cond, sc)?));
    emit_bookended(children, o, out, cx, sc)?;
    match else_ {
        None => out.push("}".into()),
        Some(ElseBranch::Else(e)) => { out.push("} else {".into()); emit_bookended(&e.children, o, out, cx, sc)?; out.push("}".into()); }
        Some(ElseBranch::Elseif(e)) => {
            let start = out.len();
            emit_if_chain(&e.condition, &e.children, e.else_.as_deref(), o, out, cx, sc)?;
            out[start] = format!("}} else {}", out[start]);
        }
    }
    Ok(())
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
    /// Test IR location (production locations come from the parser).
    const AT: SourceLocation = SourceLocation { start_line: 1, start_column: 0, end_line: 1, end_column: 0, start: 0, end: 0 };
    fn el_at() -> lwc_shared::template_ir::ElementSourceLocation { lwc_shared::template_ir::ElementSourceLocation { location: AT, start_tag: AT, end_tag: None } }
    fn text(value: TextValue) -> ChildNode { ChildNode::Text(Text { value, raw: String::new(), location: AT }) }
    #[test]
    fn text_static() {
        let root = Root { children: vec![text(TextValue::Literal("\n    Static text\n".into()))], directives: vec![], location: el_at() };
        let (body, imports) = compile_body(&root, TemplateOptions::default()).unwrap();
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
        let root = Root { children: vec![t("a"), ChildNode::Comment(lwc_shared::template_ir::Comment { value: "c".into(), raw: String::new(), location: AT }), t("b")], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body, vec!["__lwcYield += renderTextContent(normalizeTextContent(instance.a) + normalizeTextContent(instance.b));"]);
    }
    fn el(name: &str, attrs: Vec<(&str, AttributeValue<'static>)>, children: Vec<ChildNode<'static>>) -> ChildNode<'static> {
        ChildNode::Element(lwc_shared::template_ir::Element { name: name.into(), children, namespace: element::HTML_NAMESPACE.into(),
            attributes: attrs.into_iter().map(|(n, value)| Attribute { name: n.into(), value, location: AT }).collect(),
            properties: vec![], directives: vec![], listeners: vec![], location: el_at() })
    }
    fn lit(s: &str) -> AttributeValue<'static> { AttributeValue::Literal(lwc_shared::template_ir::Literal { value: LiteralValue::String(s.into()) }) }
    #[test]
    fn static_element() {
        let root = Root { children: vec![el("div", vec![("class", lit("x"))], vec![text(TextValue::Literal("hi".into()))])], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body[0], "__lwcYield += '<div';");
        assert!(body[2].contains("__lwcYield += '=\"' + 'x' + suffix + '\"';"), "{body:?}");
        assert_eq!(&body[3..], &["__lwcYield += '>';", "__lwcYield += renderTextContent(normalizeTextContent('hi'));", "__lwcYield += '</div>';"]);
    }
    #[test]
    fn dynamic_attr() {
        let e = AttributeValue::Expression(Expression::Identifier(Identifier { name: "foo".into() }));
        let root = Root { children: vec![el("p", vec![("title", e.clone()), ("class", e)], vec![])], directives: vec![], location: el_at() };
        let (body, imports) = compile_body(&root, TemplateOptions::default()).unwrap();
        let j = body.join("\n");
        assert!(j.contains("instance.foo") && j.contains("htmlEscape(String(attrValue), true)") && j.contains("normalizeClass"), "{j}");
        assert!(imports.contains(&"htmlEscape") && imports.contains(&"normalizeClass"));
    }
    #[test]
    fn void_no_close() {
        let root = Root { children: vec![el("img", vec![("src", lit("a\"b"))], vec![]), el("br", vec![], vec![])], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        // element.ts: literal values are yielded verbatim; void elements end with `>` and no close tag.
        assert_eq!(body[3], "__lwcYield += ' src=\"a\"b\"';");
        assert_eq!(body[4], "__lwcYield += '>';");
        assert_eq!(body.last().unwrap(), "__lwcYield += '>';");
        assert!(!body.join("").contains("</"), "{body:?}");
    }
    fn id(n: &str) -> Expression<'static> { Expression::Identifier(Identifier { name: n.into() }) }
    fn cmt(v: &str) -> ChildNode<'static> { ChildNode::Comment(lwc_shared::template_ir::Comment { value: v.into(), raw: String::new(), location: AT }) }
    #[test]
    fn comment_preserved_vs_stripped() {
        let root = Root { children: vec![cmt(" it's ")], directives: vec![], location: el_at() };
        assert!(compile_body(&root, TemplateOptions::default()).unwrap().0.is_empty());
        assert_eq!(compile_body(&root, TemplateOptions { preserve_comments: true, ..Default::default() }).unwrap().0, vec!["__lwcYield += '<!-- it\\'s -->';"]);
        let root = Root { children: vec![cmt("x")], directives: vec![RootDirective::PreserveComments(true)], location: el_at() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).unwrap().0, vec!["__lwcYield += '<!--x-->';"]);
    }
    fn legacy_if(m: &str) -> ChildNode<'static> {
        ChildNode::If(lwc_shared::template_ir::If { modifier: m.into(), condition: id("show"), children: vec![text(TextValue::Literal("hi".into()))], location: AT, directive_location: AT })
    }
    #[test]
    fn if_true_text() {
        let root = Root { children: vec![legacy_if("true")], directives: vec![], location: el_at() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).unwrap().0, vec!["if (instance.show) {", "__lwcYield += renderTextContent(normalizeTextContent('hi'));", "}"]);
    }
    #[test]
    fn if_false() {
        let root = Root { children: vec![legacy_if("false")], directives: vec![], location: el_at() };
        assert_eq!(compile_body(&root, TemplateOptions::default()).unwrap().0[0], "if (!instance.show) {");
    }
    #[test]
    fn lwc_if_chain() {
        use lwc_shared::template_ir::{ElseBlock, ElseifBlock, IfBlock};
        let t = |s: &str| vec![text(TextValue::Literal(s.into()))];
        let root = Root { children: vec![ChildNode::IfBlock(IfBlock { condition: id("a"), children: t("A"), location: AT, directive_location: AT,
            else_: Some(Box::new(ElseBranch::Elseif(ElseifBlock { condition: id("b"), children: t("B"), location: AT, directive_location: AT,
                else_: Some(Box::new(ElseBranch::Else(ElseBlock { children: t("C"), location: AT, directive_location: AT }))) }))) })], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        let bk = "__lwcYield += '<!---->';";
        let r = |s: &str| format!("__lwcYield += renderTextContent(normalizeTextContent('{s}'));");
        assert_eq!(body, vec!["if (instance.a) {".into(), bk.into(), r("A"), bk.into(), "} else if (instance.b) {".into(), bk.into(), r("B"), bk.into(), "} else {".into(), bk.into(), r("C"), bk.into(), "}".into()]);
    }
    fn mem(o: &str, p: &str) -> Expression<'static> { Expression::Member(lwc_shared::template_ir::MemberExpression { object: Box::new(id(o)), property: Box::new(id(p)), computed: false }) }
    fn for_each(coll: Expression<'static>, item: &str, index: Option<&str>, children: Vec<ChildNode<'static>>) -> ChildNode<'static> {
        ChildNode::ForEach(lwc_shared::template_ir::ForEach { expression: coll, item: Identifier { name: item.into() }, index: index.map(|n| Identifier { name: n.into() }), children, location: AT, directive_location: AT })
    }
    fn tv(e: Expression) -> ChildNode { text(TextValue::Expression(e)) }
    fn rt(s: &str) -> String { format!("__lwcYield += renderTextContent(normalizeTextContent({s}));") }
    #[test]
    fn for_each_item_local_text_and_attr() {
        let li = el("li", vec![("title", AttributeValue::Expression(mem("item", "id")))], vec![]);
        let root = Root { children: vec![for_each(id("items"), "item", None, vec![li, tv(mem("item", "name")), tv(id("other"))])], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body[0], "for (let [__unused__, item] of Object.entries(instance.items ?? {})) {");
        assert!(body[4].contains("let attrValue = item.id;"), "{body:?}");
        assert!(body.contains(&"__lwcYield += renderTextContent(normalizeTextContent(item.name) + normalizeTextContent(instance.other));".to_string()), "{body:?}");
        assert_eq!(body.last().unwrap(), "}");
        assert!(!body.join("\n").contains("instance.item."), "{body:?}");
    }
    #[test]
    fn for_each_with_index() {
        let root = Root { children: vec![for_each(id("items"), "item", Some("i"), vec![tv(id("i"))]), tv(id("i"))], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body, vec!["for (let [i, item] of Object.entries(instance.items ?? {})) {".to_string(), rt("i"), "}".into(), rt("instance.i")]);
    }
    #[test]
    fn for_of_iterator() {
        let root = Root { children: vec![ChildNode::ForOf(lwc_shared::template_ir::ForOf { expression: id("items"), iterator: Identifier { name: "it".into() }, children: vec![tv(Expression::Member(lwc_shared::template_ir::MemberExpression { object: Box::new(mem("it", "value")), property: Box::new(id("name")), computed: false }))], location: AT, directive_location: AT })], directives: vec![], location: el_at() };
        let (body, imports) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body, vec!["for (let it of toIteratorDirective(instance.items ?? [])) {".to_string(), rt("it.value.name"), "}".into()]);
        assert!(imports.contains(&"toIteratorDirective"));
        assert!(crate::compile_template_for_ssr(&root, &Default::default()).unwrap().code.contains("toIteratorDirective"));
    }
    #[test]
    fn nested_for_each_scopes() {
        let inner = for_each(mem("row", "cells"), "cell", None, vec![tv(mem("cell", "v")), tv(mem("row", "k"))]);
        let root = Root { children: vec![for_each(id("rows"), "row", None, vec![inner, tv(id("cell"))])], directives: vec![], location: el_at() };
        let (body, _) = compile_body(&root, TemplateOptions::default()).unwrap();
        assert_eq!(body, vec![
            "for (let [__unused__, row] of Object.entries(instance.rows ?? {})) {".to_string(),
            "for (let [__unused__, cell] of Object.entries(row.cells ?? {})) {".into(),
            "__lwcYield += renderTextContent(normalizeTextContent(cell.v) + normalizeTextContent(row.k));".into(),
            "}".into(), rt("instance.cell"), "}".into()]);
    }

    fn cte_parse(src: &str) -> (&'static lwc_template::Allocator, lwc_template::TemplateParseResult<'static>) {
        let alloc: &'static lwc_template::Allocator = Box::leak(Box::default());
        let cfg = lwc_template::Config {
            name: Some("x".into()), namespace: Some("c".into()),
            experimental_complex_expressions: Some(true),
            ..Default::default()
        };
        let parsed = lwc_template::parse(alloc, src, cfg).unwrap();
        (alloc, parsed)
    }
    fn cte_body(src: &str) -> Vec<String> {
        let (_alloc, parsed) = cte_parse(src);
        compile_body(parsed.root.as_ref().unwrap(), TemplateOptions::default()).unwrap().0
    }
    #[test]
    fn complex_expr_uses_bound_ast_not_source_splice() {
        let body = cte_body("<template>{isTrue()}</template>");
        let joined = body.join("\n");
        assert!(joined.contains("normalizeTextContent(instance.isTrue())"), "{body:?}");
        assert!(!joined.contains("normalizeTextContent((instance.isTrue()))"), "unconditional wrap should be gone: {body:?}");

        let body = cte_body("<template>{a + b.c}</template>");
        let joined = body.join("\n");
        assert!(joined.contains("instance.a + instance.b.c"), "{body:?}");

        let body = cte_body("<template>{xs.map(x => x + y)}</template>");
        let joined = body.join("\n");
        assert!(joined.contains("instance.xs.map(x => x + instance.y)"), "{body:?}");

        let body = cte_body("<template>{a === undefined}</template>");
        let joined = body.join("\n");
        assert!(joined.contains("instance.a === undefined"), "{body:?}");
    }
    #[test]
    fn complex_shorthand_matches_astring() {
        // Upstream astring prints only the bound value of a shorthand (invalid `{instance.a}`), not `{a: instance.a}`.
        let body = cte_body("<template>{({a, b: c})}</template>");
        let joined = body.join("\n");
        assert!(joined.contains("instance.a"), "{body:?}");
        assert!(joined.contains("b: instance.c") || joined.contains("b:instance.c"), "{body:?}");
        assert!(!joined.contains("a: instance.a"), "SSR no longer expands shorthand: {body:?}");
    }
    #[test]
    fn complex_default_arrow_param_is_1189() {
        let (_alloc, parsed) = cte_parse("<template>{(x = 1) => x}</template>");
        let err = compile_body(parsed.root.as_ref().unwrap(), TemplateOptions::default()).unwrap_err();
        assert!(err.message.contains("LWC1189"), "{err:?}");
        assert!(err.message.contains("AssignmentPattern"), "{err:?}");
    }
}
