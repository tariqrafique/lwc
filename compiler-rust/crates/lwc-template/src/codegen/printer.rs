//! astring 1.9.0 parenthesization and GENERATOR for expressions this crate prints.
//!
//! Composed nodes (sanitized-html, etc.) carry the facts astring reads
//! (`EXPRESSIONS_PRECEDENCE[type]`, `OPERATOR_PRECEDENCE[operator]`) so the same
//! rules apply: `expressionNeedsParenthesis` (astring.mjs:111) for binary/logical
//! operands, the `ConditionalExpression` printer (astring.mjs:977) that only
//! guards the test, and `AssignmentExpression` (astring.mjs:953) that guards
//! neither side.
//!
//! [`print_expr`] is a straight port of astring 1.9.0's GENERATOR for the node
//! kinds the complex-expression validator admits (AB-D0004 S4). Bound identifier
//! references (from [`BindPlan`]) print as `<inst>.<name>` with MemberExpression
//! precedence (19), matching upstream's substituted node.

use crate::codegen::expression::BindPlan;
use oxc_ast::ast::{
    Argument, ArrayAssignmentTarget, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, BindingProperty,
    CallExpression, ChainElement, ComputedMemberExpression, Expression, FormalParameters,
    ObjectAssignmentTarget, ObjectProperty, ObjectPropertyKind, PrivateFieldExpression, PropertyKey,
    PropertyKind, StaticMemberExpression,
};
use oxc_span::Span;
use std::collections::HashSet;

/// A printed expression plus its astring precedence class.
#[derive(Debug, Clone)]
pub struct Js {
    pub code: String,
    prec: u8,
    op: Option<&'static str>,
}

impl Js {
    /// astring `EXPRESSIONS_PRECEDENCE` of the printed node.
    pub fn precedence(&self) -> u8 { self.prec }
}

/// Failure to print a validated expression. This is an invariant violation
/// (unsupported kind after validation, or a BindPlan span the walk missed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintError(pub &'static str);

/// astring `NEEDS_PARENTHESES`.
const NEEDS_PARENTHESES: u8 = 17;
const MEMBER: u8 = 19;
const BINARY: u8 = 14;
const LOGICAL: u8 = 13;
const CONDITIONAL: u8 = 4;
const ASSIGNMENT: u8 = 3;

/// astring `OPERATOR_PRECEDENCE`.
fn operator_precedence(op: &str) -> u8 {
    match op {
        "||" => 2, "??" => 3, "&&" => 4, "|" => 5, "^" => 6, "&" => 7,
        "==" | "!=" | "===" | "!==" => 8,
        "<" | ">" | "<=" | ">=" | "in" | "instanceof" => 9,
        "<<" | ">>" | ">>>" => 10, "+" | "-" => 11, "*" | "%" | "/" => 12, "**" => 13,
        _ => unreachable!("not a binary/logical operator: {op}"),
    }
}

impl Js {
    /// `MemberExpression` (19).
    pub(crate) fn member(code: impl Into<String>) -> Js { Js { code: code.into(), prec: MEMBER, op: None } }

    /// `AssignmentExpression` (3): astring prints both sides as they are, so the right side may
    /// be any already-printed expression.
    pub(crate) fn assign(left: Js, right: &str) -> Js {
        Js { code: format!("{} = {right}", left.code), prec: ASSIGNMENT, op: None }
    }

    /// `BinaryExpression` (14) or, for `||` `&&` `??`, `LogicalExpression` (13).
    pub(crate) fn binary(op: &'static str, left: Js, right: Js) -> Js {
        let prec = if matches!(op, "||" | "&&" | "??") { LOGICAL } else { BINARY };
        let parent = Js { code: String::new(), prec, op: Some(op) };
        let code = format!("{} {op} {}", left.operand_of(&parent, false), right.operand_of(&parent, true));
        Js { code: if op == "in" { format!("({code})") } else { code }, prec, op: Some(op) }
    }

    /// `ConditionalExpression` (4): only the test is parenthesized.
    pub(crate) fn conditional(test: Js, consequent: Js, alternate: Js) -> Js {
        let t = if test.prec == NEEDS_PARENTHESES || test.prec <= CONDITIONAL { format!("({})", test.code) } else { test.code };
        Js { code: format!("{t} ? {} : {}", consequent.code, alternate.code), prec: CONDITIONAL, op: None }
    }

    /// astring `formatExpression`: `self` printed as an operand of `parent`.
    fn operand_of(self, parent: &Js, right: bool) -> String {
        if self.needs_parens(parent, right) { format!("({})", self.code) } else { self.code }
    }

