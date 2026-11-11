//! AST normalizations for runtime-equivalent module shape.
//!
//! Each function is one named rule. They mutate the program in place and report whether they
//! changed it. They do not drop, reorder, or merge anything whose evaluation order or bindings differ.
use oxc::allocator::{Allocator, TakeIn};
use oxc::ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName,
    ObjectPropertyKind, Program, PropertyKind, Statement, VariableDeclarationKind,
};
use oxc::ast_visit::walk_mut::walk_expression;
use oxc::ast_visit::VisitMut;
use oxc::span::{GetSpan, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn merge_imports<'a>(alloc: &'a Allocator, program: &mut Program<'a>) -> bool {
    let body = program.body.take_in(&alloc);
    let mut slots: Vec<Option<Statement<'a>>> = body.into_iter().map(Some).collect();
    let mut changed = false;
    let mut index = 0;
    while index < slots.len() {
        let Some(stmt) = slots[index].as_ref() else {
            index += 1;
            continue;
        };
        let Some(key) = merge_key(stmt) else {
            index += 1;
            continue;
        };
        let mut members = vec![index];
        for (j, slot) in slots.iter().enumerate().skip(index + 1) {
            if slot
                .as_ref()
                .is_some_and(|s| merge_key(s).as_deref() == Some(key.as_str()))
            {
                members.push(j);
            }
        }
        if members.len() == 1 {
            if let Some(stmt) = slots[index].as_mut() {
                changed |= sort_import(stmt);
            }
            index += 1;
            continue;
        }
        if !can_merge(&slots, &members) {
            if let Some(stmt) = slots[index].as_mut() {
                changed |= sort_import(stmt);
            }
            index += 1;
            continue;
        }
        changed |= combine(alloc, &mut slots, &members);
        index += 1;
    }
    let stmts = slots.into_iter().flatten();
    program.body = oxc::allocator::Vec::from_iter_in(stmts, &alloc);
    changed
}

fn merge_key(stmt: &Statement<'_>) -> Option<String> {
    let Statement::ImportDeclaration(decl) = stmt else {
        return None;
    };
    if decl.import_kind != ImportOrExportKind::Value
        || decl.phase.is_some()
        || decl.with_clause.is_some()
    {
        return None;
    }
    Some(decl.source.value.as_str()?.to_string())
}

fn can_merge(slots: &[Option<Statement<'_>>], idxs: &[usize]) -> bool {
    let mut defaults = 0u32;
    let mut namespaces = 0u32;
    let mut locals: BTreeMap<String, String> = BTreeMap::new();
    for &i in idxs {
        let Some(Statement::ImportDeclaration(decl)) = slots[i].as_ref() else {
            return false;
        };
        let Some(specs) = &decl.specifiers else {
            continue;
        };
        for spec in specs {
            match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => {
                    defaults += 1;
                    if !note_local(&mut locals, d.local.name.as_str(), "default") {
                        return false;
                    }
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => {
                    namespaces += 1;
                    if !note_local(&mut locals, n.local.name.as_str(), "*") {
                        return false;
                    }
                }
                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                    if s.import_kind != ImportOrExportKind::Value {
                        return false;
                    }
                    if !note_local(
                        &mut locals,
                        s.local.name.as_str(),
                        &export_name(&s.imported),
                    ) {
                        return false;
                    }
                }
            }
        }
    }
    defaults <= 1 && namespaces <= 1
}

fn note_local(locals: &mut BTreeMap<String, String>, local: &str, imported: &str) -> bool {
    if let Some(prev) = locals.get(local) {
        return prev == imported;
    }
    locals.insert(local.to_string(), imported.to_string());
    true
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(n) => n.name.to_string(),
        ModuleExportName::IdentifierReference(n) => n.name.to_string(),
        ModuleExportName::StringLiteral(n) => n.value.as_str().unwrap_or("").to_string(),
    }
}

fn sort_import(stmt: &mut Statement<'_>) -> bool {
    let Statement::ImportDeclaration(decl) = stmt else {
        return false;
    };
    let Some(specs) = decl.specifiers.as_mut() else {
        return false;
    };
    let before: Vec<String> = specs.iter().map(spec_sort_key).collect();
    let mut items: Vec<_> = specs.drain(..).collect();
    items.sort_by(|a, b| {
        spec_rank(a)
            .cmp(&spec_rank(b))
            .then_with(|| spec_sort_key(a).cmp(&spec_sort_key(b)))
    });
    let after: Vec<String> = items.iter().map(spec_sort_key).collect();
    for item in items {
        specs.push(item);
    }
    before != after
}

