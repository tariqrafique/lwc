//! @api / @track / @wire -> registerDecorators (port of babel-plugin-component/src/decorators/**).
//! Span-edit style: decorators are deleted by span, the meta object is built as source text,
//! lwc decorator import specifiers are dropped by rewriting the import declaration.
//! Decorator import usage (LWC1103/1104) resolves references with oxc semantic; member/class decorators
//! are matched by name.
use crate::Origin;
use crate::constants::*;
use crate::transforms::scoped_css::serde_quote;
use lwc_shared::api_version::{self, ApiFeature};
use lwc_shared::edits::Edit;
use lwc_shared::errors::error_info::lwc_class::DecoratorErrors as E;
use lwc_shared::{CompileError, CompileOptions, LwcErrorInfo};
use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::span::GetSpan;
use oxc::span::Span;
use std::collections::{HashMap, HashSet};

/// Decorator error: an `@lwc/errors` DecoratorErrors info, its message args, and the node span it is thrown at.
pub(crate) struct DErr(&'static LwcErrorInfo, Vec<String>, Span);
fn derr(info: &'static LwcErrorInfo, args: &[&str], span: Span) -> DErr {
    DErr(info, args.iter().map(|a| a.to_string()).collect(), span)
}

/// Port of @lwc/shared html-attributes.ts.
const DISALLOWED_PROP_SET: [&str; 4] = ["is", "class", "slot", "style"];
const AMBIGUOUS_PROP_SET: [(&str, &str); 6] = [
    ("bgcolor", "bgColor"),
    ("accesskey", "accessKey"),
    ("contenteditable", "contentEditable"),
    ("tabindex", "tabIndex"),
    ("maxlength", "maxLength"),
    ("maxvalue", "maxValue"),
];

const DECORATOR_NAMES: [&str; 3] = ["api", "wire", "track"];
const AVAILABLE: &str = "api, wire, track";

/// local name -> imported name, for specifiers imported from 'lwc'.
fn lwc_imports(program: &Program<'_>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(d) = stmt else {
            continue;
        };
        if d.source.value != LWC_PACKAGE_ALIAS {
            continue;
        }
        for s in d.specifiers.iter().flatten() {
            if let ImportDeclarationSpecifier::ImportSpecifier(s) = s {
                m.insert(s.local.name.to_string(), s.imported.name().to_string());
            }
        }
    }
    m
}

