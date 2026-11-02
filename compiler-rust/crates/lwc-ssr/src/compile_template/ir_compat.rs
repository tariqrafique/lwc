//! SSR-side shim over Template IR gaps (see the Template handoff in the port report). The Rust SHIM(template-gap:ir-compat-walk)
//! parser leaves `lwc:*` element directives as plain attributes, keeps `<lwc:component>` as an
//! Element, and splits component attributes/properties with a looser rule than template-compiler's
//! `isAttribute` + `attributeToPropertyName`. This pass rebuilds the JS IR shape before emission:
//! - `lwc:inner-html|slot-bind|dom|spread|on|dynamic|is` attributes → ElementDirective;
//! - Element `lwc:component` → LwcComponent;
//! - Component / LwcComponent attributes vs properties re-split per JS isAttribute (source order).
//! Remove once lwc-template emits these natively.
use lwc_shared::template_ir::*;

// SHIM(template-gap:ir-compat-walk): IR re-shape driver for lwc-directive-attrs, lwc-component-node, component-prop-split, clean-text-node, empty-ns-prefix
pub(crate) fn normalize_root(root: &Root) -> Root {
    let mut r = root.clone();
    normalize_children(&mut r.children);
    r
}

// SHIM(template-gap:ir-compat-walk): per-children recursion for the normalize_root shims
fn normalize_children(children: &mut Vec<ChildNode>) {
    // SHIM(template-gap:clean-text-node): attribute-live-bindings, slots, slots-fallback, dynamic-slots, wire/deep-reference: re-clean text per JS cleanTextNode
    reclean_texts(children);
    for c in children.iter_mut() { normalize_node(c); }
}

/// template-compiler `cleanTextNode` (babel JSX literal cleaning). lwc-template trims the whole
/// text node instead, losing e.g. the trailing space in `Checked: <input>` and collapsing
/// `a:\n    {b}` differently. Recompute literal segments from `Text.raw` per JS semantics.
// SHIM(template-gap:clean-text-node): port of babel cleanTextNode; lwc-template clean_text trims the whole node
fn js_clean_text_node(value: &str) -> String {
    let norm = value.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = norm.split('\n').collect();
    let last_non_empty = lines.iter().rposition(|l| l.chars().any(|c| c != ' ' && c != '\t')).unwrap_or(0);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let mut t = line.replace('\t', " ");
        if i != 0 { t = t.trim_start_matches(' ').to_string(); }
        if i != lines.len() - 1 { t = t.trim_end_matches(' ').to_string(); }
        if !t.is_empty() {
            if i != last_non_empty { t.push(' '); }
            out.push_str(&t);
        }
    }
    out
}

/// Same brace-balanced split lwc-template uses, so segments align 1:1 with its Text nodes.
// SHIM(template-gap:clean-text-node): mirror of lwc-template split_mustache so re-cleaned segments align with its Text nodes
fn split_mustache(s: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c != '{' { lit.push(c); continue; }
        let mut depth = 1;
        let mut end = None;
        for (j, cj) in s[i + 1..].char_indices() {
            match cj { '{' => depth += 1, '}' => { depth -= 1; if depth == 0 { end = Some(i + 1 + j); break; } } _ => {} }
        }
        match end {
            Some(e) => {
                if !lit.is_empty() { out.push((std::mem::take(&mut lit), false)); }
                out.push((s[i..=e].to_string(), true));
                while matches!(it.peek(), Some(&(k, _)) if k <= e) { it.next(); }
            }
            None => lit.push(c),
        }
    }
    if !lit.is_empty() { out.push((lit, false)); }
    out
}

// SHIM(template-gap:clean-text-node): rewrites Text literal segments from Text.raw (Checked: <input> trailing space, a:\n{b})
fn reclean_texts(children: &mut [ChildNode]) {
    let mut i = 0;
    while i < children.len() {
        let (raw, loc) = match &children[i] { ChildNode::Text(t) => (t.raw.clone(), t.location.clone()), _ => { i += 1; continue; } };
        let mut j = i;
        while j < children.len() && matches!(&children[j], ChildNode::Text(t) if t.raw == raw && t.location == loc) { j += 1; }
        let rust = split_mustache(raw.trim_matches(|c: char| c.is_ascii_whitespace()));
        let js = split_mustache(&js_clean_text_node(&raw));
        if rust.len() == j - i && js.len() == rust.len() && js.iter().zip(&rust).all(|(a, b)| a.1 == b.1) {
            for (k, (seg, is_expr)) in js.into_iter().enumerate() {
                if is_expr { continue; }
                if let ChildNode::Text(t) = &mut children[i + k] {
                    if matches!(t.value, TextValue::Literal(_)) { t.value = TextValue::Literal(seg); }
                }
            }
        }
        i = j;
    }
}

