//! Append `?scoped=true` to top-level `*.scoped.css` imports.
use lwc_shared::edits::Edit;
use oxc::ast::ast::{Program, Statement};

pub fn run(program: &Program<'_>, edits: &mut Vec<Edit>) {
    for stmt in &program.body {
        if let Statement::ImportDeclaration(d) = stmt {
            let v = d.source.value.as_str().unwrap_or("");
            if v.ends_with(".scoped.css") {
                let lit = lwc_shared::js::string_literal(format!("{v}?scoped=true").encode_utf16());
                edits.push(Edit::replace(d.source.span.start, d.source.span.end, lit));
            }
        }
    }
}