fn decorator_name<'a>(d: &'a Decorator<'_>) -> Option<&'a str> {
    match &d.expression {
        Expression::Identifier(i) => Some(i.name.as_str()),
        Expression::CallExpression(c) => match &c.callee {
            Expression::Identifier(i) => Some(i.name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

// ---------- transform 2: every reference to an lwc decorator import must be a member decorator ----------
// Port of validateImportedLwcDecoratorUsage (Program enter): for each `lwc` import specifier of api/track/wire
// (specifier order), for each reference to its binding (source order): `decorator` = the reference's parent when
// that is a Decorator, else its grandparent -> LWC1104 when not a Decorator; then `decorator.parentPath` must
// be a ClassProperty/ClassMethod (not private/accessor) -> else LWC1103. Parents are Babel's, mapped from oxc.
// Shipped quirk not reproduced: an aliased decorator import (`api as A`) crashes there (getBinding('api')).
fn usage_errors(program: &Program<'_>, src: &str) -> Vec<DErr> {
    use oxc::ast::AstKind;
    use oxc::semantic::AstNodes;
    use oxc::semantic::NodeId;
    let sem = crate::parse::semantic_with_nodes(program);
    let (nodes, scoping): (&AstNodes, _) = (sem.nodes(), sem.scoping());
    // Babel parent: oxc ParenthesizedExpression nodes don't exist in Babel; a class/object method's Function
    // is part of Babel's ClassMethod/ObjectMethod.
    let parent = |id: NodeId| -> NodeId {
        let mut p = nodes.parent_id(id);
        while matches!(nodes.kind(p), AstKind::ParenthesizedExpression(_))
            && p != nodes.parent_id(p)
        {
            p = nodes.parent_id(p);
        }
        match nodes.kind(p) {
            AstKind::Function(_) => {
                let gp = nodes.parent_id(p);
                match nodes.kind(gp) {
                    AstKind::MethodDefinition(_) => gp,
                    AstKind::ObjectProperty(op) if op.method || op.kind != PropertyKind::Init => gp,
                    _ => p,
                }
            }
            _ => p,
        }
    };
    // Babel node location: a class starts at its first decorator; Program spans the whole file, except that
    // after a trailing newline its end is column 0, which normalizeLocation turns into length 0.
    let span_of = |id: NodeId| -> Span {
        match nodes.kind(id) {
            AstKind::Program(_) => Span::new(
                0,
                if src.is_empty() || src.ends_with('\n') {
                    0
                } else {
                    src.len() as u32
                },
            ),
            AstKind::Class(c) => Span::new(
                c.decorators
                    .iter()
                    .map(|d| d.span.start)
                    .min()
                    .unwrap_or(c.span.start)
                    .min(c.span.start),
                c.span.end,
            ),
            k => k.span(),
        }
    };
    let mut out = Vec::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(d) = stmt else {
            continue;
        };
        if d.source.value != LWC_PACKAGE_ALIAS {
            continue;
        }
        for spec in d.specifiers.iter().flatten() {
            let ImportDeclarationSpecifier::ImportSpecifier(spec) = spec else {
                continue;
            };
            let name = spec.imported.name();
            let Some(name) = DECORATOR_NAMES
                .iter()
                .copied()
                .find(|n| *n == name.as_str())
            else {
                continue;
            };
            let Some(sym) = spec.local.symbol_id.get() else {
                continue;
            };
            let mut refs: Vec<NodeId> = scoping
                .get_resolved_reference_ids(sym)
                .iter()
                .map(|r| scoping.get_reference(*r).node_id())
                .collect();
            refs.sort_by_key(|id| nodes.kind(*id).span().start);
            for r in refs {
                let p = parent(r);
                let deco =
                    if matches!(nodes.kind(p), AstKind::Decorator(_)) || p == nodes.parent_id(p) {
                        p
                    } else {
                        parent(p)
                    };
                if !matches!(nodes.kind(deco), AstKind::Decorator(_)) {
                    out.push(derr(&E::IS_NOT_DECORATOR, &[name], span_of(deco)));
                }
                let pm = parent(deco);
                let ok = match nodes.kind(pm) {
                    AstKind::PropertyDefinition(pd) => {
                        !matches!(pd.key, PropertyKey::PrivateIdentifier(_))
                    }
                    AstKind::MethodDefinition(md) => {
                        !matches!(md.key, PropertyKey::PrivateIdentifier(_))
                    }
                    _ => false,
                };
                if !ok || pm == deco {
                    out.push(derr(
                        &E::IS_NOT_CLASS_PROPERTY_OR_CLASS_METHOD,
                        &[name],
                        span_of(pm),
                    ));
                }
            }
        }
    }
    out
}

// ---------- transform 5: registerDecorators ----------
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Property,
    Getter,
    Setter,
    Method,
    /** getDecoratedNodeType -> undefined (private field) */
    Invalid,
}

struct Meta<'s, 'a> {
    name: &'s str,
    prop: String,
    kind: Kind,
    dec: &'s Decorator<'a>,
    el: usize,
    el_span: Span,
    computed: bool,
    bool_true: bool,
}

struct Classes<'s, 'o> {
    src: &'s str,
    origin: Origin<'s>,
    filename: &'s str,
    imports: &'o HashMap<String, String>,
    skip_no_super: bool,
    reg_ids: Vec<String>,
    edits: Vec<Edit>,
    used: bool,
    first_expr: Option<bool>,
    errs: Vec<(u32, DErr)>,
    recover: bool,
    /// Babel class-features throw (`Decorators are not enabled.`) at the first class that still has decorators
    /// after the LWC visitor ran; a real throw, so it ends traversal even in recovery mode.
    fatal: Option<(u32, CompileError)>,
    imported: &'o HashSet<String>,
    consts: &'o HashSet<String>,
    /// Flag on: forward already renamed `#m` methods, so they are not private methods here.
    private_methods: bool,
}

fn sname(k: &PropertyKey<'_>) -> Option<String> {
    k.static_name().and_then(|n| n.as_str().map(String::from))
}

fn slice<'s>(src: &'s str, sp: oxc::span::Span) -> &'s str {
    &src[sp.start as usize..sp.end as usize]
}

fn is_es3_ident(s: &str) -> bool {
    const RESERVED: &[&str] = &[
        "break",
        "case",
        "catch",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "finally",
        "for",
        "function",
        "if",
        "in",
        "instanceof",
        "new",
        "return",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "class",
        "const",
        "enum",
        "export",
        "extends",
        "import",
        "super",
        "implements",
        "interface",
        "let",
        "package",
        "private",
        "protected",
        "public",
        "static",
        "yield",
        "null",
        "true",
        "false",
        "abstract",
        "boolean",
        "byte",
        "char",
        "double",
        "final",
        "float",
        "goto",
        "int",
        "long",
        "native",
        "short",
        "synchronized",
        "throws",
        "transient",
        "volatile",
    ];
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !RESERVED.contains(&s)
}

/// Port of wire/transform.ts getGeneratedConfig param value + temps.
fn param_value(path: &str, counter: &mut u32, decls: &mut Vec<String>) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    let invalid = parts.iter().any(|p| !is_es3_ident(p));
    let member = |obj: &str, p: &str| {
        if invalid {
            format!("{obj}[{}]", serde_quote(p))
        } else {
            format!("{obj}.{p}")
        }
    };
    if parts.len() == 1 {
        return member("$cmp", parts[0]);
    }
    *counter += 1;
    let v = format!("v{counter}");
    decls.push(format!("let {v} = {};", member("$cmp", parts[0])));
    let mut test = format!("{v} != null");
    for p in &parts[1..parts.len() - 1] {
        test.push_str(&format!(" && ({v} = {}) != null", member(&v, p)));
    }
    format!(
        "{test} ? {} : undefined",
        member(&v, parts[parts.len() - 1])
    )
}

