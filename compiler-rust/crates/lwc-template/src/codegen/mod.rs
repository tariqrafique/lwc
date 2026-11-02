//! IR → ESM module source (behavioral goldens, not astring bytes).

use crate::config::NormalizedConfig;
use lwc_shared::template_ir::*;

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

/// `custom-else` → `customElse`.
pub(crate) fn camel_name(rest: &str) -> String {
    let mut out = String::new();
    for (i, seg) in rest.split('-').enumerate() {
        let mut cs = seg.chars();
        match (i, cs.next()) { (0, Some(f)) => { out.push(f); out.extend(cs); } (_, Some(f)) => { out.push(f.to_ascii_uppercase()); out.extend(cs); } _ => {} }
    }
    out
}

#[derive(Default)]
struct Gen { key: u32, text: bool, dyn_text: bool, element: bool, api_key: bool, iterator: bool, locals: Vec<String>, slot: bool, bind: bool, custom: bool, slots: Vec<String>, imports: Vec<(String, String)>, has_refs: bool, static_opt: bool, comments: bool, frags: Vec<(String, bool)>, st: bool, sp: bool, fragment: bool, ssf: bool, sanitize: bool, cr: Option<lwc_shared::CustomRendererConfig>, cur_cr: bool, renderer: bool, cur_ns: String, api_version: u32, ncls: bool, inner_html: u32, shc: bool, dc: bool, ddc: bool }

impl Gen {
    fn attr_value(&self, v: &AttributeValue) -> String {
        match v {
            AttributeValue::Literal(Literal { value: LiteralValue::String(s) }) => js_string(s),
            AttributeValue::Literal(Literal { value: LiteralValue::Bool(b) }) => b.to_string(),
            AttributeValue::Expression(e) => self.expr(e),
        }
    }

    fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::Identifier(Identifier { name }) if self.locals.iter().any(|l| l == name) => name.clone(),
            Expression::Identifier(Identifier { name }) => format!("$cmp.{name}"),
            Expression::Member(MemberExpression { object, property, computed: false }) => match &**property {
                Expression::Identifier(Identifier { name }) => format!("{}.{name}", self.expr(object)),
                p => format!("{}[{}]", self.expr(object), self.expr(p)),
            },
            Expression::Member(MemberExpression { object, property, .. }) => format!("{}[{}]", self.expr(object), self.expr(property)),
            Expression::Literal(Literal { value: LiteralValue::String(s) }) => js_string(s),
            Expression::Literal(Literal { value: LiteralValue::Bool(b) }) => b.to_string(),
            // TODO: complex expressions.
            Expression::Complex(_) => "undefined".into(),
        }
    }
}

fn prop_key(name: &str) -> String {
    let ident = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident { name.to_string() } else { js_string(name) }
}

impl Gen {
    fn children(&mut self, cs: &[ChildNode], ind: usize) -> Vec<String> {
        cs.iter().filter_map(|c| self.node(c, ind)).collect()
    }

    fn element(&mut self, e: &Element, ind: usize) -> String {
        let (head, data, kids) = self.parts(&e.name, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, ind, CrKind::Element);
        if let Some(r) = self.dynamic_ctor(&e.name, &e.directives, &data, &kids) { return r; }
        self.element = true;
        format!("api_element({head}, {data}, {kids})")
    }

    /// TS transformElement: `lwc:dynamic` → genDeprecatedDynamicElement, `lwc:is` → genDynamicElement
    /// (neither registers a component import; children only when non-empty).
    fn dynamic_ctor(&mut self, name: &str, directives: &[ElementDirective], data: &str, kids: &str) -> Option<String> {
        let tail = if kids == "[]" { String::new() } else { format!(", {kids}") };
        for d in directives {
            if let ElementDirective::Dynamic(e) = d {
                self.ddc = true;
                return Some(format!("api_deprecated_dynamic_component({}, {}, {data}{tail})", js_string(name), self.expr(e)));
            }
        }
        directives.iter().find_map(|d| match d { ElementDirective::Is(e) => Some(e), _ => None }).map(|e| {
            self.dc = true;
            format!("api_dynamic_component({}, {data}{tail})", self.expr(e))
        })
    }