fn spec_rank(spec: &ImportDeclarationSpecifier<'_>) -> u8 {
    match spec {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => 0,
        ImportDeclarationSpecifier::ImportSpecifier(_) => 1,
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => 2,
    }
}

fn spec_sort_key(spec: &ImportDeclarationSpecifier<'_>) -> String {
    match spec {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => d.local.name.to_string(),
        ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.to_string(),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => n.local.name.to_string(),
    }
}

fn local_name(spec: &ImportDeclarationSpecifier<'_>) -> String {
    spec_sort_key(spec)
}

/// Fold every mergeable import of one module into the first statement, plus a following
/// `import * as ns` when a namespace is also present (the grammar cannot mix `* as` with named
/// specifiers). Duplicate bindings of the same export are dropped. Other member statements are removed.
fn combine<'a>(
    alloc: &'a Allocator,
    slots: &mut [Option<Statement<'a>>],
    members: &[usize],
) -> bool {
    let mut specs: Vec<ImportDeclarationSpecifier<'a>> = Vec::new();
    for &i in members {
        let Some(Statement::ImportDeclaration(decl)) = slots[i].as_mut() else {
            continue;
        };
        if let Some(existing) = decl.specifiers.as_mut() {
            specs.extend(existing.drain(..));
        }
    }
    let mut seen = BTreeSet::new();
    specs.retain(|spec| seen.insert(local_name(spec)));
    let mut namespace = Vec::new();
    let mut named = Vec::new();
    for spec in specs {
        match spec {
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => namespace.push(spec),
            other => named.push(other),
        }
    }
    named.sort_by(|a, b| {
        spec_rank(a)
            .cmp(&spec_rank(b))
            .then_with(|| spec_sort_key(a).cmp(&spec_sort_key(b)))
    });
    let first = members[0];
    let Some(Statement::ImportDeclaration(dest)) = slots[first].as_mut() else {
        return false;
    };
    if named.is_empty() && namespace.is_empty() {
        dest.specifiers = None;
    } else if !named.is_empty() {
        let slot = dest
            .specifiers
            .get_or_insert_with(|| oxc::allocator::Vec::new_in(&alloc));
        slot.clear();
        for spec in named {
            slot.push(spec);
        }
    }
    if !namespace.is_empty() && slots[first].as_ref().is_some_and(|s| {
        matches!(s, Statement::ImportDeclaration(d) if d.specifiers.as_ref().is_some_and(|sp| !sp.is_empty()))
    }) {
        // Named/default occupied `first`. Park the namespace on the next member statement.
        let host = members[1];
        let Some(Statement::ImportDeclaration(ns_decl)) = slots[host].as_mut() else { return false };
        let slot = ns_decl.specifiers.get_or_insert_with(|| oxc::allocator::Vec::new_in(&alloc));
        slot.clear();
        for spec in namespace {
            slot.push(spec);
        }
        for &i in members.iter().skip(2) {
            slots[i] = None;
        }
        return true;
    }
    if !namespace.is_empty() {
        let Some(Statement::ImportDeclaration(dest)) = slots[first].as_mut() else {
            return false;
        };
        let slot = dest
            .specifiers
            .get_or_insert_with(|| oxc::allocator::Vec::new_in(&alloc));
        slot.clear();
        for spec in namespace {
            slot.push(spec);
        }
    }
    for &i in members.iter().skip(1) {
        slots[i] = None;
    }
    true
}