impl<'s> Classes<'s, '_> {
    fn wire_entry(&self, m: &Meta<'s, '_>) -> String {
        let src = self.src;
        let Expression::CallExpression(call) = &m.dec.expression else {
            return format!(
                "{}: {{ config: function ($cmp) {{ return {{}}; }} }}",
                m.prop
            );
        };
        let mut parts = Vec::new();
        if let Some(e) = call.arguments.first().and_then(|a| a.as_expression()) {
            if matches!(
                e,
                Expression::Identifier(_)
                    | Expression::StaticMemberExpression(_)
                    | Expression::ComputedMemberExpression(_)
            ) {
                parts.push(format!("adapter: {}", slice(src, e.span())));
            }
        }
        let mut props: Vec<String> = Vec::new();
        let (mut decls, mut counter) = (Vec::new(), 0u32);
        if let Some(arg) = call.arguments.get(1) {
            let mut statics = Vec::new();
            let mut dynamic = Vec::new();
            let mut params = Vec::new();
            if let Some(Expression::ObjectExpression(obj)) = arg.as_expression() {
                for p in &obj.properties {
                    let observed = match p {
                        ObjectPropertyKind::ObjectProperty(op) => match &op.value {
                            Expression::StringLiteral(s)
                                if s.value.as_str().is_some_and(|v| v.starts_with('$')) =>
                            {
                                Some((op, &s.value.as_str().unwrap_or("")[1..]))
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    match observed {
                        None => statics.push(slice(src, p.span()).to_string()),
                        Some((op, path)) => {
                            let key_src = slice(src, op.key.span());
                            dynamic.push(match &op.key {
                                PropertyKey::StaticIdentifier(i) => serde_quote(i.name.as_str()),
                                PropertyKey::Identifier(i) if op.computed => i.name.to_string(),
                                PropertyKey::StringLiteral(s) => {
                                    serde_quote(s.value.as_str().unwrap_or(""))
                                }
                                PropertyKey::NumericLiteral(n) => serde_quote(&n.value.to_string()),
                                PropertyKey::BooleanLiteral(b) => serde_quote(&b.value.to_string()),
                                PropertyKey::NullLiteral(_) => serde_quote("null"),
                                PropertyKey::BigIntLiteral(_) => {
                                    serde_quote(key_src.trim_end_matches('n'))
                                }
                                _ => key_src.to_string(),
                            });
                            let key = if op.computed {
                                format!("[{key_src}]")
                            } else {
                                key_src.to_string()
                            };
                            params.push(format!(
                                "{key}: {}",
                                param_value(path, &mut counter, &mut decls)
                            ));
                        }
                    }
                }
            }
            parts.push(format!("dynamic: [{}]", dynamic.join(", ")));
            props.extend(statics);
            props.extend(params);
        }
        if m.kind == Kind::Method {
            parts.push("method: 1".into());
        }
        decls.push(format!("return {{ {} }};", props.join(", ")));
        parts.push(format!("config: function ($cmp) {{ {} }}", decls.join(" ")));
        format!("{}: {{ {} }}", m.prop, parts.join(", "))
    }

    /// Port of the shipped `Class` visitor. Returns the LWC errors plus whether the member decorators were
    /// removed (shipped removes them only when the meta property list is non-empty; otherwise they stay and
    /// Babel's class-features plugin throws). A class without a superclass is skipped before any validation
    /// when SKIP_UNNECESSARY_REGISTER_DECORATORS is on.
    fn process(&mut self, class: &Class<'_>) -> (Vec<DErr>, bool) {
        match self.process_inner(class) {
            Ok(removed) => (Vec::new(), removed),
            Err((errs, removed)) => (errs, removed),
        }
    }

    fn process_inner(&mut self, class: &Class<'_>) -> Result<bool, (Vec<DErr>, bool)> {
        let body = &class.body.body;
        if body.is_empty() {
            return Ok(false);
        }
        if class.heritage.is_none() && self.skip_no_super {
            return Ok(false);
        }
        let src = self.src;
        let mut metas: Vec<Meta> = Vec::new();
        let mut pre_errs: Vec<DErr> = Vec::new();
        // shipped collects every decorator's meta (LWC1101) before validate() reports LWC1102s.
        let mut type_errs: Vec<DErr> = Vec::new();
        for (el_idx, el) in body.iter().enumerate() {
            let (decs, key, kind) = match el {
                ClassElement::MethodDefinition(m) => (
                    &m.decorators,
                    &m.key,
                    match m.kind {
                        MethodDefinitionKind::Get => Kind::Getter,
                        MethodDefinitionKind::Set => Kind::Setter,
                        _ => Kind::Method,
                    },
                ),
                ClassElement::PropertyDefinition(p) => (&p.decorators, &p.key, Kind::Property),
                ClassElement::AccessorProperty(p) if !p.decorators.is_empty() => {
                    return Err((vec![derr(&E::INVALID_DECORATOR_TYPE, &[], p.span)], true));
                }
                _ => continue,
            };
            for d in decs {
                // LWC1100/1102 point at the decorated member (shipped: handleError(path.parentPath)).
                // recovery: an unimported but known decorator name (e.g. `@api` without import) reports
                // LWC1102 and is still validated as that decorator (babel handleError keeps going).
                let unimported_known = decorator_name(d)
                    .filter(|n| !self.imports.contains_key(*n))
                    .and_then(|n| DECORATOR_NAMES.iter().copied().find(|k| *k == n))
                    .filter(|_| self.recover);
                if let Some(n) = unimported_known {
                    pre_errs.push(derr(
                        &E::INVALID_DECORATOR_WITH_NAME,
                        &[n, AVAILABLE, LWC_PACKAGE_ALIAS],
                        el.span(),
                    ));
                }
                let Some(name) = decorator_name(d)
                    .filter(|n| self.imports.contains_key(*n) || unimported_known.is_some())
                else {
                    let e = match decorator_name(d) {
                        Some(n) => derr(
                            &E::INVALID_DECORATOR_WITH_NAME,
                            &[n, AVAILABLE, LWC_PACKAGE_ALIAS],
                            el.span(),
                        ),
                        None => derr(
                            &E::INVALID_DECORATOR,
                            &[AVAILABLE, LWC_PACKAGE_ALIAS],
                            el.span(),
                        ),
                    };
                    // recovery: handleError collects and the visitor goes on; an unknown name (`@foo`) still
                    // yields a meta that no decorator transform picks up, a non-identifier (LWC1100) yields none.
                    if !self.recover {
                        return Err((vec![e], true));
                    }
                    if e.0.code == E::INVALID_DECORATOR.code {
                        type_errs.push(e)
                    } else {
                        pre_errs.push(e)
                    }
                    continue;
                };
                // A private field is a Babel ClassPrivateProperty: getDecoratedNodeType reports LWC1101 and the
                // decorator is validated with no node type (only reachable in recovery; LWC1103 is thrown first).
                let kind = if matches!(key, PropertyKey::PrivateIdentifier(_)) {
                    type_errs.push(derr(&E::INVALID_DECORATOR_TYPE, &[], el.span()));
                    Kind::Invalid
                } else {
                    kind
                };
                let prop = sname(key).unwrap_or_else(|| slice(src, key.span()).to_string());
                let (computed, bool_true) = match el {
                    ClassElement::MethodDefinition(m) => (m.computed, false),
                    ClassElement::PropertyDefinition(p) => (
                        p.computed,
                        matches!(&p.value, Some(Expression::BooleanLiteral(b)) if b.value),
                    ),
                    _ => (false, false),
                };
                metas.push(Meta {
                    name: unimported_known.unwrap_or_else(|| self.imports[name].as_str()),
                    prop,
                    kind,
                    dec: d,
                    el: el_idx,
                    el_span: el.span(),
                    computed,
                    bool_true,
                });
            }
        }
        // validate (decorators/index.ts order: api, wire, track)
        let mut errs = type_errs;
        errs.append(&mut pre_errs);
        validate_api(&metas, &mut errs);
        self.validate_wire(&metas, &mut errs);
        for m in metas
            .iter()
            .filter(|m| m.name == "track" && m.kind != Kind::Property)
        {
            errs.push(derr(
                &E::TRACK_ONLY_ALLOWED_ON_CLASS_PROPERTIES,
                &[],
                m.dec.span,
            ));
        }
        if !errs.is_empty() && !self.recover {
            return Err((errs, true));
        }
        let mut list = Vec::new();
        // api
        let mut public_props: Vec<(String, u32)> = Vec::new();
        for m in metas
            .iter()
            .filter(|m| m.name == "api" && m.kind != Kind::Method)
        {
            let bit = |k: Kind| match k {
                Kind::Getter => 1,
                Kind::Setter => 2,
                _ => 0,
            };
            let mut cfg = bit(m.kind);
            if matches!(m.kind, Kind::Getter | Kind::Setter) {
                let want = if m.kind == Kind::Getter {
                    MethodDefinitionKind::Set
                } else {
                    MethodDefinitionKind::Get
                };
                if body.iter().any(|el| matches!(el, ClassElement::MethodDefinition(md) if md.kind == want && sname(&md.key).as_deref() == Some(m.prop.as_str()))) {
                    cfg |= bit(if m.kind == Kind::Getter { Kind::Setter } else { Kind::Getter });
                }
            }
            match public_props.iter_mut().find(|(n, _)| *n == m.prop) {
                Some(e) => e.1 |= cfg,
                None => public_props.push((m.prop.clone(), cfg)),
            }
        }
        let key = |n: &str| {
            if is_es3_ident(n) {
                n.to_string()
            } else {
                serde_quote(n)
            }
        };
        if !public_props.is_empty() {
            let v: Vec<_> = public_props
                .iter()
                .map(|(n, c)| format!("{}: {{ config: {c} }}", key(n)))
                .collect();
            list.push(format!("publicProps: {{ {} }}", v.join(", ")));
        }
        let methods: Vec<_> = metas
            .iter()
            .filter(|m| m.name == "api" && m.kind == Kind::Method)
            .map(|m| serde_quote(&m.prop))
            .collect();
        if !methods.is_empty() {
            list.push(format!("publicMethods: [{}]", methods.join(", ")));
        }
        let track: Vec<_> = metas
            .iter()
            .filter(|m| m.name == "track")
            .map(|m| format!("{}: 1", key(&m.prop)))
            .collect();
        if !track.is_empty() {
            list.push(format!("track: {{ {} }}", track.join(", ")));
        }
        let wires: Vec<_> = metas
            .iter()
            .filter(|m| m.name == "wire")
            .map(|m| self.wire_entry(m))
            .collect();
        if !wires.is_empty() {
            list.push(format!("wire: {{ {} }}", wires.join(", ")));
        }
        let fields: Vec<_> = body
            .iter()
            .filter_map(|el| match el {
                ClassElement::PropertyDefinition(p)
                    if !p.computed && !p.r#static && p.decorators.is_empty() =>
                {
                    match &p.key {
                        PropertyKey::StaticIdentifier(i) => Some(serde_quote(i.name.as_str())),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect();
        if !fields.is_empty() {
            list.push(format!("fields: [{}]", fields.join(", ")));
        }
        if !errs.is_empty() {
            return Err((errs, !list.is_empty()));
        }
        if list.is_empty() {
            return Ok(false);
        }

        for m in &metas {
            self.edits
                .push(Edit::replace(m.dec.span.start, m.dec.span.end, ""));
        }
        let meta = format!("{{ {} }}", list.join(", "));
        self.used = true;
        if self.first_expr.is_none() {
            self.first_expr = Some(class.r#type != ClassType::ClassDeclaration);
        }
        // Babel: each class's `addNamed('registerDecorators')` mints its own uid (`_registerDecorators`,
        // `_registerDecorators2`, ...) in visit (pre-)order, skipping names already in the program.
        let reg = (1..)
            .map(|i| {
                if i == 1 {
                    format!("_{REGISTER_DECORATORS_ID}")
                } else {
                    format!("_{REGISTER_DECORATORS_ID}{i}")
                }
            })
            .find(|c| !self.reg_ids.contains(c) && !self.src.contains(c.as_str()))
            .unwrap();
        self.reg_ids.push(reg.clone());
        match (&class.id, class.r#type) {
            (Some(id), ClassType::ClassDeclaration) => self.edits.push(Edit::insert(
                class.span.end,
                format!("\n{reg}({}, {meta});", id.name),
            )),
            _ => {
                self.edits
                    .push(Edit::insert(class.span.start, format!("{reg}(")));
                self.edits
                    .push(Edit::insert(class.span.end, format!(", {meta})")));
            }
        }
        Ok(true)
    }
}

impl<'a> Visit<'a> for Classes<'_, '_> {
    fn visit_class(&mut self, it: &Class<'a>) {
        if self.fatal.is_some() || !(self.recover || self.errs.is_empty()) {
            return;
        }
        let (errs, removed) = self.process(it);
        let thrown = !self.recover && !errs.is_empty();
        self.errs
            .extend(errs.into_iter().map(|d| (it.span.start, d)));
        if !thrown {
            let member_decorated = it.body.body.iter().any(|el| match el {
                ClassElement::MethodDefinition(m) => !m.decorators.is_empty(),
                ClassElement::PropertyDefinition(p) => !p.decorators.is_empty(),
                ClassElement::AccessorProperty(p) => !p.decorators.is_empty(),
                _ => false,
            });
            if !it.decorators.is_empty() || (member_decorated && !removed) {
                let start = it
                    .decorators
                    .iter()
                    .map(|d| d.span.start)
                    .fold(it.span.start, u32::min);
                self.fatal = Some((
                    start,
                    class_features_error(
                        it,
                        self.origin,
                        self.filename,
                        start,
                        self.private_methods,
                    ),
                ));
                return;
            }
        }
        walk::walk_class(self, it);
    }
}

const DECORATORS_NOT_ENABLED: &str = "Decorators are not enabled.\nIf you are using [\"@babel/plugin-proposal-decorators\", { \"version\": \"legacy\" }], make sure it comes *before* \"@babel/plugin-transform-class-properties\" and enable loose mode, like so:\n\t[\"@babel/plugin-proposal-decorators\", { \"version\": \"legacy\" }]\n\t[\"@babel/plugin-transform-class-properties\", { \"loose\": true }]";

/// @babel/helper-create-class-features-plugin `shouldTransform` on a class that still carries decorators
/// (no decorators plugin in @lwc/compiler's Babel config): a private field / private method in it throws
/// first at that member, else `Decorators are not enabled.` at the class (start includes class decorators).
/// @lwc/compiler then wraps it: LWC1198 when the Babel message (incl. its filename and code frame) mentions
/// `track`/`api`/`wire` as a word, else LWC1007 (normalizeToCompilerError; no location, Babel set none).
fn class_features_error(
    class: &Class<'_>,
    origin: Origin<'_>,
    filename: &str,
    class_start: u32,
    private_methods: bool,
) -> CompileError {
    // shipped checks a private field before a private method, each at the first such member.
    // With the flag on, forward renamed `#m` methods to public identifiers before this visitor;
    // they are not private methods anymore (the rename is not re-parsed; the AST still says `#m`).
    let find = |field: bool| {
        class.body.body.iter().find_map(|el| match el {
            ClassElement::PropertyDefinition(p)
                if field && matches!(p.key, PropertyKey::PrivateIdentifier(_)) =>
            {
                Some((
                    "Private fields in decorated classes are not supported yet.",
                    p.span,
                ))
            }
            ClassElement::MethodDefinition(m)
                if !field
                    && !private_methods
                    && matches!(m.key, PropertyKey::PrivateIdentifier(_)) =>
            {
                Some((
                    "Private methods in decorated classes are not supported yet.",
                    m.span,
                ))
            }
            _ => None,
        })
    };
    let private = find(true).or_else(|| find(false));
    let (text, start, end) = match private {
        Some((t, sp)) => (t, sp.start, sp.end),
        None => (DECORATORS_NOT_ENABLED, class_start, class.span.end),
    };
    // offsets are into the renamed source; Babel frames the original (`#m`, not the `__lwc_` rename)
    let (src, start, end) = (origin.src, origin.map(start, false), origin.map(end, true));
    let s = lwc_shared::babel_line_col(src, start);
    let e = lwc_shared::babel_line_col(src, end);
    let frame = lwc_shared::code_frame(src, s, (s.0 == e.0).then_some(e));
    // javascript.ts sniffs this message for JS_TRANSFORMER_DECORATOR_ERROR (LWC1198); see lwc-compiler.
    CompileError::babel(
        "SyntaxError",
        format!("{text}\n{frame}"),
        filename,
        None,
        None,
    )
}

/// Port of decorators/api/validate.ts.
fn validate_api(metas: &[Meta<'_, '_>], errs: &mut Vec<DErr>) {
    let apis: Vec<&Meta> = metas.iter().filter(|m| m.name == "api").collect();
    'm: for m in &apis {
        if metas.iter().any(|t| t.name == "track" && t.el == m.el) {
            {
                errs.push(derr(&E::API_AND_TRACK_DECORATOR_CONFLICT, &[], m.dec.span));
                continue 'm;
            }
        }
        if m.kind == Kind::Method {
            continue;
        }
        let sp = m.el_span;
        if m.computed {
            {
                errs.push(derr(&E::PROPERTY_CANNOT_BE_COMPUTED, &[], sp));
                continue 'm;
            }
        }
        let n = m.prop.as_str();
        if n == "part" {
            {
                errs.push(derr(&E::PROPERTY_NAME_PART_IS_RESERVED, &[n], sp));
                continue 'm;
            }
        } else if n.starts_with("on") {
            {
                errs.push(derr(&E::PROPERTY_NAME_CANNOT_START_WITH_ON, &[n], sp));
                continue 'm;
            }
        } else if n.starts_with("data") && n.len() > 4 {
            {
                errs.push(derr(&E::PROPERTY_NAME_CANNOT_START_WITH_DATA, &[n], sp));
                continue 'm;
            }
        } else if DISALLOWED_PROP_SET.contains(&n) {
            {
                errs.push(derr(&E::PROPERTY_NAME_IS_RESERVED, &[n], sp));
                continue 'm;
            }
        } else if let Some((_, camel)) = AMBIGUOUS_PROP_SET.iter().find(|(k, _)| *k == n) {
            {
                errs.push(derr(&E::PROPERTY_NAME_IS_AMBIGUOUS, &[n, camel], sp));
                continue 'm;
            }
        }
        if m.bool_true {
            {
                errs.push(derr(&E::INVALID_BOOLEAN_PUBLIC_PROPERTY, &[], sp));
                continue 'm;
            }
        }
    }
    let mut seen = HashSet::new();
    for m in &apis {
        if matches!(m.kind, Kind::Getter | Kind::Setter) && !seen.insert(m.prop.as_str()) {
            {
                errs.push(derr(
                    &E::SINGLE_DECORATOR_ON_SETTER_GETTER_PAIR,
                    &[&m.prop],
                    m.el_span,
                ));
                continue;
            }
        }
    }
    for cur in &apis {
        for cmp in &apis {
            let pair = matches!(
                (cur.kind, cmp.kind),
                (Kind::Getter, Kind::Setter) | (Kind::Setter, Kind::Getter)
            );
            if cur.prop == cmp.prop && !std::ptr::eq(*cur, *cmp) && !pair {
                {
                    errs.push(derr(&E::DUPLICATE_API_PROPERTY, &[&cur.prop], cmp.dec.span));
                    continue;
                }
            }
        }
    }
}

impl Classes<'_, '_> {
    /// Subset of decorators/wire/validate.ts (name-based bindings, no scope analysis).
    fn validate_wire(&self, metas: &[Meta<'_, '_>], errs: &mut Vec<DErr>) {
        'w: for w in metas.iter().filter(|m| m.name == "wire") {
            for d in metas.iter().filter(|d| d.el == w.el) {
                if d.name == "wire" && !std::ptr::eq(d, w) {
                    {
                        errs.push(derr(&E::ONE_WIRE_DECORATOR_ALLOWED, &[], w.dec.span));
                        continue 'w;
                    }
                }
                if d.name == "api" || d.name == "track" {
                    {
                        errs.push(derr(
                            &E::CONFLICT_WITH_ANOTHER_DECORATOR,
                            &[d.name],
                            w.dec.span,
                        ));
                        continue 'w;
                    }
                }
            }
            let args = match &w.dec.expression {
                Expression::CallExpression(c) => Some(&c.arguments),
                _ => None,
            };
            let Some(first) = args.and_then(|a| a.first()) else {
                {
                    errs.push(derr(&E::ADAPTER_SHOULD_BE_FIRST_PARAMETER, &[], w.dec.span));
                    continue 'w;
                }
            };
            let root = match first.as_expression() {
                Some(Expression::Identifier(i)) => Some(i.name.as_str()),
                Some(Expression::ComputedMemberExpression(m)) => {
                    errs.push(derr(
                        &E::FUNCTION_IDENTIFIER_CANNOT_HAVE_COMPUTED_PROPS,
                        &[],
                        m.span,
                    ));
                    continue 'w;
                }
                Some(Expression::StaticMemberExpression(m)) => match &m.object {
                    Expression::Identifier(i) => Some(i.name.as_str()),
                    Expression::StaticMemberExpression(_)
                    | Expression::ComputedMemberExpression(_) => {
                        errs.push(derr(
                            &E::FUNCTION_IDENTIFIER_CANNOT_HAVE_NESTED_MEMBER_EXRESSIONS,
                            &[],
                            m.span,
                        ));
                        continue 'w;
                    }
                    _ => None,
                },
                _ => None,
            };
            match root {
                // recovery: babel keeps validating the config arg after a bad adapter
                None => {
                    errs.push(derr(
                        &E::FUNCTION_IDENTIFIER_SHOULD_BE_FIRST_PARAMETER,
                        &[],
                        first.span(),
                    ));
                    if !self.recover {
                        continue 'w;
                    }
                }
                Some(r) if !self.imported.contains(r) && self.src_declares(r) => {
                    errs.push(derr(
                        &E::IMPORTED_FUNCTION_IDENTIFIER_SHOULD_BE_FIRST_PARAMETER,
                        &[],
                        first.span(),
                    ));
                    continue 'w;
                }
                Some(r) if !self.imported.contains(r) => {
                    errs.push(derr(
                        &E::WIRE_ADAPTER_SHOULD_BE_IMPORTED,
                        &[r],
                        first.span(),
                    ));
                    continue 'w;
                }
                _ => {}
            }
            if let Some(Some(Expression::ObjectExpression(obj))) =
                args.and_then(|a| a.get(1)).map(|c| c.as_expression())
            {
                for p in &obj.properties {
                    let ObjectPropertyKind::ObjectProperty(op) = p else {
                        continue;
                    };
                    if !op.computed {
                        continue;
                    }
                    let bad = match &op.key {
                        PropertyKey::TemplateLiteral(t) => {
                            errs.push(derr(
                                &E::COMPUTED_PROPERTY_CANNOT_BE_TEMPLATE_LITERAL,
                                &[],
                                t.span,
                            ));
                            continue 'w;
                        }
                        PropertyKey::Identifier(i) => {
                            !(i.name == "undefined" && !self.src_declares("undefined"))
                                && !self.consts.contains(i.name.as_str())
                        }
                        PropertyKey::StringLiteral(_)
                        | PropertyKey::NumericLiteral(_)
                        | PropertyKey::BigIntLiteral(_)
                        | PropertyKey::BooleanLiteral(_)
                        | PropertyKey::NullLiteral(_) => false,
                        _ => true,
                    };
                    if bad {
                        errs.push(derr(
                            &E::COMPUTED_PROPERTY_MUST_BE_CONSTANT_OR_LITERAL,
                            &[],
                            op.key.span(),
                        ));
                        continue 'w;
                    }
                }
            }
            if let Some(cfg) = args.and_then(|a| a.get(1)) {
                if !matches!(cfg.as_expression(), Some(Expression::ObjectExpression(_))) {
                    {
                        errs.push(derr(
                            &E::CONFIG_OBJECT_SHOULD_BE_SECOND_PARAMETER,
                            &[],
                            cfg.span(),
                        ));
                        continue 'w;
                    } // shipped: handleError(config)
                }
            }
        }
    }
    /// Cheap stand-in for a scope binding: a top-level-ish non-import declaration of `name`.
    fn src_declares(&self, name: &str) -> bool {
        ["const ", "let ", "var ", "function "]
            .iter()
            .any(|k| self.src.contains(&format!("{k}{name}")))
    }
}

/// Errors come back keyed by Babel traversal position (Program enter = 0, class errors = class start) so the
/// caller can interleave them with dynamic-import errors in shipped visit order.
pub fn run(
    program: &Program<'_>,
    src: &str,
    filename: &str,
    o: &CompileOptions,
    origin: Origin<'_>,
    edits: &mut Vec<Edit>,
) -> Result<Option<String>, Vec<(u32, CompileError)>> {
    let err = |DErr(info, args, sp): DErr| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        CompileError::generate_at(info, &args, filename, src, sp.start, sp.end)
    };
    let imports = lwc_imports(program);
    let mut usage = usage_errors(program, src);
    let recover = o.experimental_error_recovery_mode;
    if !recover && !usage.is_empty() {
        return Err(vec![(0, err(usage.swap_remove(0)))]);
    }

    let skip = api_version::is_api_feature_enabled(
        ApiFeature::SkipUnnecessaryRegisterDecorators,
        api_version::get_api_version_from_number(o.api_version),
    );
    let imported: HashSet<String> = program
        .body
        .iter()
        .filter_map(|s| match s {
            Statement::ImportDeclaration(d) => d.specifiers.as_ref(),
            _ => None,
        })
        .flatten()
        .map(|s| s.local().name.to_string())
        .collect();
    let consts: HashSet<String> = program
        .body
        .iter()
        .filter_map(|s| match s {
            Statement::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Const => {
                Some(d)
            }
            _ => None,
        })
        .flat_map(|d| d.declarations.iter())
        .flat_map(|v| v.id.get_binding_identifiers())
        .map(|b| b.name.to_string())
        .collect();
    let mut c = Classes {
        consts: &consts,
        imported: &imported,
        src,
        origin,
        filename,
        fatal: None,
        imports: &imports,
        skip_no_super: skip,
        reg_ids: Vec::new(),
        edits: Vec::new(),
        used: false,
        first_expr: None,
        errs: Vec::new(),
        recover: o.experimental_error_recovery_mode,
        private_methods: o.enable_private_methods,
    };
    c.visit_program(program);
    let mut all: Vec<(u32, DErr)> = usage.into_iter().map(|d| (0, d)).collect();
    all.extend(c.errs);
    if !all.is_empty() || c.fatal.is_some() {
        if !recover {
            all.truncate(1);
        }
        let mut out: Vec<(u32, CompileError)> = all.into_iter().map(|(k, d)| (k, err(d))).collect();
        // A class-features throw: recovery drops the collected errors (caller keeps only the throw); normal
        // mode reaches it only when no LWC error was thrown earlier in traversal.
        if let Some(f) = c.fatal {
            if recover || out.is_empty() {
                out.push(f);
            }
        }
        return Err(out);
    }
    // Babel quirk: when the first registered class is a class expression, the registerDecorators
    // import lands after the template import (component.ts); for declarations it lands first.
    let mut late = None;
    if c.used {
        let specs: Vec<String> = c
            .reg_ids
            .iter()
            .map(|r| format!("{REGISTER_DECORATORS_ID} as {r}"))
            .collect();
        let imp = format!(
            "import {{ {} }} from \"{LWC_PACKAGE_ALIAS}\";\n",
            specs.join(", ")
        );
        if c.first_expr == Some(true) {
            late = Some(imp);
        } else {
            edits.push(Edit::insert(0, imp));
        }
    }
    edits.append(&mut c.edits);

    // transform 8: drop api/track/wire specifiers from 'lwc' imports (whole decl if emptied).
    for stmt in &program.body {
        let Statement::ImportDeclaration(d) = stmt else {
            continue;
        };
        if d.source.value != LWC_PACKAGE_ALIAS {
            continue;
        }
        let Some(specs) = &d.specifiers else { continue };
        let is_dec = |s: &ImportDeclarationSpecifier| matches!(s, ImportDeclarationSpecifier::ImportSpecifier(s) if DECORATOR_NAMES.contains(&s.imported.name().as_str()));
        if !specs.iter().any(is_dec) {
            continue;
        }
        let kept: Vec<_> = specs.iter().filter(|s| !is_dec(s)).collect();
        if kept.is_empty() {
            edits.push(Edit::replace(d.span.start, d.span.end, ""));
            continue;
        }
        let mut head = Vec::new();
        let mut named = Vec::new();
        for s in kept {
            match s {
                ImportDeclarationSpecifier::ImportSpecifier(_) => named.push(slice(src, s.span())),
                _ => head.push(slice(src, s.span()).to_string()),
            }
        }
        if !named.is_empty() {
            head.push(format!("{{ {} }}", named.join(", ")));
        }
        edits.push(Edit::replace(
            d.span.start,
            d.span.end,
            format!("import {} from \"{LWC_PACKAGE_ALIAS}\";", head.join(", ")),
        ));
    }
    Ok(late)
}
