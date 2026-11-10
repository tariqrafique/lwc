//! Expressions built while emitting, with spans that index [`Chunk::code`].
//!
//! The static hoist walks this tree. It does not parse the printed text.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget, BinaryOperator,
    BindingPattern, Expression, FormalParameter, FormalParameterKind, FormalParameters,
    FunctionBody, FunctionType, IdentifierName, LogicalOperator, ObjectPropertyKind, PropertyKey,
    PropertyKind, Statement, TemplateElement, TemplateElementValue, UnaryOperator,
};
use oxc_ast::builder::AstBuilder;
use oxc_ast_visit::VisitMut;
use oxc_span::Span;
use oxc_syntax::number::NumberBase;

/// Printed JavaScript plus the oxc expression whose spans index `code`.
///
/// `prec` / `op` are astring's precedence class so a parent can add grouping parentheses the same
/// way [`super::printer`] does. Grouping parentheses are not nodes: [`group`] shifts spans inward
/// and leaves the root span on the inner expression.
pub struct Chunk<'a> {
    pub code: String,
    pub expr: Expression<'a>,
    pub prec: u8,
    pub op: Option<&'static str>,
}

impl<'a> Chunk<'a> {
    pub fn new(code: impl Into<String>, expr: Expression<'a>, prec: u8) -> Self {
        Self {
            code: code.into(),
            expr,
            prec,
            op: None,
        }
    }

    pub fn with_op(mut self, op: &'static str) -> Self {
        self.op = Some(op);
        self
    }
}

/// One property of an object literal emitted by codegen.
pub enum Field<'a> {
    /// `key` is the printed key (`classMap`, `"href"`, `__proto__`).
    Kv(String, Chunk<'a>),
    Spread(Chunk<'a>),
}

pub struct B<'a> {
    alloc: &'a Allocator,
    ast: AstBuilder<'a>,
}

impl<'a> B<'a> {
    pub fn new(alloc: &'a Allocator) -> Self {
        Self {
            alloc,
            ast: AstBuilder::new(alloc),
        }
    }

