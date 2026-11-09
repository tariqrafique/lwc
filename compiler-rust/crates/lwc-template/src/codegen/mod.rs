//! IR → ESM module source (behavioral goldens, not astring bytes).

use crate::config::NormalizedConfig;
use crate::parser::InternalError;
use expression::BindError;
use printer::PrintError;

mod optimize;
pub(crate) mod printer;
pub(crate) mod expression;
use printer::Js;
use lwc_shared::template_ir::*;
use lwc_shared::api_version::{is_api_feature_enabled, ApiFeature};
use lwc_shared::js::{js_trim, js_ws};
use lwc_shared::style::{normalize_style_attribute_value, parse_style_text, split_important};

pub fn js_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"), '\r' => o.push_str("\\r"), '\t' => o.push_str("\\t"),
            '\u{2028}' => o.push_str("\\u2028"), '\u{2029}' => o.push_str("\\u2029"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\x{:02x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Bind (1189) or print (invariant InternalError) failure from generate.
#[derive(Debug)]
pub(crate) enum CodegenError {
    Bind(BindError),
    Internal(InternalError),
}

impl From<BindError> for CodegenError {
    fn from(e: BindError) -> Self { Self::Bind(e) }
}
impl From<PrintError> for CodegenError {
    fn from(e: PrintError) -> Self { Self::Internal(InternalError::new(e.0)) }
}
impl From<InternalError> for CodegenError {
    fn from(e: InternalError) -> Self { Self::Internal(e) }
}

/// `custom-else` → `customElse`.
pub(crate) fn camel_name(rest: &str) -> String {
    let mut out = String::new();
    for (i, seg) in rest.split('-').enumerate() {
        let mut cs = seg.chars();
        match (i, cs.next()) { (0, Some(f)) => { out.push(f); out.extend(cs); } (_, Some(f)) => { out.push(f.to_ascii_uppercase()); out.extend(cs); } _ => {} }
    }
    out
}

/// TS RENDER_APIS (codegen.ts): the `$api` helpers a template can destructure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ApiHelper { StaticPart, StaticFragment, Text, DynamicText, Element, Key, Iterator, Flatten, ScopedId, ScopedFragId, TabIndex, Comment, Slot, Bind, Fragment, ScopedSlotFactory, CustomElement, DeprecatedDynamicComponent, DynamicComponent, SanitizeHtmlContent, NormalizeClassName }

impl ApiHelper {
    /// (local alias, `$api` name), as in TS RENDER_APIS `{ name, alias }`.
    const fn names(self) -> (&'static str, &'static str) {
        match self {
            ApiHelper::StaticPart => ("sp", "api_static_part"),
            ApiHelper::StaticFragment => ("st", "api_static_fragment"),
            ApiHelper::Text => ("t", "api_text"),
            ApiHelper::DynamicText => ("d", "api_dynamic_text"),
            ApiHelper::Element => ("h", "api_element"),
            ApiHelper::Key => ("k", "api_key"),
            ApiHelper::Iterator => ("i", "api_iterator"),
            ApiHelper::Flatten => ("f", "api_flatten"),
            ApiHelper::ScopedId => ("gid", "api_scoped_id"),
            ApiHelper::ScopedFragId => ("fid", "api_scoped_frag_id"),
            ApiHelper::TabIndex => ("ti", "api_tab_index"),
            ApiHelper::Comment => ("co", "api_comment"),
            ApiHelper::Slot => ("s", "api_slot"),
            ApiHelper::Bind => ("b", "api_bind"),
            ApiHelper::Fragment => ("fr", "api_fragment"),
            ApiHelper::ScopedSlotFactory => ("ssf", "api_scoped_slot_factory"),
            ApiHelper::CustomElement => ("c", "api_custom_element"),
            ApiHelper::DeprecatedDynamicComponent => ("ddc", "api_deprecated_dynamic_component"),
            ApiHelper::DynamicComponent => ("dc", "api_dynamic_component"),
            ApiHelper::SanitizeHtmlContent => ("shc", "api_sanitize_html_content"),
            ApiHelper::NormalizeClassName => ("ncls", "api_normalize_class_name"),
        }
    }
}

#[derive(Default)]
struct Gen { key: u32, locals: Vec<String>, slots: Vec<String>, imports: Vec<(String, String)>, has_refs: bool, static_opt: bool, comments: bool, frags: Vec<(String, bool)>, sanitize: bool, cr: Option<lwc_shared::CustomRendererConfig>, cur_cr: bool, renderer: bool, api_version: u32, light: bool, inner_html: u32, synthetic: bool, scope_frag: bool, apis: Vec<ApiHelper>, memo: u32}


/// TS `isExpression` (shared/ast.ts:525): `node.type !== 'Literal'`. A complex expression whose AST
/// is itself a Literal is not an expression for codegen branching (`computeAttrValue`).
fn is_estree_expression(v: &AttributeValue) -> bool {
    match v {
        AttributeValue::Literal(_) => false,
        AttributeValue::Expression(Expression::Literal(_)) => false,
        AttributeValue::Expression(Expression::Complex(c)) => c.as_literal().is_none(),
        AttributeValue::Expression(_) => true,
    }
}

/// TS `isStringLiteral`.
fn attr_string_literal<'a>(v: &'a AttributeValue<'a>) -> Option<&'a str> {
    match v {
        AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) => Some(s),
        AttributeValue::Expression(Expression::Literal(Literal { value: LiteralValue::String(s) })) => Some(s),
        AttributeValue::Expression(Expression::Complex(c)) => match c.as_literal() {
            Some(LiteralView::String(s)) => Some(s),
            _ => None,
        },
        _ => None,
    }
}

/// Value spelling of an already-matched ESTree Literal (astring `JSON.stringify` / `ToJsString`).
fn attr_literal_js(lit: LiteralView<'_>) -> String {
    match lit {
        LiteralView::Number(n) => printer::js_number(n),
        LiteralView::Bool(b) => b.to_string(),
        LiteralView::Null => "null".into(),
        LiteralView::String(s) => js_string(s),
    }
}

/// Classify an attribute value so `compute_attr` never receives a general expression as a literal.
enum AttrValueKind<'a> {
    StringLiteral(&'a str),
    OtherLiteral(LiteralView<'a>),
    Expression,
}

fn attr_value_kind<'a>(v: &'a AttributeValue<'a>) -> AttrValueKind<'a> {
    match v {
        AttributeValue::Literal(Literal { value: LiteralValue::String(s) })
        | AttributeValue::Expression(Expression::Literal(Literal { value: LiteralValue::String(s) })) => {
            AttrValueKind::StringLiteral(s)
        }
        AttributeValue::Literal(Literal { value: LiteralValue::Bool(b) })
        | AttributeValue::Expression(Expression::Literal(Literal { value: LiteralValue::Bool(b) })) => {
            AttrValueKind::OtherLiteral(LiteralView::Bool(*b))
        }
        AttributeValue::Expression(Expression::Complex(c)) => match c.as_literal() {
            Some(LiteralView::String(s)) => AttrValueKind::StringLiteral(s),
            Some(lit) => AttrValueKind::OtherLiteral(lit),
            None => AttrValueKind::Expression,
        },
        AttributeValue::Expression(_) => AttrValueKind::Expression,
    }
}

impl Gen {
    fn attr_value(&self, v: &AttributeValue) -> Result<String, CodegenError> { self.attr_value_at(v, 1) }

    fn attr_value_at(&self, v: &AttributeValue, indent: usize) -> Result<String, CodegenError> {
        match v {
            AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) => Ok(js_string(s)),
            AttributeValue::Literal(Literal { value: LiteralValue::Bool(b) }) => Ok(b.to_string()),
            AttributeValue::Expression(e) => self.expr_at(e, indent),
        }
    }

    fn expr(&self, e: &Expression) -> Result<String, CodegenError> { self.expr_at(e, 1) }

    fn expr_at(&self, e: &Expression, indent: usize) -> Result<String, CodegenError> {
        match e {
            Expression::Identifier(Identifier { name }) if self.locals.iter().any(|l| l == name) => Ok(name.clone()),
            Expression::Identifier(Identifier { name }) => Ok(format!("$cmp.{name}")),
            Expression::Member(MemberExpression { object, property, computed: false }) => match &**property {
                Expression::Identifier(Identifier { name }) => Ok(format!("{}.{name}", self.expr_at(object, indent)?)),
                p => Ok(format!("{}[{}]", self.expr_at(object, indent)?, self.expr_at(p, indent)?)),
            },
            // Typed computed members exist only without complex expressions (parser `non_binding_expr`), i.e.
            // under TS bindExpression (expression.ts:73-83), which binds an identifier only as the expression
            // itself or as a member `object`: a computed property identifier stays as written (`arr[index]`).
            Expression::Member(MemberExpression { object, property, .. }) => match &**property {
                Expression::Identifier(Identifier { name }) => Ok(format!("{}[{name}]", self.expr_at(object, indent)?)),
                p => Ok(format!("{}[{}]", self.expr_at(object, indent)?, self.expr_at(p, indent)?)),
            },
            Expression::Literal(Literal { value: LiteralValue::String(s) }) => Ok(js_string(s)),
            Expression::Literal(Literal { value: LiteralValue::Bool(b) }) => Ok(b.to_string()),
            Expression::Complex(c) => self.print_complex(c, indent),
        }
    }

    /// Bind + print `c.ast` (AB-D0004 S4). BindError (1189) and PrintError (InternalError) propagate
    /// with `?`; never a placeholder, panic, or Gen-field sink.
    fn print_complex(&self, c: &ComplexExpression, indent: usize) -> Result<String, CodegenError> {
        let plan = expression::bind(c, &|n| self.locals.iter().any(|l| l == n))?;
        Ok(printer::print_expr(c.ast, &plan, "$cmp", indent)?.code)
    }
}

impl Gen {
    /// TS transformChildren's per-child expressions (before the flatten decision). An `if:*` child
    /// splices its children in, each wrapped in its own ternary (TS transformIf). `slot_parent` is the
    /// TS `isSlot(parent) ? parent.slotName : undefined` of the parent whose children these are
    /// (index.ts:193); only element children receive it.
    fn children(&mut self, cs: &[ChildNode], ind: usize, slot_parent: Option<&str>) -> Result<Vec<String>, CodegenError> {
        // TS transformChildren: contiguous text nodes (skipping non-preserved comments) become one api_text.
        let mut res = Vec::new();
        let mut run: Vec<&Text> = Vec::new();
        for c in cs {
            match c {
                ChildNode::Text(t) => { run.push(t); continue; }
                ChildNode::Comment(_) if !self.comments => continue,
                _ => {}
            }
            if !run.is_empty() { res.push(self.text_node(&std::mem::take(&mut run))?); }
            match c {
                ChildNode::If(f) => res.extend(self.inline_if(f, ind)?),
                _ => res.extend(self.node(c, ind, slot_parent)?),
            }
        }
        if !run.is_empty() { res.push(self.text_node(&run)?); }
        Ok(res)
    }

    /// TS shouldFlatten: a for-block child, a light-DOM `<slot>` before USE_FRAGMENTS_FOR_LIGHT_DOM_SLOTS,
    /// or an `if:*` whose own children need flattening.
    fn should_flatten(&self, cs: &[ChildNode]) -> bool {
        cs.iter().any(|c| match c {
            ChildNode::ForEach(_) | ChildNode::ForOf(_) => true,
            ChildNode::Slot(_) => self.light && !is_api_feature_enabled(ApiFeature::UseFragmentsForLightDomSlots, self.api_version),
            ChildNode::If(f) => self.should_flatten(&f.children),
            _ => false,
        })
    }

    /// TS CodeGen._renderApiCall: register `h` (first use only), then emit `alias(args)` with the alias
    /// from the names() table. The only place a `$api` helper is registered, so none is registered
    /// without being emitted. Callers build `args` first, so helpers used inside the arguments register
    /// before `h` (TS post-order).
    fn call(&mut self, h: ApiHelper, args: &str) -> String {
        if !self.apis.contains(&h) { self.apis.push(h); }
        format!("{}({args})", h.names().1)
    }

    /// TS transformChildren: an array of child vnodes, or (when shouldFlatten) the single child expression
    /// / `api_flatten([...])`, so an iterator's array never lands as a nested non-vnode child.
    fn kids(&mut self, cs: &[ChildNode], ind: usize, slot_parent: Option<&str>) -> Result<Kids, CodegenError> {
        let res = self.children(cs, ind, slot_parent)?;
        if !self.should_flatten(cs) { return Ok(Kids::Arr(res)); }
        // TS `if (children.length === 1) return res[0]`: the lone child is a for-block, a light-DOM slot or a
        // flattening if:*, and each of those yields exactly one expression.
        if cs.len() == 1 {
            let [only]: [String; 1] = res.try_into().expect("a lone flattening child yields one expression");
            return Ok(Kids::Expr(only));
        }
        Ok(Kids::Expr(self.call(ApiHelper::Flatten, &format!("[{}]", res.join(", ")))))
    }

    /// TS transformIf / applyInlineIf: array children → one `test ? child : null` per child; a flattened
    /// expression → `test ? expr : []`.
    fn inline_if(&mut self, f: &If, ind: usize) -> Result<Vec<String>, CodegenError> {
        let x = self.expr(&f.condition)?;
        let test = match f.modifier.as_str() { "false" => format!("!{x}"), "strict-true" => format!("{x} === true"), _ => x };
        Ok(match self.kids(&f.children, ind, None)? {
            Kids::Arr(v) => v.into_iter().map(|e| format!("{test} ? {e} : null")).collect(),
            Kids::Expr(e) => vec![format!("{test} ? {e} : []")],
        })
    }

    /// TS transformElement: `lwc:dynamic` → genDeprecatedDynamicElement, `lwc:is` → genDynamicElement
    /// (neither registers a component import; children only when non-empty).
    fn dynamic_ctor(&mut self, name: &str, directives: &[ElementDirective], data: &str, kids: &str) -> Result<Option<String>, CodegenError> {
        let tail = if kids == "[]" { String::new() } else { format!(", {kids}") };
        for d in directives {
            if let ElementDirective::Dynamic(e) = d {
                let args = format!("{}, {}, {data}{tail}", js_string(name), self.expr(e)?);
                return Ok(Some(self.call(ApiHelper::DeprecatedDynamicComponent, &args)));
            }
        }
        if let Some(e) = directives.iter().find_map(|d| match d { ElementDirective::Is(e) => Some(e), _ => None }) {
            let args = format!("{}, {data}{tail}", self.expr(e)?);
            return Ok(Some(self.call(ApiHelper::DynamicComponent, &args)));
        }
        Ok(None)
    }

    /// TS genSanitizedHtmlExpr (`$ctx._sanitizedHtml$N` / `$ctx._rawHtml$N` memo).
    fn sanitized_html(&mut self, v: &AttributeValue, indent: usize) -> Result<String, CodegenError> {
        let i = self.inner_html;
        self.inner_html += 1;
        let x = self.attr_value_at(v, indent)?;
        let shc = self.call(ApiHelper::SanitizeHtmlContent, &x);
        let (sanitized, raw) = (|| Js::member(format!("$ctx._sanitizedHtml${i}")), || Js::member(format!("$ctx._rawHtml${i}")));
        Ok(match v {
            // codegen.ts:510-525: `$ctx._sanitizedHtml$N || ($ctx._sanitizedHtml$N = shc)`.
            AttributeValue::Literal(_) => Js::binary("||", sanitized(), Js::assign(sanitized(), &shc)),
            // codegen.ts:537-560: astring leaves the consequent assignment bare.
            AttributeValue::Expression(_) => Js::conditional(
                Js::binary("!==", raw(), Js::assign(raw(), &x)),
                Js::assign(sanitized(), &shc),
                sanitized(),
            ),
        }
        .code)
    }

    /// `x-foo-bar` → (`_xFooBar`, `x/fooBar`) (TS kebabcaseToCamelcase on the name after the namespace).
    fn component_binding(&mut self, tag: &str) -> String {
        let (ns, rest) = tag.split_once('-').unwrap_or((tag, ""));
        let name = camel_name(rest);
        let mut b = format!("_{ns}");
        let mut cs = name.chars();
        if let Some(f) = cs.next() { b.push(f.to_ascii_uppercase()); b.extend(cs); }
        if !self.imports.iter().any(|(x, _)| x == &b) { self.imports.push((b.clone(), format!("{ns}/{name}"))); }
        b
    }

    /// TS elementDataBag: the element's data object. The single place databag `$api` helpers
    /// (ncls, gid/fid/ti, shc, k, b) are registered; `transform_element` calls it for every element,
    /// static ones included.
    fn element_data_bag(&mut self, el: &ElementView, ind: usize, slot_parent: Option<&str>) -> Result<String, CodegenError> {
        let ElementView { name, ns, attrs, props, directives, listeners, kind, .. } = *el;
        let key = self.key;
        self.key += 1;
        let pad = "  ".repeat(ind);
        let needs_renderer = self.needs_renderer(name, ns, attrs, directives, kind);
        let outer_cr = std::mem::replace(&mut self.cur_cr, needs_renderer);
        // astring ObjectExpression: one property per line, no trailing comma.
        let obj = |props: Vec<String>, pad: &str| format!("{{\n{}\n{pad}}}", props.iter().map(|p| format!("{pad}  {p}")).collect::<Vec<_>>().join(",\n"));
        let mut data: Vec<String> = Vec::new();
        let inner_pad = format!("{pad}  ");
        // TS elementDataBag (index.ts:511-561), one walk over `attributes` so `$api` helpers register in
        // attribute order. `class`, `style` and `slot` become their own databag properties, pushed as
        // they are met and so ahead of `attrs`:
        // - class: expression → `className` (genClassExpression); string → `classMap` of
        //   parseClassNames (helpers.ts:219) → `true`; a boolean `class` emits nothing.
        // - style: expression → `style`; string → `styleDecls` (parseStyleText +
        //   styleMapToStyleDeclsAST, helpers.ts:202); a boolean `style` emits nothing.
        // - slot: expression → `slotAssignment`; string → its value; boolean → `""`.
        let mut attr_vs: Vec<(String, String)> = Vec::new();
        let bag_indent = ind + 1;
        let nest_indent = ind + 2;
        for a in attrs {
            let lit = attr_string_literal(&a.value);
            match a.name.as_str() {
                "class" if is_estree_expression(&a.value) => data.push(format!("className: {}", self.gen_class_expression(&a.value, bag_indent)?)),
                "class" => if let Some(v) = lit {
                    let names = parse_class_names(v).into_iter().map(|n| format!("{}: true", js_string(n))).collect::<Vec<_>>();
                    data.push(format!("classMap: {}", if names.is_empty() { "{}".into() } else { obj(names, &inner_pad) }));
                },
                "style" if is_estree_expression(&a.value) => data.push(format!("style: {}", self.attr_value_at(&a.value, bag_indent)?)),
                "style" => if let Some(v) = lit {
                    let decls = parse_style_text(v).into_iter().map(|(k, v)| {
                        let (v, important) = split_important(&v);
                        format!("[{}, {}, {important}]", js_string(&k), js_string(js_trim(v)))
                    }).collect::<Vec<_>>();
                    data.push(format!("styleDecls: [{}]", decls.join(", ")));
                },
                "slot" if is_estree_expression(&a.value) => data.push(format!("slotAssignment: {}", self.attr_value_at(&a.value, bag_indent)?)),
                "slot" => data.push(format!("slotAssignment: {}", js_string(lit.unwrap_or("")))),
                _ => {
                    let used = is_attribute(kind, name, &a.name);
                    let v = self.compute_attr(name, ns, &a.name, &a.value, used, !self.cur_cr, nest_indent)?;
                    attr_vs.push((js_string(&a.name), v));
                }
            }
        }
        // TS objectToAST (helpers.ts:35) / the props loop (index.ts:573): attrs, props and `on` keys
        // are always `t.literal`, i.e. quoted.
        if !attr_vs.is_empty() {
            data.push(format!("attrs: {}", obj(attr_vs.into_iter().map(|(k, v)| format!("{k}: {v}")).collect(), &inner_pad)));
        }
        // TS elementDataBag order: attrs, ref, props (properties, innerHTML, spread last), context, key, on.
        if let Some(r) = directives.iter().find_map(|d| match d { ElementDirective::Ref(r) => Some(r), _ => None }) {
            self.has_refs = true;
            data.push(format!("ref: {}", js_string(r)));
        }
        let inner = directives.iter().find_map(|d| match d { ElementDirective::InnerHtml(v) => Some(v), _ => None });
        let spread = directives.iter().find_map(|d| match d { ElementDirective::Spread(e) => Some(e), _ => None });
        let mut entries: Vec<String> = Vec::new();
        for p in props {
            // TS computeAttrValue on a property: keyed by the attribute name.
            let v = self.compute_attr(name, ns, &p.attribute_name, &p.value, is_attribute(kind, name, &p.attribute_name), !self.cur_cr, nest_indent)?;
            entries.push(format!("{}: {v}", js_string(&p.name)));
        }
        if let Some(v) = inner {
            // A custom-renderer element gets the raw value (the renderer sanitizes).
            let x = if needs_renderer { self.attr_value_at(v, bag_indent)? } else { self.sanitized_html(v, bag_indent)? };
            entries.push(format!("innerHTML: {x}"));
        }
        if let Some(e) = spread { entries.push(format!("...{}", self.expr_at(e, nest_indent)?)); }
        if !entries.is_empty() { data.push(format!("props: {}", obj(entries, &inner_pad))); }
        if inner.is_some() || directives.iter().any(|d| matches!(d, ElementDirective::Dom(_))) {
            data.push(format!("context: {{\n{pad}    lwc: {{\n{pad}      dom: \"manual\"\n{pad}    }}\n{pad}  }}"));
        }
        let key_dir = directives.iter().find_map(|d| match d { ElementDirective::Key(e) => Some(e), _ => None });
        let key = match key_dir {
            Some(e) => { let args = format!("{key}, {}", self.expr_at(e, bag_indent)?); self.call(ApiHelper::Key, &args) }
            None => standalone_key(key, slot_parent),
        };
        data.push(format!("key: {key}"));
        if !listeners.is_empty() { data.push(format!("on: {}", self.gen_event_listeners(listeners)?)); }
        // TS genDynamicEventListeners (lwc:on): raw value, then a null-prototype clone.
        if let Some(e) = directives.iter().find_map(|d| match d { ElementDirective::OnDirective(e) => Some(e), _ => None }) {
            let x = self.expr(e)?;
            data.push(format!("dynamicOnRaw: {x}"));
            data.push(format!("dynamicOn: {}", obj(vec!["__proto__: null".into(), format!("...{x}")], &inner_pad)));
        }
        // TS elementDataBag (index.ts:640): `svg: true` for any element in the SVG namespace.
        if ns == SVG_NS { data.push("svg: true".into()); }
        if needs_renderer {
            // TS: `renderer: renderer` data property + `renderer` lwc import.
            self.renderer = true;
            data.push("renderer: renderer".into());
        }
        // TS elementDataBag: `slotData` (lwc:slot-bind) after the renderer.
        if let Some(e) = directives.iter().find_map(|d| match d { ElementDirective::SlotBind(e) => Some(e), _ => None }) {
            data.push(format!("slotData: {}", self.expr(e)?));
        }
        self.cur_cr = outer_cr;
        // TS CSR ExternalComponent: plain api_element + `external: true` at the end of the databag.
        if kind == CrKind::External { data.push("external: true".into()); }
        Ok(obj(data, &pad))
    }

    /// TS genClassExpression: normalized with TEMPLATE_CLASS_NAME_OBJECT_BINDING.
    fn gen_class_expression(&mut self, v: &AttributeValue, indent: usize) -> Result<String, CodegenError> {
        let x = self.attr_value_at(v, indent)?;
        if !is_api_feature_enabled(ApiFeature::TemplateClassNameObjectBinding, self.api_version) { return Ok(x); }
        Ok(self.call(ApiHelper::NormalizeClassName, &x))
    }

    /// TS genEventListeners (codegen.ts:386-450), the `on` value for a databag or a static part: bind
    /// every handler in listener order, keyed like a JS object (a repeated name keeps its first
    /// position, last value). With a handler rooted at a local (iteration item, slot data) only the
    /// non-local handlers are memoized, one `_mN` each; otherwise the whole object is one `_mN`.
    fn gen_event_listeners(&mut self, listeners: &[EventListener]) -> Result<String, CodegenError> {
        let mut on: Vec<(String, String, bool)> = Vec::new();
        let mut has_local = false;
        for l in listeners {
            let local = self.is_local_root(&l.handler);
            has_local |= local;
            let b = self.gen_bind(&l.handler)?;
            let k = js_string(&l.name);
            match on.iter_mut().find(|e| e.0 == k) { Some(e) => { e.1 = b; e.2 = local; } None => on.push((k, b, local)) }
        }
        if has_local {
            let props: Vec<String> = on.into_iter().map(|(k, b, local)| format!("{k}: {}", if local { b } else { self.memoize(&b) })).collect();
            Ok(format!("{{ {} }}", props.join(", ")))
        } else {
            let o = format!("{{ {} }}", on.into_iter().map(|(k, b, _)| format!("{k}: {b}")).collect::<Vec<_>>().join(", "));
            Ok(self.memoize(&o))
        }
    }

    /// TS getMemoizationId + genEventListeners' memoize (codegen.ts:373-418): `_mN || ($ctx._mN = expr)`,
    /// with `_mN` destructured from `$ctx` at the top of `tmpl`.
    fn memoize(&mut self, expr: &str) -> String {
        let id = self.memo;
        self.memo += 1;
        format!("_m{id} || ($ctx._m{id} = {expr})")
    }

    /// TS isLocalIdentifier(getMemberExpressionRoot(handler)): the handler's root identifier is an
    /// iteration / slot-data local. Any other expression shape has no identifier root.
    fn is_local_root(&self, e: &Expression) -> bool {
        match e {
            Expression::Identifier(Identifier { name }) => self.locals.iter().any(|l| l == name),
            Expression::Member(m) => self.is_local_root(&m.object),
            _ => false,
        }
    }

    /// TS genBind.
    fn gen_bind(&mut self, handler: &Expression) -> Result<String, CodegenError> {
        let x = self.expr(handler)?;
        Ok(self.call(ApiHelper::Bind, &x))
    }

    /// TS transformChildren for an element's children argument.
    fn child_list(&mut self, children: &[ChildNode], ind: usize, slot_parent: Option<&str>) -> Result<String, CodegenError> {
        let pad = "  ".repeat(ind);
        Ok(match self.kids(children, ind + 1, slot_parent)? {
            Kids::Expr(e) => e,
            Kids::Arr(kids) if kids.is_empty() => "[]".to_string(),
            Kids::Arr(kids) => format!("[\n{}\n{pad}]", kids.iter().map(|k| format!("{pad}  {k}")).collect::<Vec<_>>().join(",\n")),
        })
    }

    /// TS transformElement(element, slotParentName). `None` for node kinds that are not elements.
    fn transform_element(&mut self, c: &ChildNode, ind: usize, slot_parent: Option<&str>) -> Result<Option<String>, CodegenError> {
        let el = match c {
            ChildNode::Element(e) => ElementView::new(&e.name, &e.namespace, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, CrKind::Element),
            ChildNode::LwcComponent(e) => ElementView::new(&e.name, &e.namespace, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, CrKind::LwcComponent),
            ChildNode::Component(e) => ElementView::new(&e.name, &e.namespace, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, CrKind::Component),
            ChildNode::ExternalComponent(e) => ElementView::new(&e.name, &e.namespace, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, CrKind::External),
            ChildNode::Slot(e) => {
                if !self.slots.contains(&e.slot_name) { self.slots.push(e.slot_name.clone()); }
                // TS elementDataBag keeps `name` in `attrs` (index.ts:556); slot_name only feeds api_slot.
                ElementView::new(&e.name, &e.namespace, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, CrKind::Element)
            }
            _ => return Ok(None),
        };
        // TODO [#4077] (lwc): TS gathers the databag before the static-element check and discards it on the
        // static path; its `$api` registrations and key are still observable, so do the same.
        let data = self.element_data_bag(&el, ind, slot_parent)?;
        if matches!(c, ChildNode::Element(_)) && self.static_opt && self.is_static(c) {
            return Ok(Some(self.static_fragment(c, slot_parent)?));
        }
        // TS transformChildren (index.ts:193): a `<slot>`'s element children get its slotName.
        let own_slot = match c { ChildNode::Slot(e) => Some(e.slot_name.as_str()), _ => None };
        let kids = self.child_list(el.children, ind, own_slot)?;
        // TS genElement/genCustomElement: the children argument is omitted when it is `[]`.
        let kids_arg = if kids == "[]" { String::new() } else { format!(", {kids}") };
        let head = js_string(el.name);
        Ok(Some(match c {
            ChildNode::Element(e) => match self.dynamic_ctor(&e.name, &e.directives, &data, &kids)? {
                Some(r) => r,
                None => self.call(ApiHelper::Element, &format!("{head}, {data}{kids_arg}")),
            },
            // TS genLwcComponent → api_dynamic_component(ctor, data, children).
            ChildNode::LwcComponent(e) => match self.dynamic_ctor(&e.name, &e.directives, &data, &kids)? {
                Some(r) => r,
                None => self.call(ApiHelper::Element, &format!("{head}, {data}{kids_arg}")),
            },
            // TS genCustomElement registers the import after transforming children (post-order).
            ChildNode::Component(e) => match self.dynamic_ctor(&e.name, &e.directives, &data, &kids)? {
                Some(r) => r,
                None => {
                    let b = self.component_binding(&e.name);
                    self.call(ApiHelper::CustomElement, &format!("{head}, {b}, {data}{kids_arg}"))
                }
            },
            // TS CSR: plain api_element + `external: true`; no custom-element import.
            ChildNode::ExternalComponent(_) => self.call(ApiHelper::Element, &format!("{head}, {data}{kids_arg}")),
            ChildNode::Slot(e) => self.call(ApiHelper::Slot, &format!("{}, {data}, {kids}, $slotset", js_string(&e.slot_name))),
            _ => unreachable!(),
        }))
    }

    /// TS transformConditionalParentBlock: each branch is `api_fragment(ifBlockKey, children, 0)`.
    fn branch(&mut self, cs: &[ChildNode], key: u32, ind: usize) -> Result<String, CodegenError> {
        let kids = match self.kids(cs, ind, None)? { Kids::Expr(e) => e, Kids::Arr(v) => format!("[{}]", v.join(", ")) };
        Ok(self.call(ApiHelper::Fragment, &format!("{key}, {kids}, 0")))
    }

    fn else_branch(&mut self, e: Option<&ElseBranch>, key: u32, ind: usize) -> Result<String, CodegenError> {
        Ok(match e {
            None => "null".into(),
            Some(ElseBranch::Else(b)) => self.branch(&b.children, key, ind)?,
            Some(ElseBranch::Elseif(b)) => {
                let body = self.branch(&b.children, key, ind)?;
                let alt = self.else_branch(b.else_.as_deref(), key, ind)?;
                format!("{} ? {body} : {alt}", self.expr(&b.condition)?)
            }
        })
    }

    fn loop_body(&mut self, cs: &[ChildNode], ind: usize) -> Result<String, CodegenError> {
        let pad = "  ".repeat(ind);
        // TS transformForBlock: a one-element children array is unwrapped.
        let ret = match self.kids(cs, ind + 1, None)? {
            Kids::Expr(e) => e,
            Kids::Arr(kids) => match kids.len() { 1 => kids.into_iter().next().unwrap(), _ => format!("[{}]", kids.join(", ")) },
        };
        Ok(format!("{pad}  return {ret};\n{pad}"))
    }

    fn node(&mut self, c: &ChildNode, ind: usize, slot_parent: Option<&str>) -> Result<Option<String>, CodegenError> {
        Ok(Some(match c {
            ChildNode::Text(t) => self.text_node(&[t])?,
            // TS transformComment → api_comment (only reached when comments are preserved).
            ChildNode::Comment(cm) if self.comments => self.call(ApiHelper::Comment, &js_string(&cm.value)),
            ChildNode::Element(_) | ChildNode::LwcComponent(_) | ChildNode::Component(_) | ChildNode::ExternalComponent(_) | ChildNode::Slot(_) => return Ok(self.transform_element(c, ind, slot_parent)?),
            // `if:*` children are spliced by `children` (TS transformIf); one alone in a slot is unreachable here.
            ChildNode::If(f) => { let v = self.inline_if(f, ind)?; return Ok((!v.is_empty()).then(|| v.join(", "))); }
            ChildNode::IfBlock(b) => {
                let key = self.key;
                self.key += 1;
                let body = self.branch(&b.children, key, ind)?;
                let alt = self.else_branch(b.else_.as_deref(), key, ind)?;
                format!("{} ? {body} : {alt}", self.expr(&b.condition)?)
            }
            ChildNode::ForEach(f) => {
                let list = self.expr(&f.expression)?;
                let mut params = vec![f.item.name.clone()];
                if let Some(i) = &f.index { params.push(i.name.clone()); }
                let n = self.locals.len();
                self.locals.extend(params.iter().cloned());
                let body = self.loop_body(&f.children, ind)?;
                self.locals.truncate(n);
                self.call(ApiHelper::Iterator, &format!("{list}, function ({}) {{\n{body}}}", params.join(", ")))
            }
            ChildNode::ForOf(f) => {
                // LWC shape: function (itValue, itIndex, itFirst, itLast) { const it = {...}; return ...; }
                let list = self.expr(&f.expression)?;
                let it = &f.iterator.name;
                let n = self.locals.len();
                self.locals.push(it.clone());
                let body = self.loop_body(&f.children, ind)?;
                self.locals.truncate(n);
                let pad = "  ".repeat(ind + 1);
                self.call(ApiHelper::Iterator, &format!("{list}, function ({it}Value, {it}Index, {it}First, {it}Last) {{\n{pad}const {it} = {{\n{pad}  value: {it}Value,\n{pad}  index: {it}Index,\n{pad}  first: {it}First,\n{pad}  last: {it}Last\n{pad}}};\n{body}}}"))
            }
            ChildNode::ScopedSlotFragment(f) => {
                // TS transformScopedSlotFragment: the slot-data identifier and `key` are bound locals
                // of `function (data, key) { return api_fragment(key, [...], 0); }`.
                let name = self.attr_value(&f.slot_name)?;
                let Expression::Identifier(Identifier { name: data }) = &f.slot_data else { return Ok(None) };
                let n = self.locals.len();
                self.locals.push(data.clone());
                self.locals.push("key".into());
                let pad = "  ".repeat(ind);
                let kids = self.kids(&f.children, ind + 2, None)?;
                self.locals.truncate(n);
                let kids = match kids {
                    Kids::Expr(e) => e,
                    Kids::Arr(kids) if kids.is_empty() => "[]".to_string(),
                    Kids::Arr(kids) => format!("[\n{}\n{pad}    ]", kids.iter().map(|k| format!("{pad}      {k}")).collect::<Vec<_>>().join(",\n")),
                };
                // TS registers fr (inner) before ssf (outer).
                let fr = self.call(ApiHelper::Fragment, &format!("\n{pad}    key,\n{pad}    {kids},\n{pad}    0\n{pad}  "));
                self.call(ApiHelper::ScopedSlotFactory, &format!("{name}, function ({data}, key) {{\n{pad}  return {fr};\n{pad}}}"))
            }
            // Other node kinds: future codegen cut.
            _ => return Ok(None),
        }))
    }
}

/// TS genKeyExpression without a `key` directive (codegen.ts:481-493): the numeric key, or for an
/// element whose parent is a `<slot>` (slotName may be "") the string `@${slotParentName}:${key}`.
fn standalone_key(key: u32, slot_parent: Option<&str>) -> String {
    match slot_parent {
        // astring prints the string Literal with JSON.stringify.
        Some(name) => lwc_shared::js_literal::json_string(format!("@{name}:{key}").encode_utf16()),
        None => key.to_string(),
    }
}

/// TS transformChildren result: a children array, or a single (possibly flattened) expression.
enum Kids { Arr(Vec<String>), Expr(String) }

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "keygen", "menuitem"]; // + DEPRECATED_VOID_ELEMENTS

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// TS `isSvgUseHref`.
fn is_svg_use_href(tag: &str, attr: &str, ns: &str) -> bool {
    (attr == "href" || attr == "xlink:href") && tag == "use" && ns == SVG_NS
}

