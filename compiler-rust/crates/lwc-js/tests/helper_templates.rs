//! D-0265: helper-body templates localize at the AST-member spans pinned in `helpers.rs`.
use oxc::allocator::Allocator;
use oxc::ast::ast::{Expression, IdentifierReference, StaticMemberExpression};
use oxc::ast_visit::{Visit, walk};

struct Hits {
    prefix: u32,
    members: Vec<(u32, u32, String)>,
    ids: Vec<(u32, u32)>,
}

impl<'a> Visit<'a> for Hits {
    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(o) = &it.object
            && o.name == "babelHelpers"
        {
            self.members.push((
                it.span.start - self.prefix,
                it.span.end - self.prefix,
                it.property.name.to_string(),
            ));
        }
        walk::walk_static_member_expression(self, it);
    }
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.name == "ID" {
            self.ids
                .push((it.span.start - self.prefix, it.span.end - self.prefix));
        }
        walk::walk_identifier_reference(self, it);
    }
}

#[test]
fn helper_template_spans_match_ast() {
    const PREFIX: &str = "void (";
    for (name, body, members, ids) in lwc_js::helpers::helper_ast_templates() {
        let src = format!("{PREFIX}{body});");
        let alloc = Allocator::default();
        let program = lwc_js::parse::parse(&alloc, &src, "helper.js")
            .unwrap_or_else(|e| panic!("{name} template failed to parse: {e}"));
        let mut hits = Hits {
            prefix: PREFIX.len() as u32,
            members: vec![],
            ids: vec![],
        };
        hits.visit_program(&program);
        let got: Vec<(u32, u32, &str)> = hits
            .members
            .iter()
            .map(|(s, e, n)| (*s, *e, n.as_str()))
            .collect();
        let exp: Vec<(u32, u32, &str)> = members.iter().map(|(s, e, n)| (*s, *e, *n)).collect();
        assert_eq!(got, exp, "{name} helper members");
        if name == "classPrivateFieldLooseKey" {
            assert_eq!(hits.ids.as_slice(), ids, "{name} ID identifier");
        } else {
            assert!(hits.ids.is_empty(), "{name} unexpected ID {:?}", hits.ids);
        }
    }
}