// SHIM(template-gap:ir-compat-walk): recursion into lwc:elseif/lwc:else branches only (IR else chain already matches JS)
fn normalize_else(e: &mut Option<Box<ElseBranch>>) {
    if let Some(b) = e {
        match b.as_mut() {
            ElseBranch::Else(x) => normalize_children(&mut x.children),
            ElseBranch::Elseif(x) => { normalize_children(&mut x.children); normalize_else(&mut x.else_); }
        }
    }
}

// SHIM(template-gap:ir-compat-walk): per-node dispatch for the normalize_root shims
fn normalize_node(n: &mut ChildNode) {
    if let ChildNode::Element(e) = n {
        // SHIM(template-gap:lwc-component-node): dynamic-components, attribute-class, dynamic-imports, read-only-props: <lwc:component> arrives as Element
        if e.name == "lwc:component" {
            let e = std::mem::take(e);
            *n = ChildNode::LwcComponent(LwcComponent { name: e.name, namespace: e.namespace, attributes: e.attributes, properties: e.properties,
                directives: e.directives, listeners: e.listeners, children: e.children, location: e.location });
        }
    }
    match n {
        ChildNode::Element(e) => {
            // Template handoff (g): lwc-template qualifies foreign attrs as `{prefix}:{name}` even when
            // parse5's prefix is empty, so `<svg xmlns>` arrives as `:xmlns`.
            // SHIM(template-gap:empty-ns-prefix): svgs: <svg xmlns> arrives as attribute `:xmlns`
            for a in e.attributes.iter_mut() { if let Some(n) = a.name.strip_prefix(':') { a.name = n.to_string(); } }
            // SHIM(template-gap:lwc-directive-attrs): lwc-inner-html, lwc-dom-manual, lwc-on, lwc-dynamic, inner-outer-html: lwc:* left in Element.attributes
            lift_directives(&mut e.attributes, &mut e.directives);
            normalize_children(&mut e.children);
        }
        // SHIM(template-gap:lwc-directive-attrs): directive-external: lwc:* left in ExternalComponent.attributes
        ChildNode::ExternalComponent(e) => { lift_directives(&mut e.attributes, &mut e.directives); normalize_children(&mut e.children); }
        // SHIM(template-gap:lwc-directive-attrs): scoped-slots, slot-forwarding, context-slotted: lwc:slot-bind left in Slot.attributes
        ChildNode::Slot(e) => { lift_directives(&mut e.attributes, &mut e.directives); normalize_children(&mut e.children); }
        ChildNode::Component(e) => {
            // SHIM(template-gap:lwc-directive-attrs): inner-outer-html (lwc:spread on x-*): lwc:* left in Component.attributes
            lift_directives(&mut e.attributes, &mut e.directives);
            // SHIM(template-gap:component-prop-split): attribute-component-aria, attribute-component-global-html, read-only-props: attrs/props split differs from JS isAttribute
            resplit_component(&mut e.attributes, &mut e.properties);
            normalize_children(&mut e.children);
        }
        ChildNode::LwcComponent(e) => {
            // SHIM(template-gap:lwc-directive-attrs): dynamic-components: lwc:is / lwc:slot-bind left in attributes
            lift_directives(&mut e.attributes, &mut e.directives);
            // SHIM(template-gap:component-prop-split): dynamic-components, read-only-props: lwc:component attrs never split into properties
            resplit_component(&mut e.attributes, &mut e.properties);
            normalize_children(&mut e.children);
        }
        ChildNode::ForEach(f) => normalize_children(&mut f.children),
        ChildNode::ForOf(f) => normalize_children(&mut f.children),
        ChildNode::If(f) => normalize_children(&mut f.children),
        ChildNode::IfBlock(b) => { normalize_children(&mut b.children); normalize_else(&mut b.else_); }
        ChildNode::ElseifBlock(b) => { normalize_children(&mut b.children); normalize_else(&mut b.else_); }
        ChildNode::ElseBlock(b) => normalize_children(&mut b.children),
        ChildNode::ScopedSlotFragment(s) => normalize_children(&mut s.children),
        ChildNode::Text(_) | ChildNode::Comment(_) => {}
    }
}