/// @lwc/shared `ID_REFERENCING_ATTRIBUTES_SET`.
fn is_id_referencing(attr: &str) -> bool {
    matches!(attr, "aria-activedescendant" | "aria-controls" | "aria-describedby" | "aria-details" | "aria-errormessage"
        | "aria-flowto" | "aria-labelledby" | "aria-owns" | "for" | "popovertarget")
}

/// TS `isAllowedFragOnlyUrlsXHTML`.
fn is_frag_only_xhtml(tag: &str, attr: &str, ns: &str) -> bool { attr == "href" && (tag == "a" || tag == "area") && ns == HTML_NS }

/// TS `hasIdAttribute`: any element (attribute or property) named `id`, descending through `children`.
fn has_id_attribute(cs: &[ChildNode]) -> bool {
    cs.iter().any(|c| {
        let (attrs, props, kids): (&[Attribute], &[Property], &[ChildNode]) = match c {
            ChildNode::Element(e) => (&e.attributes, &e.properties, &e.children),
            ChildNode::Component(e) => (&e.attributes, &e.properties, &e.children),
            ChildNode::ExternalComponent(e) => (&e.attributes, &e.properties, &e.children),
            ChildNode::LwcComponent(e) => (&e.attributes, &e.properties, &e.children),
            ChildNode::Slot(e) => (&e.attributes, &e.properties, &e.children),
            ChildNode::If(f) => (&[], &[], &f.children),
            ChildNode::IfBlock(f) => (&[], &[], &f.children),
            ChildNode::ElseifBlock(f) => (&[], &[], &f.children),
            ChildNode::ElseBlock(f) => (&[], &[], &f.children),
            ChildNode::ForEach(f) => (&[], &[], &f.children),
            ChildNode::ForOf(f) => (&[], &[], &f.children),
            ChildNode::ScopedSlotFragment(f) => (&[], &[], &f.children),
            ChildNode::Text(_) | ChildNode::Comment(_) => (&[], &[], &[]),
        };
        attrs.iter().any(|a| a.name == "id") || props.iter().any(|p| p.name == "id") || has_id_attribute(kids)
    })
}

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
/// TS `rawContentElements` (static-element-serializer.ts).
const RAW_CONTENT: &[&str] = &["STYLE", "SCRIPT", "XMP", "IFRAME", "NOEMBED", "NOFRAMES", "PLAINTEXT"];

