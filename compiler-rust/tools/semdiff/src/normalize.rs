//! AST normalizations for runtime-equivalent module shape.
//!
//! Each function is one named rule. They mutate the program in place and report whether they
//! changed it. They do not drop, reorder, or merge anything whose evaluation order or bindings differ.
use oxc::allocator::{Allocator, TakeIn};
use oxc::ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName,
    Program, Statement, VariableDeclarationKind,
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
        }
        walk_expression(self, it);
    }
}

pub(crate) fn is_import(stmt: &Statement<'_>) -> bool {
    matches!(stmt, Statement::ImportDeclaration(_))
}