// SHIM(template-gap:lwc-directive-attrs): helper for lift_directives
fn as_expr(v: &AttributeValue) -> Option<Expression> {
    match v { AttributeValue::Expression(e) => Some(e.clone()), AttributeValue::Literal(_) => None }
}

// SHIM(template-gap:lwc-directive-attrs): lwc:inner-html|slot-bind|spread|on|dynamic|is|dom attributes -> ElementDirective
fn lift_directives(attrs: &mut Vec<Attribute>, dirs: &mut Vec<ElementDirective>) {
    attrs.retain(|a| {
        let d = match a.name.as_str() {
            "lwc:inner-html" => Some(ElementDirective::InnerHtml(a.value.clone())),
            "lwc:slot-bind" => as_expr(&a.value).map(ElementDirective::SlotBind),
            "lwc:spread" => as_expr(&a.value).map(ElementDirective::Spread),
            "lwc:on" => as_expr(&a.value).map(ElementDirective::OnDirective),
            "lwc:dynamic" => as_expr(&a.value).map(ElementDirective::Dynamic),
            "lwc:is" => as_expr(&a.value).map(ElementDirective::Is),
            "lwc:dom" => match &a.value { AttributeValue::Literal(l) => Some(ElementDirective::Dom(l.value.clone())), _ => None },
            _ => return true,
        };
        if let Some(d) = d { dirs.push(d); }
        false
    });
}

/// Template handoff (f): lwc-template lowers a valueless attribute (`<input hidden>`) to the
/// string literal `""`; template-compiler makes it the boolean literal `true` (attribute.ts
/// `normalizeAttributeValue`: no `=` in the raw attribute source). Recover that from the source.
// SHIM(template-gap:valueless-attr-bool): global-html-attributes, known-boolean-attributes, attribute-boolean: `<input hidden>` is "" not true (source rescan)
pub(crate) fn mark_valueless_attrs(root: &mut Root, src: &str) {
    fn fix(v: &mut AttributeValue, loc: &SourceLocation, src: &str) {
        if !matches!(v, AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) if s.is_empty()) { return; }
        let raw: Option<String> = if src.is_ascii() { src.get(loc.start as usize..loc.end as usize).map(str::to_string) }
            else { Some(src.chars().skip(loc.start as usize).take(loc.end.saturating_sub(loc.start) as usize).collect()) };
        if matches!(raw, Some(r) if !r.is_empty() && !r.contains('=')) { *v = AttributeValue::Literal(Literal { value: LiteralValue::Bool(true) }); }
    }
    for_each_attr_value(&mut root.children, &mut |v, loc| fix(v, loc, src));
}

/// Template handoff (h): under `experimentalComplexExpressions` template-compiler parses a quoted
/// `attr="{expr}"` and a non-binding `{expr}` text segment as (complex) expressions; lwc-template
/// keeps both as string literals. Lift them to `Expression::Complex` (bound by bind_expr).
// SHIM(template-gap:complex-expr-literal): attribute-dynamic-complex, text-interpolation-complex, scoped-slots/complex: `{expr}` kept as string literal
pub(crate) fn lift_quoted_complex(root: &mut Root) {
    fn inner(s: &str) -> Option<String> {
        let i = s.strip_prefix('{')?.strip_suffix('}')?;
        Some(i.trim().to_string())
    }
    for_each_attr_value(&mut root.children, &mut |v, _| {
        if let AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) = v {
            if let Some(i) = inner(s) { *v = AttributeValue::Expression(Expression::Complex(i)); }
        }
    });
    for_each_node(&mut root.children, &mut |n| {
        if let ChildNode::Text(t) = n {
            if let TextValue::Literal(s) = &t.value {
                if let Some(i) = inner(s) { t.value = TextValue::Expression(Expression::Complex(i)); }
            }
        }
    });
}