fn is_text_expr(c: &ChildNode) -> bool { matches!(c, ChildNode::Text(t) if text_literal(t).is_none()) }

/// TS `isStringLiteral(text.value)`: a literal text, or a `{'str'}` expression, is static text.
fn text_literal<'t>(t: &'t Text) -> Option<&'t str> {
    match &t.value {
        TextValue::Literal(s) | TextValue::Expression(Expression::Literal(Literal { value: LiteralValue::String(s) })) => Some(s),
        TextValue::Expression(Expression::Complex(c)) => match c.as_literal() {
            Some(LiteralView::String(s)) => Some(s),
            _ => None,
        },
        _ => None,
    }
}
fn is_void(name: &str, ns: &str) -> bool { ns == HTML_NS && VOID.contains(&name.to_ascii_lowercase().as_str()) }

/// TS `transformStaticChildren` output item: a run of contiguous text nodes, or any other node.
enum SChild<'a> { Texts(Vec<&'a Text<'a>>), Node(&'a ChildNode<'a>) }

/// TS `transformStaticChildren`: only elements with dynamic text group contiguous text nodes; non-preserved
/// comments are dropped without breaking the run.
fn static_children<'a>(e: &'a Element<'a>, comments: bool) -> Vec<SChild<'a>> {
    if !e.children.iter().any(is_text_expr) { return e.children.iter().map(SChild::Node).collect(); }
    let mut out: Vec<SChild> = Vec::new();
    let mut open = false;
    for c in &e.children {
        match c {
            ChildNode::Text(t) => {
                match out.last_mut() { Some(SChild::Texts(v)) if open => v.push(t), _ => out.push(SChild::Texts(vec![t])) }
                open = true;
            }
            ChildNode::Comment(_) if !comments => {}
            _ => { open = false; out.push(SChild::Node(c)); }
        }
    }
    out
}