    pub fn ast(&self) -> &AstBuilder<'a> {
        &self.ast
    }

    pub(super) fn astr(&self, text: &str) -> &'a str {
        self.s(text)
    }

    fn s(&self, text: &str) -> &'a str {
        self.alloc.alloc_str(text)
    }

    pub(super) fn av<T>(&self, items: Vec<T>) -> oxc_allocator::Vec<'a, T> {
        oxc_allocator::Vec::from_iter_in(items, &self.ast)
    }

    fn ident_expr(&self, span: Span, name: &str) -> Expression<'a> {
        Expression::new_identifier(span, self.s(name), &self.ast)
    }

    pub fn ident(&self, name: &str) -> Chunk<'a> {
        let span = Span::new(0, name.len() as u32);
        Chunk::new(name, self.ident_expr(span, name), 20)
    }

    pub fn boolean(&self, value: bool) -> Chunk<'a> {
        let code = if value { "true" } else { "false" };
        let span = Span::new(0, code.len() as u32);
        Chunk::new(
            code,
            Expression::new_boolean_literal(span, value, &self.ast),
            18,
        )
    }

    pub fn null(&self) -> Chunk<'a> {
        let span = Span::new(0, 4);
        Chunk::new("null", Expression::new_null_literal(span, &self.ast), 18)
    }

    /// `printed` is the literal as it appears in source, including quotes for strings.
    pub fn string_printed(&self, printed: &str) -> Chunk<'a> {
        let span = Span::new(0, printed.len() as u32);
        // Hoist slices `printed` by span; the decoded value is unused.
        let expr = Expression::new_string_literal(span, self.s(""), None, &self.ast);
        Chunk::new(printed, expr, 18)
    }

    pub fn number_printed(&self, printed: &str) -> Chunk<'a> {
        let span = Span::new(0, printed.len() as u32);
        let raw = Some(oxc_ast::ast::Str::from(self.s(printed)));
        let expr = Expression::new_numeric_literal(span, 0.0, raw, NumberBase::Decimal, &self.ast);
        Chunk::new(printed, expr, 18)
    }

    pub fn static_member(&self, object: Chunk<'a>, prop: &str) -> Chunk<'a> {
        let mut buf = Buf::new();
        let obj = buf.place(object);
        buf.text(".");
        let start = buf.code.len() as u32;
        buf.text(prop);
        let end = buf.code.len() as u32;
        let name = IdentifierName::new(Span::new(start, end), self.s(prop), &self.ast);
        let expr =
            Expression::new_static_member_expression(buf.span(), obj, name, false, &self.ast);
        Chunk::new(buf.code, expr, 19)
    }

    pub fn computed_member(&self, object: Chunk<'a>, prop: Chunk<'a>) -> Chunk<'a> {
        let mut buf = Buf::new();
        let obj = buf.place(object);
        buf.text("[");
        let p = buf.place(prop);
        buf.text("]");
        let expr = Expression::new_computed_member_expression(buf.span(), obj, p, false, &self.ast);
        Chunk::new(buf.code, expr, 19)
    }

    pub fn call_name(&self, name: &str, args: Vec<Chunk<'a>>) -> Chunk<'a> {
        let callee = self.ident(name);
        self.call(callee, args)
    }

    /// `name(` + caller-written argument text + `)`. `render` places each argument and returns it.
    pub fn call_formatted(
        &self,
        name: &str,
        render: impl FnOnce(&mut Buf) -> Vec<Expression<'a>>,
    ) -> Chunk<'a> {
        let mut buf = Buf::new();
        let callee = buf.place(self.ident(name));
        buf.text("(");
        let args: Vec<Argument<'a>> = render(&mut buf).into_iter().map(Argument::from).collect();
        buf.text(")");
        let expr = Expression::new_call_expression(
            buf.span(),
            callee,
            None,
            self.av(args),
            false,
            &self.ast,
        );
        Chunk::new(buf.code, expr, 19)
    }

    /// Sequence whose spans already index `code` (a printed non-object node and the expressions inside it).
    pub fn sequence(&self, code: String, prec: u8, kids: Vec<Expression<'a>>) -> Chunk<'a> {
        let expr = if kids.is_empty() {
            // Leaf text. Identifier spans cover `code`; the hoist does not read the name.
            let span = Span::new(0, code.len() as u32);
            Expression::new_identifier(span, self.s(&code), &self.ast)
        } else {
            Expression::new_sequence_expression(
                Span::new(0, code.len() as u32),
                self.av(kids),
                &self.ast,
            )
        };
        Chunk::new(code, expr, prec)
    }

    /// `a, b, c` with no grouping parentheses (an `if:*` splice inside a children list).
    pub fn comma(&self, exprs: Vec<Chunk<'a>>) -> Chunk<'a> {
        if exprs.len() == 1 {
            return exprs.into_iter().next().unwrap();
        }
        let mut buf = Buf::new();
        let mut out = Vec::with_capacity(exprs.len());
        for (i, e) in exprs.into_iter().enumerate() {
            if i > 0 {
                buf.text(", ");
            }
            out.push(buf.place(e));
        }
        let expr = Expression::new_sequence_expression(buf.span(), self.av(out), &self.ast);
        Chunk::new(buf.code, expr, 20)
    }

    pub fn call(&self, callee: Chunk<'a>, args: Vec<Chunk<'a>>) -> Chunk<'a> {
        let mut buf = Buf::new();
        let callee_expr = buf.place(callee);
        buf.text("(");
        let mut out: Vec<Argument<'a>> = Vec::with_capacity(args.len());
        for (i, arg) in args.into_iter().enumerate() {
            if i > 0 {
                buf.text(", ");
            }
            out.push(Argument::from(buf.place(arg)));
        }
        buf.text(")");
        let expr = Expression::new_call_expression(
            buf.span(),
            callee_expr,
            None,
            self.av(out),
            false,
            &self.ast,
        );
        Chunk::new(buf.code, expr, 19)
    }

    pub fn array_inline(&self, elems: Vec<Chunk<'a>>) -> Chunk<'a> {
        let mut buf = Buf::new();
        buf.text("[");
        let mut out = Vec::with_capacity(elems.len());
        for (i, el) in elems.into_iter().enumerate() {
            if i > 0 {
                buf.text(", ");
            }
            out.push(ArrayExpressionElement::from(buf.place(el)));
        }
        buf.text("]");
        let expr = Expression::new_array_expression(buf.span(), self.av(out), &self.ast);
        Chunk::new(buf.code, expr, 20)
    }

    /// astring element children: `[\n{pad}  a,\n{pad}  b\n{pad}]`. Empty is `[]`.
    pub fn array_multiline(&self, elems: Vec<Chunk<'a>>, pad: &str) -> Chunk<'a> {
        if elems.is_empty() {
            return self.array_inline(Vec::new());
        }
        let mut buf = Buf::new();
        buf.text("[\n");
        let mut out = Vec::with_capacity(elems.len());
        for (i, el) in elems.into_iter().enumerate() {
            if i > 0 {
                buf.text(",\n");
            }
            buf.text(pad);
            buf.text("  ");
            out.push(ArrayExpressionElement::from(buf.place(el)));
        }
        buf.text("\n");
        buf.text(pad);
        buf.text("]");
        let expr = Expression::new_array_expression(buf.span(), self.av(out), &self.ast);
        Chunk::new(buf.code, expr, 20)
    }

    /// `{ a: 1, b: 2 }` with a space inside the braces. Empty is `{}`.
    pub fn object_spaced(&self, props: Vec<Field<'a>>) -> Chunk<'a> {
        if props.is_empty() {
            return self.object_empty();
        }
        let mut buf = Buf::new();
        buf.text("{ ");
        let built = self.write_props(&mut buf, props, ", ");
        buf.text(" }");
        self.finish_object(buf, built)
    }

    /// astring object: `{\n{pad}  k: v\n{pad}}`. Empty is `{}` (callers that want that pass no props).
    pub fn object_multiline(&self, props: Vec<Field<'a>>, pad: &str) -> Chunk<'a> {
        if props.is_empty() {
            return self.object_empty();
        }
        let mut buf = Buf::new();
        buf.text("{\n");
        let mut built = Vec::with_capacity(props.len());
        for (i, prop) in props.into_iter().enumerate() {
            if i > 0 {
                buf.text(",\n");
            }
            buf.text(pad);
            buf.text("  ");
            built.push(self.write_one(&mut buf, prop));
        }
        buf.text("\n");
        buf.text(pad);
        buf.text("}");
        self.finish_object(buf, built)
    }

    fn object_empty(&self) -> Chunk<'a> {
        let span = Span::new(0, 2);
        let expr = Expression::new_object_expression(span, self.av(Vec::new()), &self.ast);
        Chunk::new("{}", expr, 17)
    }

    fn write_props(
        &self,
        buf: &mut Buf,
        props: Vec<Field<'a>>,
        sep: &str,
    ) -> Vec<ObjectPropertyKind<'a>> {
        let mut built = Vec::with_capacity(props.len());
        for (i, prop) in props.into_iter().enumerate() {
            if i > 0 {
                buf.text(sep);
            }
            built.push(self.write_one(buf, prop));
        }
        built
    }

    fn write_one(&self, buf: &mut Buf, prop: Field<'a>) -> ObjectPropertyKind<'a> {
        match prop {
            Field::Spread(value) => {
                let start = buf.code.len() as u32;
                buf.text("...");
                let argument = buf.place(value);
                let end = buf.code.len() as u32;
                ObjectPropertyKind::new_spread_property(Span::new(start, end), argument, &self.ast)
            }
            Field::Kv(key, value) => {
                let ks = buf.code.len() as u32;
                buf.text(&key);
                let ke = buf.code.len() as u32;
                buf.text(": ");
                let val = buf.place(value);
                let end = buf.code.len() as u32;
                let pkey = self.property_key(Span::new(ks, ke), &key);
                ObjectPropertyKind::new_object_property(
                    Span::new(ks, end),
                    PropertyKind::Init,
                    pkey,
                    val,
                    false,
                    false,
                    false,
                    &self.ast,
                )
            }
        }
    }

    fn property_key(&self, span: Span, key: &str) -> PropertyKey<'a> {
        if key.starts_with('"') || key.starts_with('\'') {
            PropertyKey::from(Expression::new_string_literal(
                span,
                self.s(""),
                None,
                &self.ast,
            ))
        } else {
            PropertyKey::new_static_identifier(span, self.s(key), &self.ast)
        }
    }

    fn finish_object(&self, buf: Buf, props: Vec<ObjectPropertyKind<'a>>) -> Chunk<'a> {
        let expr = Expression::new_object_expression(buf.span(), self.av(props), &self.ast);
        // Objects need parentheses as a member/call operand (astring NEEDS_PARENTHESES).
        Chunk::new(buf.code, expr, 17)
    }

    pub fn cond(&self, test: Chunk<'a>, cons: Chunk<'a>, alt: Chunk<'a>) -> Chunk<'a> {
        let mut buf = Buf::new();
        let t = buf.place(test);
        buf.text(" ? ");
        let c = buf.place(cons);
        buf.text(" : ");
        let a = buf.place(alt);
        let expr = Expression::new_conditional_expression(buf.span(), t, c, a, &self.ast);
        Chunk::new(buf.code, expr, 4)
    }

    pub fn logical(&self, op: &'static str, left: Chunk<'a>, right: Chunk<'a>) -> Chunk<'a> {
        let operator = match op {
            "||" => LogicalOperator::Or,
            "&&" => LogicalOperator::And,
            "??" => LogicalOperator::Coalesce,
            _ => unreachable!("not a logical operator: {op}"),
        };
        let mut buf = Buf::new();
        let l = buf.place(left);
        buf.text(" ");
        buf.text(op);
        buf.text(" ");
        let r = buf.place(right);
        let expr = Expression::new_logical_expression(buf.span(), l, operator, r, &self.ast);
        Chunk::new(buf.code, expr, 13).with_op(op)
    }

    pub fn binary(&self, op: &'static str, left: Chunk<'a>, right: Chunk<'a>) -> Chunk<'a> {
        let operator = binary_op(op);
        let mut buf = Buf::new();
        let l = buf.place(left);
        buf.text(" ");
        buf.text(op);
        buf.text(" ");
        let r = buf.place(right);
        let expr = Expression::new_binary_expression(buf.span(), l, operator, r, &self.ast);
        Chunk::new(buf.code, expr, 14).with_op(op)
    }

    /// `!{arg}` with no precedence parentheses (inline `if:false`).
    pub fn not_raw(&self, arg: Chunk<'a>) -> Chunk<'a> {
        let mut buf = Buf::new();
        buf.text("!");
        let a = buf.place(arg);
        let expr =
            Expression::new_unary_expression(buf.span(), UnaryOperator::LogicalNot, a, &self.ast);
        Chunk::new(buf.code, expr, 15)
    }

    pub fn assign(&self, left: Chunk<'a>, right: Chunk<'a>) -> Chunk<'a> {
        let mut buf = Buf::new();
        let left_expr = buf.place(left);
        buf.text(" = ");
        let right_expr = buf.place(right);
        let target = assignment_target(left_expr, &self.ast);
        let expr = Expression::new_assignment_expression(
            buf.span(),
            AssignmentOperator::Assign,
            target,
            right_expr,
            &self.ast,
        );
        Chunk::new(buf.code, expr, 3)
    }

    /// `function (a, b) {\n{pad}  return {ret};\n{pad}}`.
    pub fn function_return(&self, params: &[String], ret: Chunk<'a>, ind: usize) -> Chunk<'a> {
        let pad = "  ".repeat(ind);
        let mut buf = Buf::new();
        buf.text("function (");
        buf.text(&params.join(", "));
        buf.text(") {\n");
        buf.text(&pad);
        buf.text("  return ");
        let ret_expr = buf.place(ret);
        buf.text(";\n");
        buf.text(&pad);
        buf.text("}");
        self.finish_function(
            buf,
            params,
            vec![Statement::new_return_statement(
                Span::new(0, 0),
                Some(ret_expr),
                &self.ast,
            )],
        )
    }

    /// for:of callback. `ret_body` is the `loop_body` text already including its return indent;
    /// `ret` is the returned expression, placed at the `return ` inside that text.
    pub fn function_for_of(&self, it: &str, ret: Chunk<'a>, ind: usize) -> Chunk<'a> {
        let pad = "  ".repeat(ind + 1);
        let params = [
            format!("{it}Value"),
            format!("{it}Index"),
            format!("{it}First"),
            format!("{it}Last"),
        ];
        let mut buf = Buf::new();
        buf.text("function (");
        buf.text(&params.join(", "));
        buf.text(") {\n");
        buf.text(&pad);
        buf.text("const ");
        buf.text(it);
        buf.text(" = {\n");
        buf.text(&pad);
        buf.text("  value: ");
        buf.text(&params[0]);
        buf.text(",\n");
        buf.text(&pad);
        buf.text("  index: ");
        buf.text(&params[1]);
        buf.text(",\n");
        buf.text(&pad);
        buf.text("  first: ");
        buf.text(&params[2]);
        buf.text(",\n");
        buf.text(&pad);
        buf.text("  last: ");
        buf.text(&params[3]);
        buf.text("\n");
        buf.text(&pad);
        buf.text("};\n");
        // loop_body at `ind`: `{outer}  return {ret};\n{outer}` where outer is `"  ".repeat(ind)`.
        let outer = "  ".repeat(ind);
        buf.text(&outer);
        buf.text("  return ");
        let ret_expr = buf.place(ret);
        buf.text(";\n");
        buf.text(&outer);
        buf.text("}");
        self.finish_function(
            buf,
            &params,
            vec![Statement::new_return_statement(
                Span::new(0, 0),
                Some(ret_expr),
                &self.ast,
            )],
        )
    }

    fn finish_function(
        &self,
        buf: Buf,
        params: &[String],
        statements: Vec<Statement<'a>>,
    ) -> Chunk<'a> {
        let items: Vec<FormalParameter<'a>> = params
            .iter()
            .map(|name| {
                let span = Span::new(0, name.len() as u32);
                let pattern = BindingPattern::new_binding_identifier(span, self.s(name), &self.ast);
                FormalParameter::new(
                    span,
                    self.av(Vec::<oxc_ast::ast::Decorator<'a>>::new()),
                    pattern,
                    None,
                    None,
                    false,
                    None,
                    false,
                    false,
                    &self.ast,
                )
            })
            .collect();
        let formals = FormalParameters::boxed(
            Span::new(0, 0),
            FormalParameterKind::FormalParameter,
            self.av(items),
            None,
            &self.ast,
        );
        let body = FunctionBody::boxed(
            Span::new(0, 0),
            self.av(Vec::<oxc_ast::ast::Directive<'a>>::new()),
            self.av(statements),
            &self.ast,
        );
        let expr = Expression::new_function_expression(
            buf.span(),
            FunctionType::FunctionExpression,
            None,
            false,
            false,
            false,
            None,
            None,
            formals,
            None,
            Some(body),
            &self.ast,
        );
        Chunk::new(buf.code, expr, 17)
    }

    /// `` `quasi${expr}quasi` ``. `quasis.len() == exprs.len() + 1`.
    pub fn template(&self, quasis: &[&str], exprs: Vec<Chunk<'a>>) -> Chunk<'a> {
        assert_eq!(quasis.len(), exprs.len() + 1);
        let mut buf = Buf::new();
        buf.text("`");
        let mut elements = Vec::with_capacity(quasis.len());
        let mut expressions = Vec::with_capacity(exprs.len());
        let mut exprs = exprs.into_iter();
        for (i, quasi) in quasis.iter().enumerate() {
            let start = buf.code.len() as u32;
            buf.text(quasi);
            let end = buf.code.len() as u32;
            elements.push(TemplateElement::new(
                Span::new(start, end),
                TemplateElementValue {
                    raw: oxc_ast::ast::Str::from(self.s(quasi)),
                    cooked: Some(self.s(quasi).into()),
                },
                i + 1 == quasis.len(),
                &self.ast,
            ));
            if i + 1 != quasis.len() {
                buf.text("${");
                expressions.push(buf.place(exprs.next().expect("quasi/expr arity")));
                buf.text("}");
            }
        }
        buf.text("`");
        let expr = Expression::new_template_literal(
            buf.span(),
            self.av(elements),
            self.av(expressions),
            &self.ast,
        );
        Chunk::new(buf.code, expr, 20)
    }
}

/// Buffer of printed text. [`Buf::place`] shifts a chunk's spans to the insertion point.
pub struct Buf {
    pub code: String,
}

impl Buf {
    pub fn new() -> Self {
        Self {
            code: String::new(),
        }
    }

    pub fn text(&mut self, s: &str) {
        self.code.push_str(s);
    }

    pub fn place<'a>(&mut self, mut chunk: Chunk<'a>) -> Expression<'a> {
        let at = self.code.len() as u32;
        self.code.push_str(&chunk.code);
        if at != 0 {
            shift(&mut chunk.expr, at);
        }
        chunk.expr
    }

    pub fn span(&self) -> Span {
        Span::new(0, self.code.len() as u32)
    }
}

/// Wrap `chunk` in grouping parentheses. The expression span stays on the inner node.
pub fn group(mut chunk: Chunk<'_>) -> Chunk<'_> {
    shift(&mut chunk.expr, 1);
    chunk.code.insert(0, '(');
    chunk.code.push(')');
    chunk
}

struct Shift(u32);

impl<'a> VisitMut<'a> for Shift {
    fn visit_span(&mut self, it: &mut Span) {
        it.start += self.0;
        it.end += self.0;
    }
}

pub fn shift(expr: &mut Expression<'_>, by: u32) {
    if by != 0 {
        Shift(by).visit_expression(expr);
    }
}

fn binary_op(op: &str) -> BinaryOperator {
    match op {
        "==" => BinaryOperator::Equality,
        "!=" => BinaryOperator::Inequality,
        "===" => BinaryOperator::StrictEquality,
        "!==" => BinaryOperator::StrictInequality,
        "<" => BinaryOperator::LessThan,
        "<=" => BinaryOperator::LessEqualThan,
        ">" => BinaryOperator::GreaterThan,
        ">=" => BinaryOperator::GreaterEqualThan,
        "<<" => BinaryOperator::ShiftLeft,
        ">>" => BinaryOperator::ShiftRight,
        ">>>" => BinaryOperator::ShiftRightZeroFill,
        "+" => BinaryOperator::Addition,
        "-" => BinaryOperator::Subtraction,
        "*" => BinaryOperator::Multiplication,
        "/" => BinaryOperator::Division,
        "%" => BinaryOperator::Remainder,
        "**" => BinaryOperator::Exponential,
        "|" => BinaryOperator::BitwiseOR,
        "^" => BinaryOperator::BitwiseXOR,
        "&" => BinaryOperator::BitwiseAnd,
        "in" => BinaryOperator::In,
        "instanceof" => BinaryOperator::Instanceof,
        _ => unreachable!("not a binary operator: {op}"),
    }
}

/// Left side of `=` emitted as a member or identifier. Other shapes are not assigned to by codegen.
fn assignment_target<'a>(left: Expression<'a>, ast: &AstBuilder<'a>) -> AssignmentTarget<'a> {
    match left {
        Expression::Identifier(id) => {
            let id = id.unbox();
            AssignmentTarget::new_assignment_target_identifier(id.span, id.name, ast)
        }
        Expression::StaticMemberExpression(m) => {
            let m = m.unbox();
            AssignmentTarget::new_static_member_expression(
                m.span, m.object, m.property, m.optional, ast,
            )
        }
        Expression::ComputedMemberExpression(m) => {
            let m = m.unbox();
            AssignmentTarget::new_computed_member_expression(
                m.span,
                m.object,
                m.expression,
                m.optional,
                ast,
            )
        }
        // Codegen assigns only the members and identifiers it just built (`$ctx._mN = …`).
        other => panic!("assignment target is not an identifier or member: {other:?}"),
    }
}