    /// astring `expressionNeedsParenthesis`.
    fn needs_parens(&self, parent: &Js, right: bool) -> bool {
        if self.prec == NEEDS_PARENTHESES { return true; }
        if self.prec != parent.prec {
            return (!right && self.prec == 15 && parent.prec == BINARY && parent.op == Some("**")) || self.prec < parent.prec;
        }
        if self.prec != LOGICAL && self.prec != BINARY { return false; }
        let (op, pop) = (self.op.unwrap_or_default(), parent.op.unwrap_or_default());
        if op == "**" && pop == "**" { return !right; }
        if self.prec == LOGICAL && parent.prec == LOGICAL && (op == "??" || pop == "??") { return true; }
        let (a, b) = (operator_precedence(op), operator_precedence(pop));
        if right { a <= b } else { a < b }
    }
}

/// ECMAScript `Number.prototype.toString` / JSON.stringify of a finite number, matching
/// `oxc_ecmascript::ToJsString` for `NumericLiteral` (0 and -0 both print as `0`).
pub(crate) fn js_number(n: f64) -> String {
    use oxc_syntax::number::ToJsString;
    if n == 0.0 { "0".into() } else { n.to_js_string() }
}

/// Print `e` the way astring 1.9.0 would after `bindExpression` has rewritten planned
/// identifier references to `<inst>.<name>` MemberExpressions.
///
/// `indent` is astring's `state.indentLevel` (2-space units) at the start of this expression.
/// Every span in `plan` must be printed (AB-D0004 condition 5); a miss is [`PrintError`].
pub fn print_expr(e: &Expression<'_>, plan: &BindPlan, inst: &str, indent: usize) -> Result<Js, PrintError> {
    let mut p = Printer { plan, inst, indent, printed: HashSet::new() };
    let out = p.expr(e)?;
    for sp in plan.iter() {
        if !p.printed.contains(&sp) {
            return Err(PrintError("BindPlan span was not printed"));
        }
    }
    Ok(out)
}

struct Printer<'a> {
    plan: &'a BindPlan,
    inst: &'a str,
    indent: usize,
    printed: HashSet<Span>,
}