/// template-compiler parser: `parseChildren` wraps each child in `ctx.withErrorRecovery`, so a node whose
/// expression throws (LWC1210 INVALID_NODE_CTE_API_VERSION etc.) is omitted with its subtree. With complex
/// expressions off, the Rust parser reports the error but keeps the node: the expression survives as a
/// `"{…}"` string literal on attributes / `lwc:slot-bind`, or as `Expression::Complex` on `lwc:if|elseif`.
/// Text nodes and `for:each` are already dropped by the parser.
// SHIM(template-gap:expr-error-recovery): drop nodes the JS parser's error recovery omits
pub(crate) fn drop_errored_expr_nodes(root: &mut Root) {
    fn raw_expr(v: &AttributeValue) -> bool {
        // expression.ts isExpression: `{` … `}` and longer than "{}".
        matches!(v, AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) if s.len() > 2 && s.starts_with('{') && s.ends_with('}'))
    }
    fn errored(attrs: &[Attribute], props: &[Property]) -> bool {
        attrs.iter().any(|a| raw_expr(&a.value)) || props.iter().any(|p| raw_expr(&p.value))
    }
    fn prune_else(e: &mut Option<Box<ElseBranch>>) {
        let drop = matches!(e.as_deref(), Some(ElseBranch::Elseif(x)) if matches!(x.condition, Expression::Complex(_)));
        if drop { *e = None; return; }
        if let Some(b) = e {
            match b.as_mut() {
                ElseBranch::Else(x) => prune(&mut x.children),
                ElseBranch::Elseif(x) => { prune(&mut x.children); prune_else(&mut x.else_); }
            }
        }
    }
    fn prune(children: &mut Vec<ChildNode>) {
        children.retain(|n| match n {
            ChildNode::Element(e) => !errored(&e.attributes, &e.properties),
            ChildNode::Component(e) => !errored(&e.attributes, &e.properties),
            ChildNode::ExternalComponent(e) => !errored(&e.attributes, &e.properties),
            ChildNode::LwcComponent(e) => !errored(&e.attributes, &e.properties),
            ChildNode::Slot(e) => !errored(&e.attributes, &e.properties),
            ChildNode::IfBlock(b) => !matches!(b.condition, Expression::Complex(_)),
            ChildNode::ElseifBlock(b) => !matches!(b.condition, Expression::Complex(_)),
            ChildNode::If(b) => !matches!(b.condition, Expression::Complex(_)),
            _ => true,
        });
        for n in children.iter_mut() {
            match n {
                ChildNode::Element(e) => prune(&mut e.children),
                ChildNode::ExternalComponent(e) => prune(&mut e.children),
                ChildNode::Component(e) => prune(&mut e.children),
                ChildNode::LwcComponent(e) => prune(&mut e.children),
                ChildNode::Slot(e) => prune(&mut e.children),
                ChildNode::ForEach(x) => prune(&mut x.children),
                ChildNode::ForOf(x) => prune(&mut x.children),
                ChildNode::If(x) => prune(&mut x.children),
                ChildNode::IfBlock(b) => { prune(&mut b.children); prune_else(&mut b.else_); }
                ChildNode::ElseifBlock(b) => { prune(&mut b.children); prune_else(&mut b.else_); }
                ChildNode::ElseBlock(b) => prune(&mut b.children),
                ChildNode::ScopedSlotFragment(s) => prune(&mut s.children),
                ChildNode::Text(_) | ChildNode::Comment(_) => {}
            }
        }
    }
    prune(&mut root.children);
}

/// Pre-order visit of every node, including if/elseif/else branches.
// SHIM(template-gap:ir-compat-walk): shared walker for valueless-attr-bool, complex-expr-literal, complex-for-each
fn for_each_node(children: &mut [ChildNode], f: &mut dyn FnMut(&mut ChildNode)) {
    fn walk_else(e: &mut Option<Box<ElseBranch>>, f: &mut dyn FnMut(&mut ChildNode)) {
        if let Some(b) = e {
            match b.as_mut() {
                ElseBranch::Else(x) => for_each_node(&mut x.children, f),
                ElseBranch::Elseif(x) => { for_each_node(&mut x.children, f); walk_else(&mut x.else_, f); }
            }
        }
    }
    for n in children {
        f(n);
        match n {
            ChildNode::Element(e) => for_each_node(&mut e.children, f),
            ChildNode::ExternalComponent(e) => for_each_node(&mut e.children, f),
            ChildNode::Component(e) => for_each_node(&mut e.children, f),
            ChildNode::LwcComponent(e) => for_each_node(&mut e.children, f),
            ChildNode::Slot(e) => for_each_node(&mut e.children, f),
            ChildNode::ForEach(x) => for_each_node(&mut x.children, f),
            ChildNode::ForOf(x) => for_each_node(&mut x.children, f),
            ChildNode::If(x) => for_each_node(&mut x.children, f),
            ChildNode::IfBlock(b) => { for_each_node(&mut b.children, f); walk_else(&mut b.else_, f); }
            ChildNode::ElseifBlock(b) => { for_each_node(&mut b.children, f); walk_else(&mut b.else_, f); }
            ChildNode::ElseBlock(b) => for_each_node(&mut b.children, f),
            ChildNode::ScopedSlotFragment(s) => for_each_node(&mut s.children, f),
            ChildNode::Text(_) | ChildNode::Comment(_) => {}
        }
    }
}

