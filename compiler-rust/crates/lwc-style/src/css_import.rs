//! Port of `@lwc/style-compiler/src/css-import/transform.ts`: validate leading `@import`s, collect
//! their module ids, and remove them from the tree.
use crate::ast::*;
use crate::postcss_lwc_plugin::Ctx;
use crate::StyleError;

fn node_loc(n: &Node) -> (u32, u32) {
    let l = match n { Node::Rule(r) => &r.loc, Node::AtRule(a) => &a.loc, Node::Decl(d) => &d.loc, Node::Comment(_) => return (1, 1) };
    (l.line, l.column)
}

pub(crate) fn transform(nodes: &mut Vec<Node>, imports: &mut Vec<String>, is_root: bool, scoped: bool, ctx: &mut Ctx) -> Result<(), StyleError> {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Rule(r) = &mut nodes[i] { transform(&mut r.block.nodes, imports, false, scoped, ctx)?; }
        if let Node::AtRule(a) = &mut nodes[i] {
            if a.name != "import" {
                if let Some(b) = &mut a.block { transform(&mut b.nodes, imports, false, scoped, ctx)?; }
                i += 1;
                continue;
            }
            let (line, col, params) = (a.loc.line, a.loc.column, a.params.clone());
            let check = || -> Result<String, StyleError> {
                if scoped { return Err(StyleError::syntax("Invalid import statement, imports are not allowed in *.scoped.css files.", line, col)); }
                if let Some(prev) = nodes[..i].iter().rev().find(|p| !matches!(p, Node::Comment(_)) && !matches!(p, Node::AtRule(x) if x.name == "import")) {
                    let (l, c) = node_loc(prev);
                    return Err(StyleError::syntax("@import must precede all other statements", l, c));
                }
                let p = params.trim();
                let missing = || StyleError::syntax("Invalid import statement, unable to find imported module.", line, col);
                let q = p.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(missing)?;
                let close = p[1..].find(q).map(|j| j + 1).unwrap_or(p.len());
                let value = &p[1..close.max(1).min(p.len())];
                if value.is_empty() { return Err(missing()); }
                if !p[(close + 1).min(p.len())..].trim().is_empty() {
                    return Err(StyleError::syntax("Invalid import statement, import statement only support a single parameter.", line, col));
                }
                Ok(value.to_string())
            };
            match check() {
                Ok(id) => {
                    imports.push(id);
                    let removed = nodes.remove(i);
                    if is_root && i == 0 {
                        if let (Some(next), Node::AtRule(old)) = (nodes.get_mut(0), &removed) { set_before(next, old.before.clone()); }
                    }
                    continue;
                }
                Err(e) => ctx.report(e)?,
            }
        }
        i += 1;
    }
    Ok(())
}

fn set_before(n: &mut Node, b: String) {
    match n { Node::Rule(r) => r.before = b, Node::AtRule(a) => a.before = b, Node::Decl(d) => d.before = b, Node::Comment(c) => c.before = b }
}