/// TS `parseClassNames` (helpers.ts:219): split on `/\s+/`, drop empties. Duplicates are kept, like
/// the TS property list.
fn parse_class_names(v: &str) -> Vec<&str> { v.split(js_ws).filter(|n| !n.is_empty()).collect() }

fn tpl_escape(s: &str) -> String { s.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${") }
/// `@lwc/shared` `htmlEscape`: attr mode escapes `"&`, text mode `"'<>&`.
fn html_escape(s: &str, attr: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("&quot;"),
            '&' => o.push_str("&amp;"),
            '\'' if !attr => o.push_str("&#x27;"),
            '<' if !attr => o.push_str("&lt;"),
            '>' if !attr => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

struct Ser { comments: bool, part: i32, parts: Vec<(i32, Vec<String>, Option<String>)> }

impl Gen {
    /// TS `isStaticNode`.
    fn is_static_node(&self, e: &Element) -> bool {
        // TS isStaticNode: foreign-namespace elements are static only with LOWERCASE_SCOPE_TOKENS.
        if !is_api_feature_enabled(ApiFeature::LowercaseScopeTokens, self.api_version) && e.namespace != HTML_NS { return false; }
        // JS `isElement`: `<lwc:component>` is its own node kind (LwcComponent) there; the Rust parser
        // still lowers it to `Element`, so reject it by name.
        e.name != "iframe" && e.name != "lwc:component"
            && !e.attributes.iter().any(|a| a.name == "slot")
            && e.directives.iter().all(|d| matches!(d, ElementDirective::Key(_) | ElementDirective::Ref(_)))
            && e.properties.is_empty()
    }

    /// TS `collectStaticNodes`: text/comments are static; an element is static when it passes
    /// `isStaticNode`, needs no custom renderer hook, and all its children are static.
    fn is_static(&self, c: &ChildNode) -> bool {
        match c {
            ChildNode::Text(_) | ChildNode::Comment(_) => true,
            ChildNode::Element(e) => !self.needs_renderer(&e.name, &e.namespace, &e.attributes, &e.directives, CrKind::Element) && self.is_static_node(e)
                && e.children.iter().all(|k| self.is_static(k)),
            _ => false,
        }
    }

    /// TS `genConcatenatedText`.
    /// TS genText: `api_text("lit" + api_dynamic_text(x) + ...)`.
    fn text_node(&mut self, ts: &[&Text]) -> Result<String, CodegenError> {
        let x = self.concat_text(ts)?;
        Ok(self.call(ApiHelper::Text, &x))
    }

    fn concat_text(&mut self, ts: &[&Text]) -> Result<String, CodegenError> {
        let mut parts = Vec::new();
        for t in ts {
            parts.push(if let Some(s) = text_literal(t) {
                js_string(s)
            } else {
                match &t.value {
                    TextValue::Literal(s) => js_string(s),
                    TextValue::Expression(e) => {
                        let x = self.expr(e)?;
                        self.call(ApiHelper::DynamicText, &x)
                    }
                }
            });
        }
        Ok(parts.join(" + "))
    }

    fn static_text(t: &Text, parent: &str) -> String {
        match &t.value {
            TextValue::Literal(s) => {
                let v = if RAW_CONTENT.contains(&parent.to_ascii_uppercase().as_str()) { t.raw.clone() } else { html_escape(s, false) };
                tpl_escape(&v)
            }
            TextValue::Expression(Expression::Literal(Literal { value: LiteralValue::String(s) })) => tpl_escape(&html_escape(s, false)),
            TextValue::Expression(_) => unreachable!("dynamic text outside a grouped run"),
        }
    }

    /// Port of TS `serializeStaticElement` fused with `genStaticParts`: both walk the subtree in the
    /// same pre-order and bump `partId` for every node (text runs count once; comments only when preserved).
    fn serialize(&mut self, c: &SChild, parent: &str, ser: &mut Ser) -> Result<String, CodegenError> {
        Ok(match c {
            SChild::Texts(ts) => {
                ser.part += 1;
                if ts.iter().any(|t| text_literal(t).is_none()) {
                    let id = ser.part;
                    let text = self.concat_text(ts)?;
                    ser.parts.push((id, Vec::new(), Some(text)));
                    format!("${{{}}}", js_string(&format!("t{id}")))
                } else {
                    ts.iter().map(|t| Self::static_text(t, parent)).collect()
                }
            }
            SChild::Node(ChildNode::Text(t)) => { ser.part += 1; Self::static_text(t, parent) }
            SChild::Node(ChildNode::Comment(cm)) => {
                if !ser.comments { return Ok(String::new()); }
                ser.part += 1;
                format!("<!--{}-->", html_escape(&tpl_escape(&cm.value), false))
            }
            SChild::Node(ChildNode::Element(e)) => {
                ser.part += 1;
                let id = ser.part;
                let mut databag: Vec<String> = Vec::new();
                if !e.listeners.is_empty() { databag.push(format!("on: {}", self.gen_event_listeners(&e.listeners)?)); }
                for d in &e.directives {
                    if let ElementDirective::Ref(r) = d { self.has_refs = true; databag.push(format!("ref: {}", js_string(r))); }
                }
                let mut attrs: Vec<String> = Vec::new();
                let mut h = format!("<{}", tpl_escape(&e.name));
                let mut has_class = false;
                for a in &e.attributes {
                    let is_bool = matches!(a.value, AttributeValue::Literal(Literal { value: LiteralValue::Bool(_) }));
                    if (a.name == "class" || a.name == "style") && is_bool { continue; }
                    let is_expr = is_estree_expression(&a.value);
                    // TS genStaticParts: ids/idrefs and `<a|area href="#frag">` are parts under synthetic shadow.
                    let is_id = (a.name == "id" || is_id_referencing(&a.name)) && !is_bool;
                    let is_frag = self.scope_frag && is_frag_only_xhtml(&e.name, &a.name, &e.namespace)
                        && matches!(&a.value, AttributeValue::Literal(Literal { value: LiteralValue::String(v) }) if v.starts_with('#'));
                    let needs_scoping = self.synthetic && (is_id || is_frag);
                    let placeholder = is_expr || (is_svg_use_href(&e.name, &a.name, &e.namespace) && !is_bool) || needs_scoping;
                    if placeholder {
                        let token = match a.name.as_str() {
                            "style" => { databag.push(format!("style: {}", self.attr_value_at(&a.value, 1)?)); format!("s{id}") }
                            "class" => {
                                has_class = true;
                                let v = self.gen_class_expression(&a.value, 1)?;
                                databag.push(format!("className: {v}"));
                                format!("c{id}")
                            }
                            _ => {
                                // TS bindAttributeExpression (literals too: bindExpression keeps them as-is).
                                let v = self.bind_attr_expr(&e.name, &e.namespace, &a.name, &a.value, true, true, 1)?;
                                attrs.push(format!("{}: {v}", js_string(&a.name)));
                                format!("a{id}:{}", a.name)
                            }
                        };
                        h.push_str(&format!("${{{}}}", js_string(&token)));
                        continue;
                    }
                    let name = tpl_escape(&a.name);
                    let mut v = attr_string_literal(&a.value).map(str::to_string);
                    if let Some(s) = v.clone() {
                        if a.name == "class" {
                            let n = s.split_whitespace().collect::<Vec<_>>().join(" ");
                            if n.is_empty() { continue; }
                            has_class = true;
                            v = Some(format!("{}${{0}}", html_escape(&tpl_escape(&n), true)));
                            h.push_str(&format!(" {name}=\"{}\"", v.unwrap()));
                            continue;
                        }
                        if a.name == "spellcheck" { v = Some(if s.to_ascii_lowercase() == "false" { "false".into() } else { "true".into() }); }
                        if a.name == "style" {
                            let n = normalize_style_attribute_value(&s);
                            if n.is_empty() { continue; }
                            v = Some(n);
                        }
                    }
                    match v {
                        Some(s) if !s.is_empty() => h.push_str(&format!(" {name}=\"{}\"", html_escape(&tpl_escape(&s), true))),
                        _ => h.push_str(&format!(" {name}")),
                    }
                }
                if !attrs.is_empty() { databag.push(format!("attrs: {{ {} }}", attrs.join(", "))); }
                if !databag.is_empty() { ser.parts.push((id, databag, None)); }
                h.push_str(if has_class { "${2}" } else { "${3}" });
                if e.namespace != HTML_NS && e.children.is_empty() { h.push_str("/>"); return Ok(h); }
                h.push('>');
                for k in static_children(e, ser.comments) { h.push_str(&self.serialize(&k, &e.name, ser)?); }
                if !is_void(&e.name, &e.namespace) || !e.children.is_empty() { h.push_str(&format!("</{}>", tpl_escape(&e.name))); }
                h
            }
            SChild::Node(_) => unreachable!(),
        })
    }

    /// TS `genStaticElement`: hoist the serialized fragment; returns the `api_static_fragment(...)` call.
    fn static_fragment(&mut self, c: &ChildNode, slot_parent: Option<&str>) -> Result<String, CodegenError> {
        // TS genKeyExpression, after elementDataBag took the previous key.
        let key = self.key;
        self.key += 1;
        let mut ser = Ser { comments: self.comments, part: -1, parts: Vec::new() };
        let html = self.serialize(&SChild::Node(c), "", &mut ser)?;
        let svg = matches!(c, ChildNode::Element(e) if e.name != "svg" && e.namespace == SVG_NS);
        self.frags.push((html, svg));
        // TS genStaticElement: genStaticParts (sp) runs before genKeyExpression (k) and api_static_fragment (st).
        let parts: Vec<String> = ser.parts.iter().map(|(id, bag, text)| {
            let bag = if bag.is_empty() { "null".to_string() } else { format!("{{ {} }}", bag.join(", ")) };
            self.call(ApiHelper::StaticPart, &format!("{id}, {bag}, {}", text.as_deref().unwrap_or("null")))
        }).collect();
        let key = match c {
            ChildNode::Element(e) => match e.directives.iter().find_map(|d| match d { ElementDirective::Key(k) => Some(k), _ => None }) {
                Some(k) => { let args = format!("{key}, {}", self.expr(k)?); self.call(ApiHelper::Key, &args) }
                None => standalone_key(key, slot_parent),
            },
            _ => standalone_key(key, slot_parent),
        };
        let n = self.frags.len();
        if parts.is_empty() { return Ok(self.call(ApiHelper::StaticFragment, &format!("$fragment{n}, {key}"))); }
        Ok(self.call(ApiHelper::StaticFragment, &format!("$fragment{n}, {key}, [{}]", parts.join(", "))))
    }

    fn scoped_id(&mut self, v: String) -> String { if self.synthetic { self.call(ApiHelper::ScopedId, &v) } else { v } }
    fn scoped_frag_id(&mut self, v: String) -> String { if self.synthetic { self.call(ApiHelper::ScopedFragId, &v) } else { v } }
    fn sanitize_wrap(&mut self, tag: &str, ns: &str, attr: &str, v: String, hook: bool) -> String {
        if !hook { return v; }
        self.sanitize = true;
        format!("sanitizeAttribute({}, {}, {}, {v})", js_string(tag), js_string(ns), js_string(attr))
    }

    /// TS `computeAttrValue`: literal ids/idrefs/`#frag` hrefs are scoped, svg `<use href>` sanitized
    /// (`hook` = TS addLegacySanitizationHook), booleans used as attributes become `''`.
    fn compute_attr(&mut self, tag: &str, ns: &str, attr: &str, value: &AttributeValue, used: bool, hook: bool, indent: usize) -> Result<String, CodegenError> {
        match attr_value_kind(value) {
            AttrValueKind::Expression => self.bind_attr_expr(tag, ns, attr, value, used, hook, indent),
            AttrValueKind::StringLiteral(s) => {
                let lit = js_string(s);
                if attr == "id" { return Ok(self.scoped_id(lit)); }
                if attr == "spellcheck" { return Ok((s.to_lowercase() != "false").to_string()); }
                if !used && crate::parser::element_check::is_boolean_attribute(attr, tag) { return Ok("true".into()); }
                if is_id_referencing(attr) { return Ok(self.scoped_id(lit)); }
                if self.scope_frag && is_frag_only_xhtml(tag, attr, ns) && s.starts_with('#') { return Ok(self.scoped_frag_id(lit)); }
                if is_svg_use_href(tag, attr, ns) {
                    let v = if s.starts_with('#') { self.scoped_frag_id(lit) } else { lit };
                    return Ok(self.sanitize_wrap(tag, ns, attr, v, hook));
                }
                Ok(lit)
            }
            AttrValueKind::OtherLiteral(lit) => Ok(if used { "\"\"".into() } else { attr_literal_js(lit) }),
        }
    }

    /// TS `bindAttributeExpression`.
    fn bind_attr_expr(&mut self, tag: &str, ns: &str, attr: &str, value: &AttributeValue, used: bool, hook: bool, indent: usize) -> Result<String, CodegenError> {
        let x = self.attr_value_at(value, indent)?;
        if used && crate::parser::element_check::is_boolean_attribute(attr, tag) { return Ok(format!("{x} ? \"\" : null")); }
        if attr == "tabindex" { return Ok(self.call(ApiHelper::TabIndex, &x)); }
        if attr == "id" || is_id_referencing(attr) { return Ok(self.scoped_id(x)); }
        if self.scope_frag && is_frag_only_xhtml(tag, attr, ns) { return Ok(self.scoped_frag_id(x)); }
        if is_svg_use_href(tag, attr, ns) {
            let v = match value {
                AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) if !s.starts_with('#') => x,
                _ => self.scoped_frag_id(x),
            };
            return Ok(self.sanitize_wrap(tag, ns, attr, v, hook));
        }
        Ok(x)
    }

    /// TS `isCustomRendererHookRequired` / `shouldAddCustomRenderer`.
    fn needs_renderer(&self, tag: &str, ns: &str, attrs: &[Attribute], directives: &[ElementDirective], kind: CrKind) -> bool {
        let Some(cr) = &self.cr else { return false };
        if kind == CrKind::External && cr.directives.iter().any(|d| d == "lwc:external") { return true; }
        if kind == CrKind::Component || tag == "lwc:component" { return false; }
        if directives.iter().any(|d| cr.directives.iter().any(|c| c == directive_name(d))) { return true; }
        // TS crElmToConfigMap: last config for a tag name wins.
        let Some(el) = cr.elements.iter().rev().find(|e| e.tag_name == tag) else { return false };
        if el.namespace.as_deref().is_some_and(|n| !n.is_empty() && n != ns) { return false; }
        let want = el.attributes.as_deref().unwrap_or(&[]);
        want.is_empty() || attrs.iter().any(|a| want.iter().any(|w| *w == a.name))
    }
}

use crate::parser::{is_attribute, ElementKind as CrKind};

/// TS BaseElement fields shared by Element, Component, ExternalComponent, LwcComponent and Slot.
#[derive(Clone, Copy)]
/// One element node as codegen reads it. `ns` is the node's own namespace (TS `element.namespace`),
/// never state carried over from a previously generated element.
struct ElementView<'a> {
    name: &'a str, ns: &'a str, attrs: &'a [Attribute<'a>], props: &'a [Property<'a>], directives: &'a [ElementDirective<'a>],
    listeners: &'a [EventListener<'a>], children: &'a [ChildNode<'a>], kind: CrKind,
}

