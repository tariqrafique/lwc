//! RATIFIED template IR shared by lwc-template (producer) and lwc-ssr (consumer).
//! Mirrors `@lwc/template-compiler` `shared/types.ts`.
//!
//! AB-D0004 (Option A): a complex expression carries its oxc AST, borrowed from the per-parse arena the
//! compile entry owns. `'a` is that arena's lifetime. The oxc AST has `Cell`s, so the IR is `!Sync` and never
//! crosses a thread (asserted below).

pub use crate::SourceLocation;
use oxc_span::ContentEq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementSourceLocation {
    pub location: SourceLocation,
    pub start_tag: SourceLocation,
    pub end_tag: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Literal {
    pub value: LiteralValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemberExpression<'a> {
    pub object: Box<Expression<'a>>,
    pub property: Box<Expression<'a>>,
    pub computed: bool,
}

/// A complex template expression (experimental_complex_expressions; TS `ComplexExpression`, an
/// ESTree node): the oxc AST parsed with `preserve_parens: false`, plus its source text.
#[derive(Debug, Clone, Copy)]
pub struct ComplexExpression<'a> {
    /// The parsed expression, arena-owned.
    pub ast: &'a oxc_ast::ast::Expression<'a>,
    /// The expression source between the braces (trimmed), arena-owned; the AST's spans index it.
    pub source: &'a str,
    /// The `{…}` token as written (TS `raw`), arena-owned.
    pub raw: &'a str,
    /// Location of the owning attribute / text node / directive.
    pub location: SourceLocation,
}

/// Structural equality of the expressions (oxc has `ContentEq`, not `PartialEq`).
impl PartialEq for ComplexExpression<'_> {
    fn eq(&self, o: &Self) -> bool {
        self.ast.content_eq(o.ast)
    }
}

/// A top-level ESTree `Literal` (`isLiteral` / `isStringLiteral` in shared/ast.ts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiteralView<'a> {
    String(&'a str),
    Number(f64),
    Bool(bool),
    Null,
}