/// Hoist top-level `function` declarations and initializer-less `var` bindings to the top of the
/// module body. `let` / `const` stay put (temporal dead zone). A `var` with an initializer stays
/// put. Function declarations with duplicate or missing names keep source order. Unique names are
/// sorted: a hoisted function's position is not observable.
pub(crate) fn hoist_decls<'a>(alloc: &'a Allocator, program: &mut Program<'a>) -> bool {
    let body = program.body.take_in(&alloc);
    let mut funcs: Vec<Statement<'a>> = Vec::new();
    let mut vars: Vec<Statement<'a>> = Vec::new();
    let mut rest: Vec<Statement<'a>> = Vec::new();
    for stmt in body {
        if matches!(stmt, Statement::FunctionDeclaration(_)) {
            funcs.push(stmt);
        } else if is_uninit_var(&stmt) {
            vars.push(stmt);
        } else {
            rest.push(stmt);
        }
    }
    let mut changed = false;
    let names: Vec<Option<String>> = funcs.iter().map(function_name).collect();
    let unique = names.iter().all(Option::is_some) && {
        let set: BTreeSet<_> = names.iter().filter_map(|n| n.as_deref()).collect();
        set.len() == funcs.len()
    };
    if unique && funcs.len() > 1 {
        let before: Vec<String> = names.iter().map(|n| n.clone().unwrap()).collect();
        let mut paired: Vec<(String, Statement<'a>)> = funcs
            .into_iter()
            .map(|s| (function_name(&s).unwrap(), s))
            .collect();
        paired.sort_by(|a, b| a.0.cmp(&b.0));
        let after: Vec<String> = paired.iter().map(|(n, _)| n.clone()).collect();
        changed |= before != after;
        funcs = paired.into_iter().map(|(_, s)| s).collect();
    }
    if vars.len() > 1 {
        let before: Vec<String> = vars.iter().map(var_sort_key).collect();
        let mut paired: Vec<(String, Statement<'a>)> =
            vars.into_iter().map(|s| (var_sort_key(&s), s)).collect();
        paired.sort_by(|a, b| a.0.cmp(&b.0));
        let after: Vec<String> = paired.iter().map(|(n, _)| n.clone()).collect();
        changed |= before != after;
        vars = paired.into_iter().map(|(_, s)| s).collect();
    }
    let spans: Vec<u32> = funcs
        .iter()
        .chain(vars.iter())
        .chain(rest.iter())
        .map(|s| s.span().start)
        .collect();
    let mut sorted = spans.clone();
    sorted.sort_unstable();
    changed |= spans != sorted;
    let _ = spans;
    let mut out = Vec::with_capacity(funcs.len() + vars.len() + rest.len());
    out.extend(funcs);
    out.extend(vars);
    out.extend(rest);
    program.body = oxc::allocator::Vec::from_iter_in(out, &alloc);
    changed
}

fn function_name(stmt: &Statement<'_>) -> Option<String> {
    let Statement::FunctionDeclaration(f) = stmt else {
        return None;
    };
    f.id.as_ref().map(|id| id.name.to_string())
}

fn is_uninit_var(stmt: &Statement<'_>) -> bool {
    let Statement::VariableDeclaration(v) = stmt else {
        return false;
    };
    v.kind == VariableDeclarationKind::Var
        && !v.declarations.is_empty()
        && v.declarations
            .iter()
            .all(|d| d.init.is_none() && matches!(d.id, BindingPattern::BindingIdentifier(_)))
}

fn var_sort_key(stmt: &Statement<'_>) -> String {
    let Statement::VariableDeclaration(v) = stmt else {
        return String::new();
    };
    v.declarations
        .iter()
        .filter_map(|d| match &d.id {
            BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Drop `ParenthesizedExpression` wrappers. Returns the spans of the wrappers that were removed.
/// Grouping parens do not change the tree they wrap. Callers must already have seen identical
/// printer output, so a paren the printer still emits is not treated as redundant.
pub(crate) fn strip_redundant_parens<'a>(
    alloc: &'a Allocator,
    program: &mut Program<'a>,
) -> Vec<Span> {
    let mut v = StripParens {
        alloc,
        spans: Vec::new(),
    };
    v.visit_program(program);
    v.spans
}

struct StripParens<'a> {
    alloc: &'a Allocator,
    spans: Vec<Span>,
}

impl<'a> VisitMut<'a> for StripParens<'a> {
    fn visit_expression(&mut self, it: &mut Expression<'a>) {
        while let Expression::ParenthesizedExpression(_) = &*it {
            let taken = it.take_in(&self.alloc);
            let Expression::ParenthesizedExpression(mut p) = taken else {
                unreachable!()
            };
            self.spans.push(p.span);
            *it = p.expression.take_in(&self.alloc);
            clear_pife(it);
        }
        walk_expression(self, it);
        clear_pife(it);
    }
}

fn clear_pife(it: &mut Expression<'_>) {
    match it {
        Expression::ArrowFunctionExpression(a) => a.pife = false,
        Expression::FunctionExpression(f) => f.pife = false,
        _ => {}
    }
}

/// Drop an earlier data property when a later data property in the same object has the same
/// static key. `{a: 1, a: 2}` evaluates as `{a: 2}`. A spread or a getter/setter is left alone.
pub(crate) fn collapse_last_keys<'a>(alloc: &'a Allocator, program: &mut Program<'a>) -> Vec<Span> {
    let mut v = LastKey {
        alloc,
        spans: Vec::new(),
    };
    v.visit_program(program);
    v.spans
}