/// Every attribute / property value on element-like nodes.
// SHIM(template-gap:ir-compat-walk): shared attr walker for valueless-attr-bool, complex-expr-literal
fn for_each_attr_value(children: &mut [ChildNode], f: &mut dyn FnMut(&mut AttributeValue, &SourceLocation)) {
    for_each_node(children, &mut |n| {
        let (a, p): (&mut [Attribute], &mut [Property]) = match n {
            ChildNode::Element(e) => (&mut e.attributes, &mut e.properties),
            ChildNode::ExternalComponent(e) => (&mut e.attributes, &mut e.properties),
            ChildNode::Component(e) => (&mut e.attributes, &mut e.properties),
            ChildNode::LwcComponent(e) => (&mut e.attributes, &mut e.properties),
            ChildNode::Slot(e) => (&mut e.attributes, &mut []),
            _ => return,
        };
        for x in a { f(&mut x.value, &x.location); }
        for x in p { f(&mut x.value, &x.location); }
    });
}

/// template-compiler isAttribute for Component / LwcComponent.
// SHIM(template-gap:component-prop-split): JS isAttribute for components (style|class|key|slot|exportparts|data-*)
fn is_component_attribute(name: &str) -> bool {
    matches!(name, "style" | "class" | "key" | "slot" | "exportparts") || name.starts_with("data-")
}

// SHIM(template-gap:component-prop-split): JS attributeToPropertyName special map
const SPECIAL_ATTR_TO_PROP: &[(&str, &str)] = &[
    ("accesskey", "accessKey"), ("readonly", "readOnly"), ("tabindex", "tabIndex"), ("bgcolor", "bgColor"), ("colspan", "colSpan"),
    ("rowspan", "rowSpan"), ("contenteditable", "contentEditable"), ("crossorigin", "crossOrigin"), ("datetime", "dateTime"),
    ("formaction", "formAction"), ("ismap", "isMap"), ("maxlength", "maxLength"), ("minlength", "minLength"),
    ("novalidate", "noValidate"), ("usemap", "useMap"), ("for", "htmlFor"),
];
// SHIM(template-gap:component-prop-split): JS AriaPropNameToAttrNameMap keys for attributeToPropertyName
const ARIA_PROPS: &[&str] = &["ariaActiveDescendant", "ariaAtomic", "ariaAutoComplete", "ariaBusy", "ariaChecked", "ariaColCount", "ariaColIndex",
    "ariaColIndexText", "ariaColSpan", "ariaControls", "ariaCurrent", "ariaDescribedBy", "ariaDescription", "ariaDetails", "ariaDisabled",
    "ariaErrorMessage", "ariaExpanded", "ariaFlowTo", "ariaHasPopup", "ariaHidden", "ariaInvalid", "ariaKeyShortcuts", "ariaLabel",
    "ariaLabelledBy", "ariaLevel", "ariaLive", "ariaModal", "ariaMultiLine", "ariaMultiSelectable", "ariaOrientation", "ariaOwns",
    "ariaPlaceholder", "ariaPosInSet", "ariaPressed", "ariaReadOnly", "ariaRelevant", "ariaRequired", "ariaRoleDescription", "ariaRowCount",
    "ariaRowIndex", "ariaRowIndexText", "ariaRowSpan", "ariaSelected", "ariaSetSize", "ariaSort", "ariaValueMax", "ariaValueMin",
    "ariaValueNow", "ariaValueText", "ariaBrailleLabel", "ariaBrailleRoleDescription", "role"];

/// template-compiler attributeToPropertyName.
// SHIM(template-gap:component-prop-split): JS attributeToPropertyName (lwc-template kebab_to_camel misses special/aria names)
pub(crate) fn attribute_to_property_name(attr: &str) -> String {
    if let Some((_, p)) = SPECIAL_ATTR_TO_PROP.iter().find(|(a, _)| *a == attr) { return (*p).to_string(); }
    if let Some(p) = ARIA_PROPS.iter().find(|p| p.replacen("aria", "aria-", 1).to_lowercase() == attr) { return (*p).to_string(); }
    let mut prop = String::new();
    let mut up = false;
    for c in attr.chars() {
        if c == '-' { up = true; } else { if up { prop.extend(c.to_uppercase()); } else { prop.push(c); } up = false; }
    }
    prop
}