impl Printer<'_> {
    fn expr(&mut self, e: &Expression<'_>) -> Result<Js, PrintError> {
        match e {
            Expression::Identifier(id) => self.identifier(id.span, id.name.as_str()),
            Expression::BooleanLiteral(l) => Ok(lit(if l.value { "true" } else { "false" })),
            Expression::NullLiteral(_) => Ok(lit("null")),
            Expression::NumericLiteral(l) => {
                let fallback = js_number(l.value);
                Ok(self.raw_or_value(l.raw.as_ref().map(|r| r.as_str()), &fallback))
            }
            Expression::StringLiteral(l) => {
                if let Some(raw) = l.raw.as_ref() {
                    Ok(lit(raw.as_str()))
                } else if let Some(v) = l.value.as_str() {
                    Ok(lit(&crate::codegen::js_string(v)))
                } else {
                    Err(PrintError("string literal value is not UTF-8"))
                }
            }
            Expression::BigIntLiteral(l) => {
                if let Some(raw) = l.raw.as_ref() {
                    Ok(lit(raw.as_str()))
                } else {
                    Ok(lit(&format!("{}n", l.value.as_str())))
                }
            }
            Expression::RegExpLiteral(l) => {
                if let Some(raw) = l.raw.as_ref() {
                    Ok(lit(raw.as_str()))
                } else {
                    Ok(lit(&l.regex.to_string()))
                }
            }
            Expression::TemplateLiteral(t) => self.template(t),
            Expression::TaggedTemplateExpression(t) => {
                let tag = self.format(&t.tag, tagged_parent(), false)?;
                let quasi = self.template(&t.quasi)?;
                Ok(Js { code: format!("{tag}{}", quasi.code), prec: 20, op: None })
            }
            Expression::ArrayExpression(a) => self.array(&a.elements),
            Expression::ObjectExpression(o) => self.object(&o.properties),
            Expression::StaticMemberExpression(m) => self.static_member(m),
            Expression::ComputedMemberExpression(m) => self.computed_member(m),
            Expression::PrivateFieldExpression(m) => self.private_member(m),
            Expression::CallExpression(c) => self.call(c),
            Expression::ChainExpression(c) => self.chain(&c.expression),
            Expression::UnaryExpression(u) => self.unary(u.operator.as_str(), &u.argument),
            Expression::UpdateExpression(u) => self.update(u.operator.as_str(), u.prefix, &u.argument),
            Expression::BinaryExpression(b) => self.binary(b.operator.as_str(), false, &b.left, &b.right),
            Expression::LogicalExpression(l) => self.binary(l.operator.as_str(), true, &l.left, &l.right),
            Expression::ConditionalExpression(c) => {
                let test = self.expr(&c.test)?;
                let t = if test.prec == NEEDS_PARENTHESES || test.prec <= CONDITIONAL {
                    format!("({})", test.code)
                } else {
                    test.code
                };
                let cons = self.expr(&c.consequent)?;
                let alt = self.expr(&c.alternate)?;
                Ok(Js { code: format!("{t} ? {} : {}", cons.code, alt.code), prec: CONDITIONAL, op: None })
            }
            Expression::AssignmentExpression(a) => {
                let left = self.assignment_target(&a.left)?;
                let right = self.expr(&a.right)?;
                Ok(Js { code: format!("{} {} {}", left, a.operator.as_str(), right.code), prec: ASSIGNMENT, op: None })
            }
            Expression::ArrowFunctionExpression(a) => self.arrow(a),
            Expression::ParenthesizedExpression(p) => self.expr(&p.expression),
            Expression::SequenceExpression(s) => {
                let mut parts = Vec::new();
                for e in &s.expressions { parts.push(self.expr(e)?.code); }
                Ok(Js { code: format!("({})", parts.join(", ")), prec: 20, op: None })
            }
            Expression::ThisExpression(_) => Ok(Js { code: "this".into(), prec: 20, op: None }),
            Expression::Super(_) => Ok(Js { code: "super".into(), prec: 20, op: None }),
            Expression::ImportMeta(_) => Ok(Js { code: "import.meta".into(), prec: 20, op: None }),
            Expression::NewTarget(_) => Ok(Js { code: "new.target".into(), prec: 20, op: None }),
            _ => Err(PrintError("unsupported expression kind in print_expr")),
        }
    }

    fn identifier(&mut self, span: Span, name: &str) -> Result<Js, PrintError> {
        if self.plan.contains(span) {
            self.printed.insert(span);
            Ok(Js { code: format!("{}.{name}", self.inst), prec: MEMBER, op: None })
        } else {
            // Unplanned identifiers (arrow params, locals, nested `undefined`, computed keys) still
            // "consume" a span if it happens to be listed, but they are not in the plan.
            Ok(Js { code: name.into(), prec: 20, op: None })
        }
    }

    fn raw_or_value(&self, raw: Option<&str>, value: &str) -> Js {
        lit(raw.unwrap_or(value))
    }

    fn template(&mut self, t: &oxc_ast::ast::TemplateLiteral<'_>) -> Result<Js, PrintError> {
        let mut code = String::from("`");
        for (i, expr) in t.expressions.iter().enumerate() {
            code.push_str(t.quasis[i].value.raw.as_str());
            code.push_str("${");
            code.push_str(&self.expr(expr)?.code);
            code.push('}');
        }
        code.push_str(t.quasis.last().map(|q| q.value.raw.as_str()).unwrap_or(""));
        code.push('`');
        Ok(Js { code, prec: 20, op: None })
    }

    fn array(&mut self, elements: &[ArrayExpressionElement<'_>]) -> Result<Js, PrintError> {
        let mut parts = Vec::new();
        let mut last_elision = false;
        for el in elements {
            last_elision = matches!(el, ArrayExpressionElement::Elision(_));
            parts.push(match el {
                ArrayExpressionElement::Elision(_) => String::new(),
                ArrayExpressionElement::SpreadElement(s) => format!("...{}", self.expr(&s.argument)?.code),
                other => self.expr(other.to_expression())?.code,
            });
        }
        let mut inner = parts.join(", ");
        if last_elision && !elements.is_empty() { inner.push_str(", "); }
        Ok(Js { code: format!("[{inner}]"), prec: 20, op: None })
    }

    fn object(&mut self, properties: &[ObjectPropertyKind<'_>]) -> Result<Js, PrintError> {
        let captured = self.indent;
        self.indent += 1;
        let code = if properties.is_empty() {
            "{}".into()
        } else {
            let pad = "  ".repeat(self.indent);
            let close = "  ".repeat(captured);
            let mut props = Vec::new();
            for p in properties {
                props.push(format!("{pad}{}", self.object_prop(p)?));
            }
            format!("{{\n{}\n{close}}}", props.join(",\n"))
        };
        self.indent -= 1;
        Ok(Js { code, prec: NEEDS_PARENTHESES, op: None })
    }

    fn object_prop(&mut self, p: &ObjectPropertyKind<'_>) -> Result<String, PrintError> {
        match p {
            ObjectPropertyKind::SpreadProperty(s) => Ok(format!("...{}", self.expr(&s.argument)?.code)),
            ObjectPropertyKind::ObjectProperty(p) => self.property(p),
        }
    }

    fn property(&mut self, p: &ObjectProperty<'_>) -> Result<String, PrintError> {
        if p.method || p.kind != PropertyKind::Init {
            return Err(PrintError("method/get/set object properties are rejected by validation"));
        }
        let value = self.expr(&p.value)?;
        if p.shorthand {
            // astring prints only the value, so after binding a shorthand `{b}` becomes `{$cmp.b}`
            // (D-0224 / AB-D0004 Q4: reproduce upstream).
            return Ok(value.code);
        }
        let key = if p.computed {
            format!("[{}]", self.property_key(&p.key)?)
        } else {
            self.property_key(&p.key)?
        };
        Ok(format!("{key}: {}", value.code))
    }

    fn property_key(&mut self, key: &PropertyKey<'_>) -> Result<String, PrintError> {
        match key {
            PropertyKey::StaticIdentifier(id) => Ok(id.name.as_str().to_string()),
            PropertyKey::PrivateIdentifier(id) => Ok(format!("#{}", id.name.as_str())),
            PropertyKey::Identifier(id) => self.identifier(id.span, id.name.as_str()).map(|j| j.code),
            other => Ok(self.expr(other.to_expression())?.code),
        }
    }

    fn static_member(&mut self, m: &StaticMemberExpression<'_>) -> Result<Js, PrintError> {
        let obj = self.format(&m.object, member_parent(), false)?;
        let dot = if m.optional { "?." } else { "." };
        Ok(Js { code: format!("{obj}{dot}{}", m.property.name.as_str()), prec: MEMBER, op: None })
    }

    fn computed_member(&mut self, m: &ComputedMemberExpression<'_>) -> Result<Js, PrintError> {
        let obj = self.format(&m.object, member_parent(), false)?;
        let prop = self.expr(&m.expression)?;
        let head = if m.optional { "?." } else { "" };
        Ok(Js { code: format!("{obj}{head}[{}]", prop.code), prec: MEMBER, op: None })
    }

    fn private_member(&mut self, m: &PrivateFieldExpression<'_>) -> Result<Js, PrintError> {
        let obj = self.format(&m.object, member_parent(), false)?;
        let dot = if m.optional { "?." } else { "." };
        Ok(Js { code: format!("{obj}{dot}#{}", m.field.name.as_str()), prec: MEMBER, op: None })
    }

    fn call(&mut self, c: &CallExpression<'_>) -> Result<Js, PrintError> {
        let callee = self.format(&c.callee, call_parent(), false)?;
        let args = self.sequence(&c.arguments)?;
        let opt = if c.optional { "?." } else { "" };
        Ok(Js { code: format!("{callee}{opt}{args}"), prec: MEMBER, op: None })
    }

    fn chain(&mut self, e: &ChainElement<'_>) -> Result<Js, PrintError> {
        match e {
            ChainElement::CallExpression(c) => self.call(c),
            ChainElement::StaticMemberExpression(m) => self.static_member(m),
            ChainElement::ComputedMemberExpression(m) => self.computed_member(m),
            ChainElement::PrivateFieldExpression(m) => self.private_member(m),
            ChainElement::TSNonNullExpression(n) => self.expr(&n.expression),
        }
    }

    fn unary(&mut self, op: &str, argument: &Expression<'_>) -> Result<Js, PrintError> {
        let arg = self.expr(argument)?;
        let parent = Js { code: String::new(), prec: 15, op: None };
        let needs = arg.needs_parens(&parent, false);
        // astring: space after a word operator (even when the arg is parenthesized: `typeof (...)`),
        // or between same-sign prefix `+`/`-`.
        let space = if op.len() > 1 || (!needs && same_sign_prefix(op, argument)) { " " } else { "" };
        let code = if needs {
            format!("{}{space}({})", op, arg.code)
        } else {
            format!("{op}{space}{}", arg.code)
        };
        Ok(Js { code, prec: 15, op: None })
    }

    fn update(&mut self, op: &str, prefix: bool, argument: &oxc_ast::ast::SimpleAssignmentTarget<'_>) -> Result<Js, PrintError> {
        let arg = self.simple_target(argument)?;
        let code = if prefix { format!("{op}{arg}") } else { format!("{arg}{op}") };
        Ok(Js { code, prec: 16, op: None })
    }

    fn binary(&mut self, op: &'static str, logical: bool, left: &Expression<'_>, right: &Expression<'_>) -> Result<Js, PrintError> {
        let prec = if logical { LOGICAL } else { BINARY };
        let parent = Js { code: String::new(), prec, op: Some(op) };
        let l = self.expr(left)?.operand_of(&parent, false);
        let r = self.expr(right)?.operand_of(&parent, true);
        let code = format!("{l} {op} {r}");
        Ok(Js { code: if op == "in" { format!("({code})") } else { code }, prec, op: Some(op) })
    }

    fn arrow(&mut self, a: &oxc_ast::ast::ArrowFunctionExpression<'_>) -> Result<Js, PrintError> {
        let mut code = String::new();
        if a.r#async { code.push_str("async "); }
        code.push_str(&self.formal_params(&a.params)?);
        code.push_str(" => ");
        match &a.body {
            ArrowFunctionBody::FunctionBody(_) => {
                return Err(PrintError("arrow function block body is rejected by validation"));
            }
            other => {
                let e = other.to_expression();
                if matches!(e, Expression::ObjectExpression(_)) {
                    code.push('(');
                    code.push_str(&self.expr(e)?.code);
                    code.push(')');
                } else {
                    // astring prints the body without formatExpression (no extra parens).
                    code.push_str(&self.expr(e)?.code);
                }
            }
        }
        Ok(Js { code, prec: NEEDS_PARENTHESES, op: None })
    }

    fn formal_params(&mut self, params: &FormalParameters<'_>) -> Result<String, PrintError> {
        let single = params.items.len() == 1
            && params.rest.is_none()
            && params.items[0].initializer.is_none()
            && matches!(params.items[0].pattern, BindingPattern::BindingIdentifier(_));
        if single {
            if let BindingPattern::BindingIdentifier(id) = &params.items[0].pattern {
                return Ok(id.name.as_str().to_string());
            }
        }
        let mut parts = Vec::new();
        for item in &params.items {
            if item.initializer.is_some() {
                return Err(PrintError("default parameters are rejected by the binder"));
            }
            parts.push(self.binding(&item.pattern)?);
        }
        if let Some(rest) = &params.rest {
            parts.push(format!("...{}", self.binding(&rest.rest.argument)?));
        }
        Ok(format!("({})", parts.join(", ")))
    }

    fn binding(&mut self, pat: &BindingPattern<'_>) -> Result<String, PrintError> {
        match pat {
            BindingPattern::BindingIdentifier(id) => Ok(id.name.as_str().to_string()),
            BindingPattern::AssignmentPattern(a) => {
                Ok(format!("{} = {}", self.binding(&a.left)?, self.expr(&a.right)?.code))
            }
            BindingPattern::ObjectPattern(o) => {
                let mut parts = Vec::new();
                for p in &o.properties { parts.push(self.binding_prop(p)?); }
                if let Some(rest) = &o.rest { parts.push(format!("...{}", self.binding(&rest.argument)?)); }
                Ok(format!("{{{}}}", parts.join(", ")))
            }
            BindingPattern::ArrayPattern(a) => {
                let mut parts = Vec::new();
                let mut last_hole = false;
                for el in &a.elements {
                    last_hole = el.is_none();
                    parts.push(match el {
                        Some(el) => self.binding(el)?,
                        None => String::new(),
                    });
                }
                if let Some(rest) = &a.rest { parts.push(format!("...{}", self.binding(&rest.argument)?)); last_hole = false; }
                let mut inner = parts.join(", ");
                if last_hole && !a.elements.is_empty() { inner.push_str(", "); }
                Ok(format!("[{inner}]"))
            }
        }
    }

    fn binding_prop(&mut self, p: &BindingProperty<'_>) -> Result<String, PrintError> {
        let value = self.binding(&p.value)?;
        if p.shorthand { return Ok(value); }
        let key = if p.computed {
            format!("[{}]", self.property_key(&p.key)?)
        } else {
            self.property_key(&p.key)?
        };
        Ok(format!("{key}: {value}"))
    }

    fn assignment_target(&mut self, t: &AssignmentTarget<'_>) -> Result<String, PrintError> {
        match t {
            AssignmentTarget::AssignmentTargetIdentifier(id) => self.identifier(id.span, id.name.as_str()).map(|j| j.code),
            AssignmentTarget::ComputedMemberExpression(m) => Ok(self.computed_member(m)?.code),
            AssignmentTarget::StaticMemberExpression(m) => Ok(self.static_member(m)?.code),
            AssignmentTarget::PrivateFieldExpression(m) => Ok(self.private_member(m)?.code),
            AssignmentTarget::TSNonNullExpression(n) => Ok(self.expr(&n.expression)?.code),
            AssignmentTarget::TSAsExpression(n) => Ok(self.expr(&n.expression)?.code),
            AssignmentTarget::TSSatisfiesExpression(n) => Ok(self.expr(&n.expression)?.code),
            AssignmentTarget::TSTypeAssertion(n) => Ok(self.expr(&n.expression)?.code),
            AssignmentTarget::ArrayAssignmentTarget(a) => self.array_assign(a),
            AssignmentTarget::ObjectAssignmentTarget(o) => self.object_assign(o),
        }
    }

    fn assignment_maybe(&mut self, t: &AssignmentTargetMaybeDefault<'_>) -> Result<String, PrintError> {
        match t {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => {
                Ok(format!("{} = {}", self.assignment_target(&d.binding)?, self.expr(&d.init)?.code))
            }
            other => self.assignment_target(
                other.as_assignment_target().ok_or(PrintError("inherited assignment target"))?,
            ),
        }
    }

    fn object_assign(&mut self, o: &ObjectAssignmentTarget<'_>) -> Result<String, PrintError> {
        let mut parts = Vec::new();
        for p in &o.properties {
            parts.push(match p {
                AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                    let bound = self.identifier(p.binding.span, p.binding.name.as_str())?.code;
                    // astring shorthand: print the value only. After binding, `{a} = x` is `{$cmp.a} = $cmp.x`
                    // and `{a = 1} = x` is `{$cmp.a = 1} = $cmp.x` (D-0224).
                    if let Some(init) = &p.init {
                        format!("{} = {}", bound, self.expr(init)?.code)
                    } else {
                        bound
                    }
                }
                AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => {
                    let key = if p.computed {
                        format!("[{}]", self.property_key(&p.name)?)
                    } else {
                        self.property_key(&p.name)?
                    };
                    format!("{key}: {}", self.assignment_maybe(&p.binding)?)
                }
            });
        }
        if let Some(rest) = &o.rest {
            parts.push(format!("...{}", self.assignment_target(&rest.target)?));
        }
        Ok(format!("{{{}}}", parts.join(", ")))
    }

    fn array_assign(&mut self, a: &ArrayAssignmentTarget<'_>) -> Result<String, PrintError> {
        let mut parts = Vec::new();
        let mut last_hole = false;
        for el in &a.elements {
            last_hole = el.is_none();
            parts.push(match el {
                None => String::new(),
                Some(el) => self.assignment_maybe(el)?,
            });
        }
        if let Some(rest) = &a.rest {
            parts.push(format!("...{}", self.assignment_target(&rest.target)?));
            last_hole = false;
        }
        let mut inner = parts.join(", ");
        if last_hole && !a.elements.is_empty() { inner.push_str(", "); }
        Ok(format!("[{inner}]"))
    }

    fn simple_target(&mut self, t: &oxc_ast::ast::SimpleAssignmentTarget<'_>) -> Result<String, PrintError> {
        match t {
            oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.identifier(id.span, id.name.as_str()).map(|j| j.code)
            }
            oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(m) => Ok(self.computed_member(m)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(m) => Ok(self.static_member(m)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::PrivateFieldExpression(m) => Ok(self.private_member(m)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::TSNonNullExpression(n) => Ok(self.expr(&n.expression)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::TSAsExpression(n) => Ok(self.expr(&n.expression)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::TSSatisfiesExpression(n) => Ok(self.expr(&n.expression)?.code),
            oxc_ast::ast::SimpleAssignmentTarget::TSTypeAssertion(n) => Ok(self.expr(&n.expression)?.code),
        }
    }

    fn sequence(&mut self, args: &[Argument<'_>]) -> Result<String, PrintError> {
        let mut parts = Vec::new();
        for a in args {
            parts.push(match a {
                Argument::SpreadElement(s) => format!("...{}", self.expr(&s.argument)?.code),
                other => self.expr(other.to_expression())?.code,
            });
        }
        Ok(format!("({})", parts.join(", ")))
    }

    fn format(&mut self, e: &Expression<'_>, parent: Js, right: bool) -> Result<String, PrintError> {
        Ok(self.expr(e)?.operand_of(&parent, right))
    }
}

fn lit(code: &str) -> Js { Js { code: code.into(), prec: 18, op: None } }

fn member_parent() -> Js { Js { code: String::new(), prec: MEMBER, op: None } }
fn call_parent() -> Js { Js { code: String::new(), prec: MEMBER, op: None } }
fn tagged_parent() -> Js { Js { code: String::new(), prec: 20, op: None } }

fn same_sign_prefix(op: &str, argument: &Expression<'_>) -> bool {
    match (op, argument) {
        ("+" | "-", Expression::UnaryExpression(u)) if u.operator.as_str() == op => true,
        ("+" | "-", Expression::UpdateExpression(u)) if u.prefix && u.operator.as_str().starts_with(op) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::expression::bind;
    use crate::parser::expression_complex::build;
    use lwc_shared::template_ir::SourceLocation;
    use oxc_allocator::Allocator;

    const AT: SourceLocation = SourceLocation {
        start_line: 1, start_column: 1, end_line: 1, end_column: 80, start: 0, end: 79,
    };

    fn printed(raw: &str, locals: &[&str]) -> String {
        let alloc = Allocator::default();
        let c = build(&alloc, &format!("{{{raw}}}"), AT).unwrap_or_else(|e| panic!("{raw}: {e:?}"));
        let plan = bind(&c, &|n| locals.iter().any(|s| *s == n)).unwrap_or_else(|e| panic!("{raw}: {e:?}"));
        print_expr(c.ast, &plan, "$cmp", 0).unwrap_or_else(|e| panic!("{raw}: {e:?}")).code
    }

    /// Each expected string is astring 1.9.0's output for the same ESTree (node, the pinned oracle).
    #[test]
    fn matches_astring() {
        let m = Js::member;
        // genSanitizedHtmlExpr shapes (codegen.ts:510-560).
        let lit = Js::binary("||", m("a.s"), Js::assign(m("a.s"), "f(1)"));
        assert_eq!(lit.code, "a.s || (a.s = f(1))");
        let cond = Js::conditional(Js::binary("!==", m("a.r"), Js::assign(m("a.r"), "x")), Js::assign(m("a.s"), "f(x)"), m("a.s"));
        assert_eq!(cond.code, "a.r !== (a.r = x) ? a.s = f(x) : a.s");
        // Same-precedence operands: left stays bare, right gets parentheses; `??` never mixes.
        assert_eq!(Js::binary("-", m("a"), Js::binary("-", m("b"), m("c"))).code, "a - (b - c)");
        assert_eq!(Js::binary("-", Js::binary("-", m("a"), m("b")), m("c")).code, "a - b - c");
        assert_eq!(Js::binary("||", Js::binary("??", m("a"), m("b")), m("c")).code, "(a ?? b) || c");
        assert_eq!(Js::binary("**", Js::binary("**", m("a"), m("b")), m("c")).code, "(a ** b) ** c");
        assert_eq!(Js::conditional(Js::conditional(m("a"), m("b"), m("c")), m("d"), m("e")).code, "(a ? b : c) ? d : e");
        assert_eq!(Js::binary("in", m("a"), m("b")).code, "(a in b)");
    }

    /// Unbound (empty plan) and bound spellings captured from astring 1.9.0 + upstream bindExpression.
    #[test]
    fn print_expr_matches_astring_table() {
        // Binding with no locals prefixes free identifiers. Nested `undefined` is skipped.
        let cases: &[(&str, &str)] = &[
            ("foo", "$cmp.foo"),
            ("foo.bar", "$cmp.foo.bar"),
            ("foo[bar]", "$cmp.foo[$cmp.bar]"),
            ("foo?.bar", "$cmp.foo?.bar"),
            ("foo?.(bar)", "$cmp.foo?.($cmp.bar)"),
            ("1 + 2", "1 + 2"),
            ("a + b * c", "$cmp.a + $cmp.b * $cmp.c"),
            ("(a + b) * c", "($cmp.a + $cmp.b) * $cmp.c"),
            // Member object: astring wraps the binary once. Extra parens (`((a + b)).c`) fail these.
            ("(a + b).c", "($cmp.a + $cmp.b).c"),
            ("(a + b)[c]", "($cmp.a + $cmp.b)[$cmp.c]"),
            ("a ** b ** c", "$cmp.a ** $cmp.b ** $cmp.c"),
            ("(a ** b) ** c", "($cmp.a ** $cmp.b) ** $cmp.c"),
            ("a ** -b", "$cmp.a ** -$cmp.b"),
            ("(-a) ** b", "(-$cmp.a) ** $cmp.b"),
            ("a ? b : c", "$cmp.a ? $cmp.b : $cmp.c"),
            ("(a ? b : c) ? d : e", "($cmp.a ? $cmp.b : $cmp.c) ? $cmp.d : $cmp.e"),
            ("a ? (b = c) : d", "$cmp.a ? $cmp.b = $cmp.c : $cmp.d"),
            ("!a", "!$cmp.a"),
            ("typeof a", "typeof $cmp.a"),
            ("void 0", "void 0"),
            ("- -a", "- -$cmp.a"),
            ("+ +a", "+ +$cmp.a"),
            ("-+a", "-+$cmp.a"),
            ("foo()", "$cmp.foo()"),
            ("foo(a, b)", "$cmp.foo($cmp.a, $cmp.b)"),
            ("foo`bar`", "($cmp.foo)`bar`"),
            ("foo`bar${baz}`", "($cmp.foo)`bar${$cmp.baz}`"),
            ("`t${a}`", "`t${$cmp.a}`"),
            ("`${name}: ${occupation}`", "`${$cmp.name}: ${$cmp.occupation}`"),
            ("[a, b]", "[$cmp.a, $cmp.b]"),
            ("[a,, b]", "[$cmp.a, , $cmp.b]"),
            ("[a,]", "[$cmp.a]"),
            ("{a: 1}", "{\n  a: 1\n}"),
            ("{ obj: 'literal' }", "{\n  obj: 'literal'\n}"),
            ("{a, b: c, [d]: e}", "{\n  $cmp.a,\n  b: $cmp.c,\n  [d]: $cmp.e\n}"),
            ("{a: {b: 1}}", "{\n  a: {\n    b: 1\n  }\n}"),
            ("foo => foo", "foo => foo"),
            ("(foo) => foo", "foo => foo"),
            ("(a, b) => a", "(a, b) => a"),
            ("({a}) => a", "({a}) => a"),
            ("x => ({y: x})", "x => ({\n  y: x\n})"),
            ("xs.map(x => x + y)", "$cmp.xs.map(x => x + $cmp.y)"),
            ("a && b && c", "$cmp.a && $cmp.b && $cmp.c"),
            ("a + b + c", "$cmp.a + $cmp.b + $cmp.c"),
            ("a - (b - c)", "$cmp.a - ($cmp.b - $cmp.c)"),
            ("a in b", "($cmp.a in $cmp.b)"),
            ("a instanceof b", "$cmp.a instanceof $cmp.b"),
            ("0b11", "0b11"),
            ("0x10", "0x10"),
            ("1e21", "1e21"),
            ("1.50", "1.50"),
            ("1_000", "1_000"),
            (r#""a\u0041""#, r#""a\u0041""#),
            ("'x'", "'x'"),
            ("true", "true"),
            ("false", "false"),
            ("null", "null"),
            ("undefined", "$cmp.undefined"),
            ("() => ({a} = x)", "() => {$cmp.a} = $cmp.x"),
            ("() => ([a] = x)", "() => [$cmp.a] = $cmp.x"),
            ("() => ({a = 1} = x)", "() => {$cmp.a = 1} = $cmp.x"),
            ("() => ({a: b} = x)", "() => {a: $cmp.b} = $cmp.x"),
            ("() => ({...r} = x)", "() => {...$cmp.r} = $cmp.x"),
            ("() => ([a, , b, ...r] = x)", "() => [$cmp.a, , $cmp.b, ...$cmp.r] = $cmp.x"),
            ("() => ({a: {b}} = x)", "() => {a: {$cmp.b}} = $cmp.x"),
            ("a === undefined", "$cmp.a === undefined"),
            ("foo.bar.baz", "$cmp.foo.bar.baz"),
            ("arr[index]", "$cmp.arr[$cmp.index]"),
        ];
        for (src, want) in cases {
            assert_eq!(printed(src, &[]), *want, "{src}");
        }
        // Cond 1: a mutation that prints `((a + b)).c` / `((a + b))[c]` fails these rows.
        assert_eq!(printed("(a + b).c", &[]), "($cmp.a + $cmp.b).c");
        assert_eq!(printed("(a + b)[c]", &[]), "($cmp.a + $cmp.b)[$cmp.c]");
        assert!(!printed("(a + b).c", &[]).contains("(("), "{}", printed("(a + b).c", &[]));
        assert!(!printed("(a + b)[c]", &[]).contains("(("), "{}", printed("(a + b)[c]", &[]));
        assert_eq!(printed("foo", &["foo"]), "foo");
        assert_eq!(printed("item.id === selectedId", &["item"]), "item.id === $cmp.selectedId");
    }

    #[test]
    fn number_to_js_string_matches_ecmascript() {
        // oxc_ecmascript::ToJsString for NumericLiteral: 0 and -0 → "0"; 1e21 → "1e+21".
        assert_eq!(js_number(0.0), "0");
        assert_eq!(js_number(-0.0), "0");
        assert_eq!(js_number(1.5), "1.5");
        assert_eq!(js_number(75.0), "75");
        assert_eq!(js_number(1e21), "1e+21");
        assert_eq!(js_number(1e-7), "1e-7");
    }

    #[test]
    fn bind_plan_span_must_be_printed() {
        let alloc = Allocator::default();
        let c = build(&alloc, "{foo}", AT).unwrap();
        let plan = bind(&c, &|_| false).unwrap();
        // Extra span that is not on the tree: the printer must refuse to drop it.
        let extra = crate::codegen::expression::bind(&c, &|_| false).unwrap();
        // Reconstruct a plan with an additional unused span via printing against a forged set:
        // print_expr asserts every planned span appears; forging is done by binding `foo` then
        // asking the printer to treat a different expression that has no identifiers.
        let empty = build(&alloc, "{1}", AT).unwrap();
        let err = print_expr(empty.ast, &plan, "$cmp", 0).unwrap_err();
        assert_eq!(err, PrintError("BindPlan span was not printed"));
        let _ = extra;
    }
}