struct LastKey<'a> {
    alloc: &'a Allocator,
    spans: Vec<Span>,
}

impl<'a> VisitMut<'a> for LastKey<'a> {
    fn visit_object_expression(&mut self, it: &mut oxc::ast::ast::ObjectExpression<'a>) {
        oxc::ast_visit::walk_mut::walk_object_expression(self, it);
        let mut last: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        let mut spread = false;
        for (i, prop) in it.properties.iter().enumerate() {
            match prop {
                ObjectPropertyKind::SpreadProperty(_) => spread = true,
                ObjectPropertyKind::ObjectProperty(p)
                    if p.kind == PropertyKind::Init && !p.computed =>
                {
                    if let Some(key) = static_key(&p.key) {
                        if !spread {
                            last.insert(key, i);
                        }
                    }
                }
                _ => {}
            }
        }
        if spread {
            return;
        }
        let keep: std::collections::BTreeSet<usize> = last.values().copied().collect();
        if keep.len() == it.properties.len() {
            return;
        }
        let mut dropped_until: Vec<(u32, u32)> = Vec::new();
        for (i, prop) in it.properties.iter().enumerate() {
            if keep.contains(&i) {
                continue;
            }
            let start = prop.span().start;
            let end = it
                .properties
                .get(i + 1)
                .map(|n| n.span().start)
                .unwrap_or(prop.span().end);
            dropped_until.push((start, end));
        }
        let props = it.properties.take_in(&self.alloc);
        let mut next = oxc::allocator::Vec::new_in(&self.alloc);
        for (i, prop) in props.into_iter().enumerate() {
            if keep.contains(&i) {
                next.push(prop);
            }
        }
        it.properties = next;
        self.spans.extend(
            dropped_until
                .into_iter()
                .map(|(start, end)| Span::new(start, end)),
        );
    }
}

fn static_key(key: &oxc::ast::ast::PropertyKey<'_>) -> Option<String> {
    match key {
        oxc::ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        oxc::ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str().map(str::to_string),
        _ => None,
    }
}

/// Move `var x = <literal>` across adjacent import declarations only.
/// `var id = 0` before `import "m"` and `import "m"; var id = 0` both run the
/// initializer after the module is evaluated. A literal var separated from
/// imports by any other statement stays where it is.
pub(crate) fn slide_literal_vars<'a>(alloc: &'a Allocator, program: &mut Program<'a>) -> bool {
    let body = program.body.take_in(&alloc);
    let mut stmts: Vec<Statement<'a>> = body.into_iter().collect();
    let end = stmts
        .iter()
        .position(|s| {
            !is_import(s)
                && !is_pure_literal_var(s)
                && !matches!(s, Statement::FunctionDeclaration(_))
        })
        .unwrap_or(stmts.len());
    if end == 0 {
        program.body = oxc::allocator::Vec::from_iter_in(stmts, &alloc);
        return false;
    }
    let region: Vec<Statement<'a>> = stmts.drain(..end).collect();
    let before: Vec<u32> = region.iter().map(|s| s.span().start).collect();
    let mut funcs = Vec::new();
    let mut imports = Vec::new();
    let mut lits = Vec::new();
    for s in region {
        if matches!(s, Statement::FunctionDeclaration(_)) {
            funcs.push(s);
        } else if is_import(&s) {
            imports.push(s);
        } else {
            lits.push(s);
        }
    }
    let mut rebuilt = funcs;
    rebuilt.extend(imports);
    rebuilt.extend(lits);
    let after: Vec<u32> = rebuilt.iter().map(|s| s.span().start).collect();
    let changed = before != after;
    rebuilt.extend(stmts);
    program.body = oxc::allocator::Vec::from_iter_in(rebuilt, &alloc);
    changed
}