    /// TS genSanitizedHtmlExpr (`$ctx._sanitizedHtml$N` / `$ctx._rawHtml$N` memo).
    fn sanitized_html(&mut self, v: &AttributeValue) -> String {
        let i = self.inner_html;
        self.inner_html += 1;
        self.shc = true;
        let x = self.attr_value(v);
        match v {
            AttributeValue::Literal(_) => format!("$ctx._sanitizedHtml${i} || ($ctx._sanitizedHtml${i} = api_sanitize_html_content({x}))"),
            AttributeValue::Expression(_) => format!("$ctx._rawHtml${i} !== ($ctx._rawHtml${i} = {x}) ? ($ctx._sanitizedHtml${i} = api_sanitize_html_content({x})) : $ctx._sanitizedHtml${i}"),
        }
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

    fn parts(&mut self, name: &str, attrs: &[Attribute], props: &[Property], directives: &[ElementDirective], listeners: &[EventListener], children: &[ChildNode], ind: usize, kind: CrKind) -> (String, String, String) {
        let key = self.key;
        self.key += 1;
        let pad = "  ".repeat(ind);
        let needs_renderer = self.needs_renderer(name, &self.cur_ns.clone(), attrs, directives, kind);
        let outer_cr = std::mem::replace(&mut self.cur_cr, needs_renderer);
        let mut data = String::from("{\n");
        if !attrs.is_empty() {
            data.push_str(&format!("{pad}  attrs: {{\n"));
            for a in attrs {
                let v = self.bound_attr(name, &self.cur_ns.clone(), a);
                data.push_str(&format!("{pad}    {}: {v},\n", prop_key(&a.name)));
            }
            data.push_str(&format!("{pad}  }},\n"));
        }
        // TS elementDataBag order: attrs, ref, props (properties, innerHTML, spread last), context, key, on.
        if let Some(r) = directives.iter().find_map(|d| match d { ElementDirective::Ref(r) => Some(r), _ => None }) {
            self.has_refs = true;
            data.push_str(&format!("{pad}  ref: {},\n", js_string(r)));
        }
        let inner = directives.iter().find_map(|d| match d { ElementDirective::InnerHtml(v) => Some(v), _ => None });
        let spread = directives.iter().find_map(|d| match d { ElementDirective::Spread(e) => Some(e), _ => None });
        let mut entries: Vec<String> = props.iter().map(|p| format!("{}: {}", prop_key(&p.name), self.attr_value(&p.value))).collect();
        if let Some(v) = inner {
            // A custom-renderer element gets the raw value (the renderer sanitizes).
            let x = if needs_renderer { self.attr_value(v) } else { self.sanitized_html(v) };
            entries.push(format!("innerHTML: {x}"));
        }
        if let Some(e) = spread { entries.push(format!("...{}", self.expr(e))); }
        if !entries.is_empty() {
            data.push_str(&format!("{pad}  props: {{\n"));
            for en in entries { data.push_str(&format!("{pad}    {en},\n")); }
            data.push_str(&format!("{pad}  }},\n"));
        }
        if inner.is_some() || directives.iter().any(|d| matches!(d, ElementDirective::Dom(_))) {
            data.push_str(&format!("{pad}  context: {{\n{pad}    lwc: {{\n{pad}      dom: \"manual\",\n{pad}    }},\n{pad}  }},\n"));
        }
        let key_dir = directives.iter().find_map(|d| match d { ElementDirective::Key(e) => Some(e), _ => None });
        let key = match key_dir {
            Some(e) => { self.api_key = true; format!("api_key({key}, {})", self.expr(e)) }
            None => key.to_string(),
        };
        data.push_str(&format!("{pad}  key: {key},\n"));
        if !listeners.is_empty() {
            // TODO: memoize handlers on $ctx (_m0) like LWC; inline api_bind for now.
            self.bind = true;
            data.push_str(&format!("{pad}  on: {{\n"));
            for l in listeners {
                data.push_str(&format!("{pad}    {}: api_bind({}),\n", prop_key(&l.name), self.expr(&l.handler)));
            }
            data.push_str(&format!("{pad}  }},\n"));
        }
        if needs_renderer {
            // TS: `renderer: renderer` data property + `renderer` lwc import.
            self.renderer = true;
            data.push_str(&format!("{pad}  renderer: renderer,\n"));
        }
        self.cur_cr = outer_cr;
        data.push_str(&format!("{pad}}}"));
        let kids = self.children(children, ind + 1);
        let kids = if kids.is_empty() { "[]".to_string() } else {
            format!("[\n{}\n{pad}]", kids.iter().map(|k| format!("{pad}  {k}")).collect::<Vec<_>>().join(",\n"))
        };
        (js_string(name), data, kids)
    }

    /// One or many children inside a conditional branch.
    fn branch(&mut self, cs: &[ChildNode], ind: usize) -> String {
        let kids = self.children(cs, ind);
        // TODO: LWC uses api_fragment for multi-child branches; emit array for now.
        match kids.len() { 0 => "null".into(), 1 => kids.into_iter().next().unwrap(), _ => format!("[{}]", kids.join(", ")) }
    }

    fn else_branch(&mut self, e: Option<&ElseBranch>, ind: usize) -> String {
        match e {
            None => "null".into(),
            Some(ElseBranch::Else(b)) => self.branch(&b.children, ind),
            Some(ElseBranch::Elseif(b)) => {
                let body = self.branch(&b.children, ind);
                let alt = self.else_branch(b.else_.as_deref(), ind);
                format!("{} ? {body} : {alt}", self.expr(&b.condition))
            }
        }
    }

    fn loop_body(&mut self, cs: &[ChildNode], ind: usize) -> String {
        let pad = "  ".repeat(ind);
        let kids = self.children(cs, ind + 1);
        let ret = match kids.len() { 0 => "[]".to_string(), 1 => kids.into_iter().next().unwrap(), _ => format!("[{}]", kids.join(", ")) };
        format!("{pad}  return {ret};\n{pad}")
    }

    fn node(&mut self, c: &ChildNode, ind: usize) -> Option<String> {
        Some(match c {
            ChildNode::Text(Text { value: TextValue::Literal(s), .. }) => { self.text = true; format!("api_text({})", js_string(s)) }
            ChildNode::Text(Text { value: TextValue::Expression(e), .. }) => {
                self.text = true; self.dyn_text = true;
                format!("api_text(api_dynamic_text({}))", self.expr(e))
            }
            ChildNode::Element(_) if self.static_opt && self.is_static(c) => {
                let key = self.key;
                self.key += 1;
                self.static_fragment(c, key.to_string())
            }
            ChildNode::Element(e) => { self.cur_ns = e.namespace.clone(); self.element(e, ind) }
            ChildNode::Component(e) => {
                // TS genCustomElement registers the import after transforming children (post-order).
                let (head, data, kids) = self.parts(&e.name, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, ind, CrKind::Component);
                if let Some(r) = self.dynamic_ctor(&e.name, &e.directives, &data, &kids) { return Some(r); }
                let b = self.component_binding(&e.name);
                self.custom = true;
                format!("api_custom_element({head}, {b}, {data}, {kids})")
            }
            ChildNode::ExternalComponent(e) => {
                // TS CSR: plain api_element + `external: true`; no custom-element import.
                self.cur_ns = e.namespace.clone();
                let (head, data, kids) = self.parts(&e.name, &e.attributes, &e.properties, &e.directives, &e.listeners, &e.children, ind, CrKind::External);
                let pad = "  ".repeat(ind);
                let data = format!("{}{pad}  external: true,\n{pad}}}", &data[..data.len() - pad.len() - 1]);
                self.element = true;
                format!("api_element({head}, {data}, {kids})")
            }
            ChildNode::Slot(e) => {
                if !self.slots.contains(&e.slot_name) { self.slots.push(e.slot_name.clone()); }
                // `name` is carried by slot_name; don't repeat it as an attr.
                let attrs: Vec<_> = e.attributes.iter().filter(|a| a.name != "name").cloned().collect();
                let (_, data, kids) = self.parts(&e.name, &attrs, &e.properties, &e.directives, &e.listeners, &e.children, ind, CrKind::Element);
                self.slot = true;
                format!("api_slot({}, {data}, {kids}, $slotset)", js_string(&e.slot_name))
            }
            ChildNode::If(f) => {
                let kids = self.children(&f.children, ind);
                // TODO: multiple children — LWC flattens/merges into the parent; emit array for now.
                let body = match kids.len() { 0 => return None, 1 => kids.into_iter().next().unwrap(), _ => format!("[{}]", kids.join(", ")) };
                let neg = if f.modifier == "false" { "!" } else { "" };
                format!("{neg}{} ? {body} : null", self.expr(&f.condition))
            }
            ChildNode::IfBlock(b) => {
                let body = self.branch(&b.children, ind);
                let alt = self.else_branch(b.else_.as_deref(), ind);
                format!("{} ? {body} : {alt}", self.expr(&b.condition))
            }
            ChildNode::ForEach(f) => {
                let list = self.expr(&f.expression);
                let mut params = vec![f.item.name.clone()];
                if let Some(i) = &f.index { params.push(i.name.clone()); }
                let n = self.locals.len();
                self.locals.extend(params.iter().cloned());
                let body = self.loop_body(&f.children, ind);
                self.locals.truncate(n);
                self.iterator = true;
                format!("api_iterator({list}, function ({}) {{\n{body}}})", params.join(", "))
            }
            ChildNode::ForOf(f) => {
                // LWC shape: function (itValue, itIndex, itFirst, itLast) { const it = {...}; return ...; }
                let list = self.expr(&f.expression);
                let it = &f.iterator.name;
                let n = self.locals.len();
                self.locals.push(it.clone());
                let body = self.loop_body(&f.children, ind);
                self.locals.truncate(n);
                self.iterator = true;
                let pad = "  ".repeat(ind + 1);
                format!("api_iterator({list}, function ({it}Value, {it}Index, {it}First, {it}Last) {{\n{pad}const {it} = {{\n{pad}  value: {it}Value,\n{pad}  index: {it}Index,\n{pad}  first: {it}First,\n{pad}  last: {it}Last,\n{pad}}};\n{body}}})")
            }
            ChildNode::ScopedSlotFragment(f) => {
                // TS transformScopedSlotFragment: the slot-data identifier and `key` are bound locals
                // of `function (data, key) { return api_fragment(key, [...], 0); }`.
                let name = self.attr_value(&f.slot_name);
                let Expression::Identifier(Identifier { name: data }) = &f.slot_data else { return None };
                let n = self.locals.len();
                self.locals.push(data.clone());
                self.locals.push("key".into());
                let pad = "  ".repeat(ind);
                let kids = self.children(&f.children, ind + 2);
                self.locals.truncate(n);
                let kids = if kids.is_empty() { "[]".to_string() } else {
                    format!("[\n{}\n{pad}    ]", kids.iter().map(|k| format!("{pad}      {k}")).collect::<Vec<_>>().join(",\n"))
                };
                self.fragment = true; self.ssf = true;
                format!("api_scoped_slot_factory({name}, function ({data}, key) {{\n{pad}  return api_fragment(\n{pad}    key,\n{pad}    {kids},\n{pad}    0\n{pad}  );\n{pad}}})")
            }
            // Other node kinds: future codegen cut.
            _ => return None,
        })
    }
}

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// TS `isSvgUseHref`.
fn is_svg_use_href(tag: &str, attr: &str, ns: &str) -> bool {
    (attr == "href" || attr == "xlink:href") && tag == "use" && ns == SVG_NS
}

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
/// `@lwc/shared` minApiVersion(LOWERCASE_SCOPE_TOKENS) = V59_246_WINTER_24 (enables static SVG).
const API_LOWERCASE_SCOPE_TOKENS: u32 = 59;
/// minApiVersion(TEMPLATE_CLASS_NAME_OBJECT_BINDING) = V62_252_WINTER_25.
const API_CLASS_NAME_OBJECT_BINDING: u32 = 62;
/// TS `rawContentElements` (static-element-serializer.ts).
const RAW_CONTENT: &[&str] = &["STYLE", "SCRIPT", "XMP", "IFRAME", "NOEMBED", "NOFRAMES", "PLAINTEXT"];

fn is_text_expr(c: &ChildNode) -> bool { matches!(c, ChildNode::Text(Text { value: TextValue::Expression(_) | TextValue::Complex(_), .. })) }
fn is_void(name: &str, ns: &str) -> bool { ns == HTML_NS && VOID.contains(&name.to_ascii_lowercase().as_str()) }

/// TS `transformStaticChildren` output item: a run of contiguous text nodes, or any other node.
enum SChild<'a> { Texts(Vec<&'a Text>), Node(&'a ChildNode) }

/// TS `transformStaticChildren`: only elements with dynamic text group contiguous text nodes; non-preserved
/// comments are dropped without breaking the run (Rust IR already omits them when not preserved).
fn static_children(e: &Element, comments: bool) -> Vec<SChild<'_>> {
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

/// TS `normalizeStyleAttributeValue` (`parseStyleText` + `key: value;` join).
fn normalize_style(style: &str) -> String {
    // DECLARATION_DELIMITER = /;(?![^(]*\))/g : a `;` not followed by `)` before any `(`.
    let b = style.as_bytes();
    let mut decls = Vec::new();
    let mut start = 0;
    for i in 0..b.len() {
        if b[i] != b';' { continue; }
        let rest = &style[i + 1..];
        let in_parens = rest.find(|c| c == '(' || c == ')').map_or(false, |j| rest.as_bytes()[j] == b')');
        if !in_parens { decls.push(&style[start..i]); start = i + 1; }
    }
    decls.push(&style[start..]);
    let mut map: Vec<(String, String)> = Vec::new();
    for d in decls {
        if d.is_empty() { continue; }
        let Some(j) = d.find(':') else { continue };
        let (k, v) = (d[..j].trim().to_string(), d[j + 1..].trim().to_string());
        if v.is_empty() && d[j + 1..].is_empty() { continue; } // `(.+)` needs ≥1 char after ':'
        match map.iter_mut().find(|(mk, _)| *mk == k) { Some(e) => e.1 = v, None => map.push((k, v)) }
    }
    map.into_iter().map(|(k, v)| {
        let lower = v.to_ascii_lowercase();
        let t = lower.trim_end();
        let v = if t.ends_with("important") {
            // IMPORTANT_FLAG = /\s*!\s*important\s*$/i
            let pre = t[..t.len() - "important".len()].trim_end();
            if pre.ends_with('!') { format!("{} !important", v[..pre.len() - 1].trim_end()) } else { v.clone() }
        } else { v.clone() };
        format!("{k}: {};", v.trim())
    }).collect::<Vec<_>>().join(" ")
}

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
        if self.api_version < API_LOWERCASE_SCOPE_TOKENS && e.namespace != HTML_NS { return false; }
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
    fn concat_text(&mut self, ts: &[&Text]) -> String {
        ts.iter().map(|t| match &t.value {
            TextValue::Literal(s) => js_string(s),
            TextValue::Expression(e) => { self.dyn_text = true; format!("api_dynamic_text({})", self.expr(e)) }
            TextValue::Complex(c) => { self.dyn_text = true; format!("api_dynamic_text({})", self.expr(&Expression::Complex(c.clone()))) }
        }).collect::<Vec<_>>().join(" + ")
    }

    fn static_text(t: &Text, parent: &str) -> String {
        match &t.value {
            TextValue::Literal(s) => {
                let v = if RAW_CONTENT.contains(&parent.to_ascii_uppercase().as_str()) { t.raw.clone() } else { html_escape(s, false) };
                tpl_escape(&v)
            }
            TextValue::Expression(_) | TextValue::Complex(_) => unreachable!("dynamic text outside a grouped run"),
        }
    }

    /// Port of TS `serializeStaticElement` fused with `genStaticParts`: both walk the subtree in the
    /// same pre-order and bump `partId` for every node (text runs count once; comments only when preserved).
    fn serialize(&mut self, c: &SChild, parent: &str, ser: &mut Ser) -> String {
        match c {
            SChild::Texts(ts) => {
                ser.part += 1;
                if ts.iter().any(|t| !matches!(t.value, TextValue::Literal(_))) {
                    let id = ser.part;
                    let text = self.concat_text(ts);
                    ser.parts.push((id, Vec::new(), Some(text)));
                    format!("${{{}}}", js_string(&format!("t{id}")))
                } else {
                    ts.iter().map(|t| Self::static_text(t, parent)).collect()
                }
            }
            SChild::Node(ChildNode::Text(t)) => { ser.part += 1; Self::static_text(t, parent) }
            SChild::Node(ChildNode::Comment(cm)) => {
                if !ser.comments { return String::new(); }
                ser.part += 1;
                format!("<!--{}-->", html_escape(&tpl_escape(&cm.value), false))
            }
            SChild::Node(ChildNode::Element(e)) => {
                ser.part += 1;
                let id = ser.part;
                let mut databag: Vec<String> = Vec::new();
                if !e.listeners.is_empty() {
                    // TODO: memoize handlers on $ctx (_m0) like LWC; inline api_bind (same as dynamic path).
                    self.bind = true;
                    let on = e.listeners.iter().map(|l| format!("{}: api_bind({})", js_string(&l.name), self.expr(&l.handler))).collect::<Vec<_>>().join(", ");
                    databag.push(format!("on: {{ {on} }}"));
                }
                for d in &e.directives {
                    if let ElementDirective::Ref(r) = d { self.has_refs = true; databag.push(format!("ref: {}", js_string(r))); }
                }
                let mut attrs: Vec<String> = Vec::new();
                let mut h = format!("<{}", tpl_escape(&e.name));
                let mut has_class = false;
                for a in &e.attributes {
                    let is_bool = matches!(a.value, AttributeValue::Literal(Literal { value: LiteralValue::Bool(_) }));
                    if (a.name == "class" || a.name == "style") && is_bool { continue; }
                    let is_expr = matches!(a.value, AttributeValue::Expression(_));
                    // TODO: synthetic-shadow id/idref + `#frag` href scoping parts (no api_scoped_id in Rust codegen yet).
                    let placeholder = is_expr || (is_svg_use_href(&e.name, &a.name, &e.namespace) && !is_bool);
                    if placeholder {
                        let token = match a.name.as_str() {
                            "style" => { databag.push(format!("style: {}", self.attr_value(&a.value))); format!("s{id}") }
                            "class" => {
                                has_class = true;
                                let mut v = self.attr_value(&a.value);
                                if self.api_version >= API_CLASS_NAME_OBJECT_BINDING { self.ncls = true; v = format!("api_normalize_class_name({v})"); }
                                databag.push(format!("className: {v}"));
                                format!("c{id}")
                            }
                            _ => { attrs.push(format!("{}: {}", js_string(&a.name), self.bound_attr(&e.name, &e.namespace, a))); format!("a{id}:{}", a.name) }
                        };
                        h.push_str(&format!("${{{}}}", js_string(&token)));
                        continue;
                    }
                    let name = tpl_escape(&a.name);
                    let mut v = match &a.value {
                        AttributeValue::Literal(Literal { value: LiteralValue::String(v) }) => Some(v.clone()),
                        _ => None,
                    };
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
                            let n = normalize_style(&s);
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
                if e.namespace != HTML_NS && e.children.is_empty() { h.push_str("/>"); return h; }
                h.push('>');
                for k in static_children(e, ser.comments) { h.push_str(&self.serialize(&k, &e.name, ser)); }
                if !is_void(&e.name, &e.namespace) || !e.children.is_empty() { h.push_str(&format!("</{}>", tpl_escape(&e.name))); }
                h
            }
            SChild::Node(_) => unreachable!(),
        }
    }

    /// TS `genStaticElement`: hoist the serialized fragment; returns the `api_static_fragment(...)` call.
    fn static_fragment(&mut self, c: &ChildNode, key: String) -> String {
        let mut ser = Ser { comments: self.comments, part: -1, parts: Vec::new() };
        let html = self.serialize(&SChild::Node(c), "", &mut ser);
        let svg = matches!(c, ChildNode::Element(e) if e.name != "svg" && e.namespace == SVG_NS);
        self.frags.push((html, svg));
        self.st = true;
        let key = match c {
            ChildNode::Element(e) => match e.directives.iter().find_map(|d| match d { ElementDirective::Key(k) => Some(k), _ => None }) {
                Some(k) => { self.api_key = true; format!("api_key({key}, {})", self.expr(k)) }
                None => key,
            },
            _ => key,
        };
        if ser.parts.is_empty() { return format!("api_static_fragment($fragment{}, {key})", self.frags.len()); }
        self.sp = true;
        let parts: Vec<String> = ser.parts.iter().map(|(id, bag, text)| {
            let bag = if bag.is_empty() { "null".to_string() } else { format!("{{ {} }}", bag.join(", ")) };
            format!("api_static_part({id}, {bag}, {})", text.as_deref().unwrap_or("null"))
        }).collect();
        format!("api_static_fragment($fragment{}, {key}, [{}])", self.frags.len(), parts.join(", "))
    }

    /// Attribute value incl. TS `sanitizeAttribute` wrap for svg `<use href>`.
    fn bound_attr(&mut self, tag: &str, ns: &str, a: &Attribute) -> String {
        let v = self.attr_value(&a.value);
        if is_svg_use_href(tag, &a.name, ns) && !self.cur_cr {
            self.sanitize = true;
            return format!("sanitizeAttribute({}, {}, {}, {v})", js_string(tag), js_string(ns), js_string(&a.name));
        }
        v
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CrKind { Element, External, Component }

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

pub fn generate(root: &Root, config: &NormalizedConfig, filename: &str, scope_token: &str) -> String {
    let mut g = Gen { static_opt: config.enable_static_content_optimization, comments: config.preserve_html_comments, cr: config.custom_renderer_config.clone(), api_version: config.api_version, ..Default::default() };
    let use_static = config.enable_static_content_optimization && !root.children.is_empty()
        && root.children.iter().all(|c| matches!(c, ChildNode::Element(_)) && g.is_static(c));
    let items = if use_static {
        root.children.iter().enumerate().map(|(i, c)| {
            g.static_fragment(c, (i + 1).to_string())
        }).collect()
    } else { g.children(&root.children, 2) };
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
    out.push_str(&format!("import {{ {} }} from \"lwc\";\n", lwc.join(", ")));
    for (i, (f, svg)) in fragments.iter().enumerate() {
        out.push_str(&format!("const $fragment{} = {}`{f}`;\n", i + 1, if *svg { "parseSVGFragment" } else { "parseFragment" }));
    }
    out.push_str("function tmpl($api, $cmp, $slotset, $ctx) {\n");
    let mut helpers = Vec::new();
    if g.sp { helpers.push("sp: api_static_part"); }
    if use_static || g.st { helpers.push("st: api_static_fragment"); }
    if g.text { helpers.push("t: api_text"); }
    if g.dyn_text { helpers.push("d: api_dynamic_text"); }
    if g.element { helpers.push("h: api_element"); }
    if g.api_key { helpers.push("k: api_key"); }
    if g.iterator { helpers.push("i: api_iterator"); }
    if g.slot { helpers.push("s: api_slot"); }
    if g.bind { helpers.push("b: api_bind"); }
    if g.fragment { helpers.push("fr: api_fragment"); }
    if g.ssf { helpers.push("ssf: api_scoped_slot_factory"); }
    if g.custom { helpers.push("c: api_custom_element"); }
    if g.ddc { helpers.push("ddc: api_deprecated_dynamic_component"); }
    if g.dc { helpers.push("dc: api_dynamic_component"); }
    if g.shc { helpers.push("shc: api_sanitize_html_content"); }
    if g.ncls { helpers.push("ncls: api_normalize_class_name"); }
    if !helpers.is_empty() { out.push_str(&format!("  const {{ {} }} = $api;\n", helpers.join(", "))); }
    if items.is_empty() {
        out.push_str("  return [];\n}\n");
    } else if use_static {
        out.push_str(&format!("  return [{}];\n}}\n", items.join(", ")));
    } else {
        out.push_str(&format!("  return [\n{},\n  ];\n}}\n", items.iter().map(|i| format!("    {i}")).collect::<Vec<_>>().join(",\n")));
    }
    out.push_str("export default registerTemplate(tmpl);\n");
    if !g.slots.is_empty() {
        out.push_str(&format!("tmpl.slots = [{}];\n", g.slots.iter().map(|s| js_string(s)).collect::<Vec<_>>().join(", ")));
    }
    if g.has_refs { out.push_str("tmpl.hasRefs = true;\n"); }
    // TODO: implicit `./{base}.css` + `./{base}.scoped.css?scoped=true` imports and push blocks (TS always emits them).
    out.push_str("tmpl.stylesheets = [];\n");
    out.push_str(&format!("tmpl.stylesheetToken = {};\n", js_string(scope_token)));
    out.push_str("if (_implicitStylesheets) {\n  tmpl.stylesheets.push.apply(tmpl.stylesheets, _implicitStylesheets);\n}\nif (_implicitScopedStylesheets) {\n  tmpl.stylesheets.push.apply(tmpl.stylesheets, _implicitScopedStylesheets);\n}\n");
    out.push_str("freezeTemplate(tmpl);\n");
    out
}