// SHIM(template-gap:component-prop-split): re-split Component/LwcComponent attrs vs props in source order, map overwrite on duplicates
fn resplit_component(attrs: &mut Vec<Attribute>, props: &mut Vec<Property>) {
    let mut all: Vec<Attribute> = std::mem::take(attrs);
    all.extend(std::mem::take(props).into_iter().map(|p| Attribute { name: p.attribute_name, value: p.value, location: p.location }));
    all.sort_by_key(|a| a.location.start);
    for a in all {
        if is_component_attribute(&a.name) {
            attrs.push(a);
        } else {
            let name = attribute_to_property_name(&a.name);
            // Map semantics: a later duplicate property overwrites the value in place.
            if let Some(p) = props.iter_mut().find(|p| p.name == name) { p.value = a.value; p.attribute_name = a.name; p.location = a.location; }
            else { props.push(Property { name, attribute_name: a.name, value: a.value, location: a.location }); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prop_names() {
        assert_eq!(attribute_to_property_name("aria-labelledby"), "ariaLabelledBy");
        assert_eq!(attribute_to_property_name("tabindex"), "tabIndex");
        assert_eq!(attribute_to_property_name("foo-bar"), "fooBar");
        assert_eq!(attribute_to_property_name("role"), "role");
    }
}

/// Template handoff (h): with `experimentalComplexExpressions`, lwc-template drops a
/// `<template for:each="{list()}">` (or `={list()}`) whose value is not a plain member path
/// (warning 1153, children lost). Mask each such value with a same-length placeholder identifier SHIM(template-gap:complex-for-each)
/// (`{_$N}` + spaces) so the parser builds the ForEach; `unmask_complex_for_each` restores it.
/// Returns the masked source (byte- and char-length preserving) and `(placeholder, expr)` pairs. SHIM(template-gap:complex-for-each)
// SHIM(template-gap:complex-for-each): for-each-block/complex: for:each="{list()}" dropped with 1153
pub(crate) fn mask_complex_for_each(src: &str) -> (String, Vec<(String, String)>) { // SHIM(template-gap:complex-for-each)
    const KEY: &str = "for:each=";
    let mut out = String::with_capacity(src.len());
    let mut pairs = Vec::new();
    let mut at = 0;
    while let Some(rel) = src[at..].find(KEY) {
        let vstart = at + rel + KEY.len();
        out.push_str(&src[at..vstart]);
        at = vstart;
        let rest = &src[vstart..];
        let (quote, open) = match rest.as_bytes().first() {
            Some(b'"') | Some(b'\'') if rest.as_bytes().get(1) == Some(&b'{') => (Some(rest.as_bytes()[0] as char), 2),
            Some(b'{') => (None, 1),
            _ => continue,
        };
        let close = match quote { Some(q) => format!("}}{q}"), None => "}".to_string() };
        let Some(end) = rest[open..].find(&close) else { continue };
        let expr = &rest[open..open + end];
        let total = open + end + close.len();
        let simple = expr.trim().chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.');
        let ident = format!("_${}", pairs.len());
        if simple || !expr.is_ascii() || expr.contains('\n') || ident.len() + 2 > total {
            continue;
        }
        out.push('{');
        out.push_str(&ident);
        out.push('}');
        out.push_str(&" ".repeat(total - ident.len() - 2));
        pairs.push((ident, expr.trim().to_string()));
        at = vstart + total;
    }
    out.push_str(&src[at..]);
    (out, pairs)
}

// SHIM(template-gap:complex-for-each): restore the masked for:each expression as Expression::Complex
pub(crate) fn unmask_complex_for_each(root: &mut Root, pairs: &[(String, String)]) {
    if pairs.is_empty() { return; }
    for_each_node(&mut root.children, &mut |n| {
        if let ChildNode::ForEach(f) = n {
            if let Expression::Identifier(i) = &f.expression {
                if let Some((_, e)) = pairs.iter().find(|(p, _)| *p == i.name) {
                    f.expression = Expression::Complex(e.clone());
                }
            }
        }
    });
}
