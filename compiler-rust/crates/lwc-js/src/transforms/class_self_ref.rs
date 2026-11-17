//! Babel `@babel/helper-function-name`, as class-features runs it when it names an anonymous
//! `const B = class { ... }` (`class B`, see private_fields.rs): if the class body refers to the
//! outer binding `B`, the inserted name would capture those references (inside the class, `B`
//! would be the class's own immutable binding, not the live, TDZ-checked variable). Babel then
//! `scope.rename`s the outer binding to a uid (`_B`), everywhere in its scope:
//! - the declaration id and every reference (`{ B }` -> `{ B: _B }`, `export { B }` -> `export { _B as B }`);
//! - `export const B = ...` -> `const _B = ...;` plus `export { _B as B };` after the statement.
//!
//! class_properties.rs reads [`SelfRef`]: for these classes the static-initializer `B` is NOT the
//! class alias, and the class-features alias is generated from "Class" (as Babel does for a class
//! that was anonymous when the alias was made).
use super::hoist::Uids;
use lwc_shared::edits::Edit;
use oxc::ast::AstKind;
use oxc::ast::ast::*;
use oxc::span::GetSpan;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct SelfRef {
    /// rename edits (in `src` coordinates)
    pub edits: Vec<Edit>,
    /// span starts of the classes whose outer binding was renamed
    pub classes: HashSet<u32>,
    /// the uids taken by the renames (other passes must not generate them again)
    pub names: Vec<String>,
}

/// `named` are the private-fields edits: a ` B` insert at `class` + 5 is a class it named.
pub fn run(program: &Program<'_>, named: &[Edit]) -> SelfRef {
    let sem = crate::parse::semantic_with_nodes(program);
    let (nodes, scoping) = (sem.nodes(), sem.scoping());
    let mut uids = Uids::new(program);
    let mut out = SelfRef::default();
    // statement span -> its `export ` prefix end and the declaration's bound names
    let mut exports: HashMap<u32, (u32, u32, Vec<String>)> = HashMap::new();
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut export_of: HashMap<u32, u32> = HashMap::new(); // declaration start -> export start
    for stmt in &program.body {
        if let Statement::ExportDeclaration(e) = stmt
            && let Declaration::VariableDeclaration(d) = &e.declaration
        {
            let names = d
                .declarations
                .iter()
                .flat_map(|dc| dc.id.get_binding_identifiers())
                .map(|b| b.name.to_string())
                .collect();
            exports.insert(e.span.start, (d.span.start, e.span.end, names));
            export_of.insert(d.span.start, e.span.start);
        }
    }
    let mut renamed_exports: HashSet<u32> = HashSet::new();
    for node in nodes.iter() {
        let AstKind::VariableDeclarator(dc) = node.kind() else {
            continue;
        };
        let (BindingPattern::BindingIdentifier(b), Some(Expression::ClassExpression(c))) =
            (&dc.id, dc.init.as_ref().map(|e| e.get_inner_expression()))
        else {
            continue;
        };
        if c.id.is_some() || inserted_name(named, c.span.start).as_deref() != Some(b.name.as_str())
        {
            continue;
        }
        let Some(sym) = b.symbol_id.get() else {
            continue;
        };
        let refs: Vec<_> = scoping
            .get_resolved_reference_ids(sym)
            .iter()
            .map(|r| scoping.get_reference(*r).node_id())
            .collect();
        let inside = |id| {
            let s = nodes.kind(id).span();
            s.start >= c.span.start && s.end <= c.span.end
        };
        if !refs.iter().any(|r| inside(*r)) {
            continue;
        }
        let new = uids.generate(&b.name);
        out.names.push(new.clone());
        out.classes.insert(c.span.start);
        renamed.insert(b.name.to_string(), new.clone());
        out.edits
            .push(Edit::replace(b.span.start, b.span.end, new.clone()));
        for r in refs {
            let span = nodes.kind(r).span();
            let text = match nodes.parent_kind(r) {
                AstKind::ObjectProperty(p) if p.shorthand => format!("{}: {new}", b.name),
                AstKind::AssignmentTargetPropertyIdentifier(_) => format!("{}: {new}", b.name),
                AstKind::ExportSpecifier(s) if s.span == span => format!("{new} as {}", b.name),
                _ => new.clone(),
            };
            out.edits.push(Edit::replace(span.start, span.end, text));
        }
        // Which `export <decl>` statement (if any) declares it: the declarator's declaration parent.
        let decl = nodes.parent_kind(node.id());
        if let AstKind::VariableDeclaration(d) = decl
            && let Some(&e) = export_of.get(&d.span.start)
        {
            renamed_exports.insert(e);
        }
    }
    for e in renamed_exports {
        let (decl_start, end, names) = &exports[&e];
        out.edits.push(Edit::replace(e, *decl_start, String::new()));
        let list: Vec<String> = names
            .iter()
            .map(|n| match renamed.get(n) {
                Some(new) => format!("{new} as {n}"),
                None => n.clone(),
            })
            .collect();
        out.edits.push(Edit::insert(
            *end,
            format!("\nexport {{ {} }};", list.join(", ")),
        ));
    }
    out
}

fn inserted_name(prior: &[Edit], class_start: u32) -> Option<String> {
    prior
        .iter()
        .find(|e| e.start == class_start + 5 && e.start == e.end && e.text.starts_with(' '))
        .map(|e| e.text.trim().to_string())
}