impl<'a> ElementView<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(name: &'a str, ns: &'a str, attrs: &'a [Attribute], props: &'a [Property], directives: &'a [ElementDirective], listeners: &'a [EventListener], children: &'a [ChildNode], kind: CrKind) -> Self {
        ElementView { name, ns, attrs, props, directives, listeners, children, kind }
    }
}

/// TS `ElementDirectiveName`.
fn directive_name(d: &ElementDirective) -> &'static str {
    match d {
        ElementDirective::Key(_) => "key",
        ElementDirective::Dynamic(_) => "lwc:dynamic",
        ElementDirective::Is(_) => "lwc:is",
        ElementDirective::Dom(_) => "lwc:dom",
        ElementDirective::Spread(_) => "lwc:spread",
        ElementDirective::OnDirective(_) => "lwc:on",
        ElementDirective::InnerHtml(_) => "lwc:inner-html",
        ElementDirective::Ref(_) => "lwc:ref",
        ElementDirective::SlotBind(_) => "lwc:slot-bind",
        ElementDirective::SlotData(_) => "lwc:slot-data",
        ElementDirective::External => "lwc:external",
    }
}

pub fn generate(root: &Root, config: &NormalizedConfig, filename: &str, tokens: &crate::scope_tokens::ScopeTokens) -> Result<String, CodegenError> {
    let mut g = Gen { static_opt: config.enable_static_content_optimization, comments: config.preserve_html_comments || root.directives.iter().any(|d| matches!(d, RootDirective::PreserveComments(true))), cr: config.custom_renderer_config.clone(), api_version: config.api_version,
        light: root.directives.iter().any(|d| matches!(d, RootDirective::RenderMode(RenderMode::Light))), scope_frag: has_id_attribute(&root.children), ..Default::default() };
    // TS CodeGen.isSyntheticShadow.
    g.synthetic = !g.light && !config.disable_synthetic_shadow_support;
    // TS transform(): `return transformChildren(codeGen.root)`.
    let mut root_expr: Option<String> = None;
    let items = match g.kids(&root.children, 2, None)? { Kids::Arr(v) => v, Kids::Expr(e) => { root_expr = Some(e); Vec::new() } };
    let fragments = std::mem::take(&mut g.frags);
    let base = std::path::Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let mut out = format!("import _implicitStylesheets from {};\nimport _implicitScopedStylesheets from {};\n", js_string(&format!("./{base}.css")), js_string(&format!("./{base}.scoped.css?scoped=true")));
    for (b, m) in &g.imports { out.push_str(&format!("import {b} from {};\n", js_string(m))); }
    let mut lwc = vec!["freezeTemplate"];
    if fragments.iter().any(|(_, svg)| !svg) { lwc.push("parseFragment"); }
    if fragments.iter().any(|(_, svg)| *svg) { lwc.push("parseSVGFragment"); }
    lwc.push("registerTemplate");
    if g.renderer { lwc.push("renderer"); }
    if g.sanitize { lwc.push("sanitizeAttribute"); }
    // astring ImportDeclaration / ObjectPattern (astring.mjs ImportDeclaration, ObjectPattern): `{a, b}`, no inner spaces.
    out.push_str(&format!("import {{{}}} from \"lwc\";\n", lwc.join(", ")));
    for (i, (f, svg)) in fragments.iter().enumerate() {
        out.push_str(&format!("const $fragment{} = {}`{f}`;\n", i + 1, if *svg { "parseSVGFragment" } else { "parseFragment" }));
    }
    // astring ArrayExpression: `[a, b]` on one line, no trailing comma.
    let ret = root_expr.unwrap_or_else(|| format!("[{}]", items.join(", ")));
    // TS format(): optimizeStaticExpressions(templateFn); the `stcN` consts follow the hoisted
    // `$fragmentN` nodes and precede `function tmpl`.
    let mut stcs = Vec::new();
    let ret = optimize::hoist_static_expressions(&ret, &mut stcs);
    for (n, (_, v)) in stcs.iter().enumerate() { out.push_str(&format!("const stc{n} = {v};\n")); }
    out.push_str("function tmpl($api, $cmp, $slotset, $ctx) {\n");
    // TS CodeGen.usedApis: `$api` helpers in first-use order of the codegen traversal.
    let helpers: Vec<String> = g.apis.iter().map(|h| { let (a, n) = h.names(); format!("{a}: {n}") }).collect();
    if !helpers.is_empty() { out.push_str(&format!("  const {{{}}} = $api;\n", helpers.join(", "))); }
    // TS index.ts:694: `const { _m0, ... } = $ctx;` after the `$api` destructuring.
    if g.memo > 0 { out.push_str(&format!("  const {{{}}} = $ctx;\n", (0..g.memo).map(|i| format!("_m{i}")).collect::<Vec<_>>().join(", "))); }
    out.push_str(&format!("  return {ret};\n"));
    // index.ts:713-716: the function body BlockStatement carries `trailingComments: [LWC_VERSION_COMMENT]`;
    // astring BlockStatement (astring.mjs:302-306) prints it via formatComments at the statement indent,
    // after the last statement and before the closing brace.
    out.push_str(&format!("  /*{}*/\n}}\n", lwc_shared::meta::LWC_VERSION_COMMENT));
    out.push_str("export default registerTemplate(tmpl);\n");
    if !g.slots.is_empty() {
        // TS generateTemplateMetadata (helpers.ts:97-99): `Array.from(slotNames).sort()`.
        g.slots.sort_by(|a, b| lwc_shared::js::js_default_sort_cmp(a, b));
        out.push_str(&format!("tmpl.slots = [{}];\n", g.slots.iter().map(|s| js_string(s)).collect::<Vec<_>>().join(", ")));
    }
    // TS generateTemplateMetadata: slots, renderMode (light only), hasRefs, stylesheets, token.
    if g.light { out.push_str("tmpl.renderMode = \"light\";\n"); }
    if g.has_refs { out.push_str("tmpl.hasRefs = true;\n"); }
    // TODO: implicit `./{base}.css` + `./{base}.scoped.css?scoped=true` imports and push blocks (TS always emits them).
    out.push_str("tmpl.stylesheets = [];\n");
    // TS generateStylesheetTokens: one assignment, picked by LOWERCASE_SCOPE_TOKENS.
    let token = if is_api_feature_enabled(ApiFeature::LowercaseScopeTokens, config.api_version) { &tokens.scope_token } else { &tokens.legacy_scope_token };
    out.push_str(&format!("tmpl.stylesheetToken = {};\n", js_string(token)));
    out.push_str("if (_implicitStylesheets) {\n  tmpl.stylesheets.push.apply(tmpl.stylesheets, _implicitStylesheets);\n}\nif (_implicitScopedStylesheets) {\n  tmpl.stylesheets.push.apply(tmpl.stylesheets, _implicitScopedStylesheets);\n}\n");
    out.push_str("freezeTemplate(tmpl);\n");
    Ok(out)
}