impl ComplexExpression<'_> {
    /// `node.type === 'Literal'`. BigInt and RegExp are rejected by validation, so they are `None`.
    pub fn as_literal(&self) -> Option<LiteralView<'_>> {
        match self.ast {
            oxc_ast::ast::Expression::StringLiteral(l) => l.value.as_str().map(LiteralView::String),
            oxc_ast::ast::Expression::NumericLiteral(l) => Some(LiteralView::Number(l.value)),
            oxc_ast::ast::Expression::BooleanLiteral(l) => Some(LiteralView::Bool(l.value)),
            oxc_ast::ast::Expression::NullLiteral(_) => Some(LiteralView::Null),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression<'a> {
    Identifier(Identifier),
    Member(MemberExpression<'a>),
    Literal(Literal),
    Complex(ComplexExpression<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue<'a> {
    Literal(Literal),
    Expression(Expression<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute<'a> {
    pub name: String,
    pub value: AttributeValue<'a>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Property<'a> {
    pub name: String,
    pub attribute_name: String,
    pub value: AttributeValue<'a>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EventListener<'a> {
    pub name: String,
    pub handler: Expression<'a>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Shadow,
    Light,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElementDirective<'a> {
    Key(Expression<'a>),
    Dynamic(Expression<'a>),
    Is(Expression<'a>),
    Dom(LiteralValue),
    Spread(Expression<'a>),
    OnDirective(Expression<'a>),
    InnerHtml(AttributeValue<'a>),
    Ref(String),
    SlotBind(Expression<'a>),
    SlotData(Expression<'a>),
    External,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RootDirective {
    RenderMode(RenderMode),
    PreserveComments(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextValue<'a> {
    Literal(String),
    Expression(Expression<'a>),
}

/// Static text uses `TextValue::Literal` (SSR relies on this).
#[derive(Debug, Clone, PartialEq)]
pub struct Text<'a> {
    pub value: TextValue<'a>,
    pub raw: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub value: String,
    pub raw: String,
    pub location: SourceLocation,
}

macro_rules! base_parent {
    ($($n:ident),*) => { $(
        #[derive(Debug, Clone, PartialEq)]
        pub struct $n<'a> {
            pub name: String,
            pub namespace: String,
            pub attributes: Vec<Attribute<'a>>,
            pub properties: Vec<Property<'a>>,
            pub directives: Vec<ElementDirective<'a>>,
            pub listeners: Vec<EventListener<'a>>,
            pub children: Vec<ChildNode<'a>>,
            pub location: ElementSourceLocation,
        }
    )* };
}
base_parent!(Element, Component, ExternalComponent, LwcComponent);

/// `<slot>`; `slot_name` mirrors TS `slotName` (empty string = default slot).
#[derive(Debug, Clone, PartialEq)]
pub struct Slot<'a> {
    pub name: String,
    pub namespace: String,
    pub attributes: Vec<Attribute<'a>>,
    pub properties: Vec<Property<'a>>,
    pub directives: Vec<ElementDirective<'a>>,
    pub listeners: Vec<EventListener<'a>>,
    pub children: Vec<ChildNode<'a>>,
    pub location: ElementSourceLocation,
    pub slot_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct If<'a> {
    pub modifier: String,
    pub condition: Expression<'a>,
    pub children: Vec<ChildNode<'a>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct IfBlock<'a> {
    pub condition: Expression<'a>,
    pub children: Vec<ChildNode<'a>>,
    pub else_: Option<Box<ElseBranch<'a>>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ElseifBlock<'a> {
    pub condition: Expression<'a>,
    pub children: Vec<ChildNode<'a>>,
    pub else_: Option<Box<ElseBranch<'a>>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ElseBlock<'a> {
    pub children: Vec<ChildNode<'a>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch<'a> {
    Elseif(ElseifBlock<'a>),
    Else(ElseBlock<'a>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct ForEach<'a> {
    pub expression: Expression<'a>,
    pub item: Identifier,
    pub index: Option<Identifier>,
    pub children: Vec<ChildNode<'a>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ForOf<'a> {
    pub expression: Expression<'a>,
    pub iterator: Identifier,
    pub children: Vec<ChildNode<'a>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedSlotFragment<'a> {
    pub slot_data: Expression<'a>,
    pub slot_name: AttributeValue<'a>,
    pub children: Vec<ChildNode<'a>>,
    pub location: SourceLocation,
    pub directive_location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildNode<'a> {
    Element(Element<'a>),
    Component(Component<'a>),
    ExternalComponent(ExternalComponent<'a>),
    Slot(Slot<'a>),
    LwcComponent(LwcComponent<'a>),
    Text(Text<'a>),
    Comment(Comment),
    ForEach(ForEach<'a>),
    ForOf(ForOf<'a>),
    If(If<'a>),
    IfBlock(IfBlock<'a>),
    ElseifBlock(ElseifBlock<'a>),
    ElseBlock(ElseBlock<'a>),
    ScopedSlotFragment(ScopedSlotFragment<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Root<'a> {
    pub location: ElementSourceLocation,
    pub directives: Vec<RootDirective>,
    pub children: Vec<ChildNode<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node<'a> {
    Root(Root<'a>),
    Child(ChildNode<'a>),
}

/// AB-D0004 condition (6): the IR is `!Sync` (the borrowed oxc AST has `Cell`s), so it cannot cross a
/// thread; the arena and the IR both live inside one compile call. Compile-time check: if `T: Sync`,
/// both impls apply and the `_` below is ambiguous.
#[allow(dead_code)]
trait AmbiguousIfSync<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSync<()> for T {}
impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
const _: fn() = || {
    let _ = <ComplexExpression<'static> as AmbiguousIfSync<_>>::check;
    let _ = <Root<'static> as AmbiguousIfSync<_>>::check;
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_text_literal() {
        let at = SourceLocation {
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 1,
            start: 0,
            end: 1,
        };
        let location = ElementSourceLocation {
            location: at,
            start_tag: at,
            end_tag: None,
        };
        let r = Root {
            location,
            directives: Vec::new(),
            children: vec![ChildNode::Text(Text {
                value: TextValue::Literal("a".into()),
                raw: "a".into(),
                location: at,
            })],
        };
        assert!(
            matches!(&r.children[0], ChildNode::Text(Text { value: TextValue::Literal(s), .. }) if s == "a")
        );
    }
}
