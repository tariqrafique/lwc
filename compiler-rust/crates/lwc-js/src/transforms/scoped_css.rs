//! Append `?scoped=true` to top-level `*.scoped.css` imports.
use crate::edits::Edit;
use oxc::ast::ast::{Program, Statement};

pub fn run(program: &Program<'_>, edits: &mut Vec<Edit>) {
    for stmt in &program.body {
        if let Statement::ImportDeclaration(d) = stmt {
            let v = d.source.value.as_str().unwrap_or("");
            if v.ends_with(".scoped.css") {
                let lit = serde_quote(&format!("{v}?scoped=true"));
                edits.push(Edit::replace(d.source.span.start, d.source.span.end, lit));
            }
        }
    }
}

pub(crate) fn serde_quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), c => o.push(c) }
    }
    o.push('"');
    o
}