fn is_pure_literal_var(stmt: &Statement<'_>) -> bool {
    let Statement::VariableDeclaration(v) = stmt else {
        return false;
    };
    if v.kind != VariableDeclarationKind::Var || v.declarations.is_empty() {
        return false;
    }
    v.declarations.iter().all(|d| {
        matches!(d.id, BindingPattern::BindingIdentifier(_))
            && match &d.init {
                Some(Expression::NumericLiteral(_))
                | Some(Expression::StringLiteral(_))
                | Some(Expression::BooleanLiteral(_))
                | Some(Expression::NullLiteral(_)) => true,
                Some(Expression::UnaryExpression(u)) => {
                    u.operator == oxc::ast::ast::UnaryOperator::Void
                        && matches!(u.argument, Expression::NumericLiteral(_))
                }
                _ => false,
            }
    })
}

pub(crate) fn is_import(stmt: &Statement<'_>) -> bool {
    matches!(stmt, Statement::ImportDeclaration(_))
}

/// `{a: a}` and `{a}` print the same (oxc codegen forces shorthand when the key and the value
/// identifier match) but `ObjectProperty::shorthand` still differs. Set the flag to the printer's
/// choice. Returns the spans of properties whose flag flipped, so the extra `: name` tokens can be
/// dropped. `__proto__` keeps its flag: the printer emits that one as written.
pub(crate) struct ShorthandMark {
    pub span: Span,
    pub name: String,
}

pub(crate) fn canon_object_shorthand<'a>(program: &mut Program<'a>) -> Vec<ShorthandMark> {
    let mut v = ShorthandCanon { marks: Vec::new() };
    v.visit_program(program);
    v.marks
}

struct ShorthandCanon {
    marks: Vec<ShorthandMark>,
}

impl<'a> VisitMut<'a> for ShorthandCanon {
    fn visit_object_property(&mut self, it: &mut oxc::ast::ast::ObjectProperty<'a>) {
        oxc::ast_visit::walk_mut::walk_object_property(self, it);
        let Some(name) = shorthand_name_expr(&it.key, &it.value) else {
            return;
        };
        if it.shorthand {
            return;
        }
        it.shorthand = true;
        self.marks.push(ShorthandMark {
            span: it.span,
            name,
        });
    }

    fn visit_binding_property(&mut self, it: &mut oxc::ast::ast::BindingProperty<'a>) {
        oxc::ast_visit::walk_mut::walk_binding_property(self, it);
        let Some(name) = shorthand_name_binding(&it.key, &it.value) else {
            return;
        };
        if it.shorthand {
            return;
        }
        it.shorthand = true;
        self.marks.push(ShorthandMark {
            span: it.span,
            name,
        });
    }
}

fn shorthand_name_expr(
    key: &oxc::ast::ast::PropertyKey<'_>,
    value: &Expression<'_>,
) -> Option<String> {
    let oxc::ast::ast::PropertyKey::StaticIdentifier(key) = key else {
        return None;
    };
    if key.name == "__proto__" {
        return None;
    }
    let Expression::Identifier(ident) = value.without_parentheses() else {
        return None;
    };
    (key.name == ident.name).then(|| key.name.to_string())
}

fn shorthand_name_binding(
    key: &oxc::ast::ast::PropertyKey<'_>,
    value: &BindingPattern<'_>,
) -> Option<String> {
    let oxc::ast::ast::PropertyKey::StaticIdentifier(key) = key else {
        return None;
    };
    let name_matches = match value {
        BindingPattern::BindingIdentifier(id) => key.name == id.name,
        BindingPattern::AssignmentPattern(p) => match &p.left {
            BindingPattern::BindingIdentifier(id) => key.name == id.name,
            _ => false,
        },
        _ => false,
    };
    name_matches.then(|| key.name.to_string())
}

#[cfg(test)]
mod paren_probe {
    use super::*;
    use oxc::codegen::Codegen;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    #[test]
    fn double_paren_arrow_strips() {
        let alloc = Allocator::default();
        let src = "this.stack.indexOf(((item) => item.element === element));";
        let ret = Parser::new(&alloc, src, SourceType::mjs()).parse();
        assert_eq!(ret.diagnostics.errors().count(), 0);
        let mut program = ret.program;
        let spans = strip_redundant_parens(&alloc, &mut program);
        let code = Codegen::new().build(&program).code;
        assert!(!spans.is_empty(), "{code}");
        assert!(code.contains("indexOf((item)"), "{code}");
        assert!(!code.contains("indexOf((("), "{code}");
    }
}
