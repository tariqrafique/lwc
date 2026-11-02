//! RATIFIED template IR shared by lwc-template (producer) and lwc-ssr (consumer).
//! Mirrors `@lwc/template-compiler` `shared/types.ts`. ComplexExpression is opaque source text.

pub use crate::SourceLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ElementSourceLocation {
    pub location: SourceLocation,
    pub start_tag: SourceLocation,
    pub end_tag: Option<SourceLocation>,
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self { start_line: 1, start_column: 1, end_line: 1, end_column: 1, start: 0, end: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue { String(String), Bool(bool) }

#[derive(Debug, Clone, PartialEq)]
pub struct Literal { pub value: LiteralValue }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier { pub name: String }

#[derive(Debug, Clone, PartialEq)]
pub struct MemberExpression {
    pub object: Box<Expression>,
    pub property: Box<Expression>,
    pub computed: bool,
}

/// Opaque complex-expression source (experimental_complex_expressions).
pub type ComplexExpression = String;

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Identifier(Identifier),
    Member(MemberExpression),
    Literal(Literal),
    Complex(ComplexExpression),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue { Literal(Literal), Expression(Expression) }

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute { pub name: String, pub value: AttributeValue, pub location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct Property { pub name: String, pub attribute_name: String, pub value: AttributeValue, pub location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct EventListener { pub name: String, pub handler: Expression, pub location: SourceLocation }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode { #[default] Shadow, Light }

#[derive(Debug, Clone, PartialEq)]
pub enum ElementDirective {
    Key(Expression),
    Dynamic(Expression),
    Is(Expression),
    Dom(LiteralValue),
    Spread(Expression),
    OnDirective(Expression),
    InnerHtml(AttributeValue),
    Ref(String),
    SlotBind(Expression),
    SlotData(Expression),
    External,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RootDirective {
    RenderMode(RenderMode),
    PreserveComments(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextValue { Literal(String), Expression(Expression), Complex(ComplexExpression) }

/// Static text uses `TextValue::Literal` (SSR relies on this).
#[derive(Debug, Clone, PartialEq)]
pub struct Text { pub value: TextValue, pub raw: String, pub location: SourceLocation }

#[derive(Debug, Clone, PartialEq)]
pub struct Comment { pub value: String, pub raw: String, pub location: SourceLocation }

macro_rules! base_parent {
    ($($n:ident),*) => { $(
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $n {
            pub name: String,
            pub namespace: String,
            pub attributes: Vec<Attribute>,
            pub properties: Vec<Property>,
            pub directives: Vec<ElementDirective>,
            pub listeners: Vec<EventListener>,
            pub children: Vec<ChildNode>,
            pub location: ElementSourceLocation,
        }
    )* };
}
base_parent!(Element, Component, ExternalComponent, LwcComponent);

/// `<slot>`; `slot_name` mirrors TS `slotName` (empty string = default slot).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slot {
    pub name: String,
    pub namespace: String,
    pub attributes: Vec<Attribute>,
    pub properties: Vec<Property>,
    pub directives: Vec<ElementDirective>,
    pub listeners: Vec<EventListener>,
    pub children: Vec<ChildNode>,
    pub location: ElementSourceLocation,
    pub slot_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct If { pub modifier: String, pub condition: Expression, pub children: Vec<ChildNode>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct IfBlock { pub condition: Expression, pub children: Vec<ChildNode>, pub else_: Option<Box<ElseBranch>>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct ElseifBlock { pub condition: Expression, pub children: Vec<ChildNode>, pub else_: Option<Box<ElseBranch>>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct ElseBlock { pub children: Vec<ChildNode>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch { Elseif(ElseifBlock), Else(ElseBlock) }
#[derive(Debug, Clone, PartialEq)]
pub struct ForEach { pub expression: Expression, pub item: Identifier, pub index: Option<Identifier>, pub children: Vec<ChildNode>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct ForOf { pub expression: Expression, pub iterator: Identifier, pub children: Vec<ChildNode>, pub location: SourceLocation, pub directive_location: SourceLocation }
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedSlotFragment { pub slot_data: Expression, pub slot_name: AttributeValue, pub children: Vec<ChildNode>, pub location: SourceLocation, pub directive_location: SourceLocation }

#[derive(Debug, Clone, PartialEq)]
pub enum ChildNode {
    Element(Element), Component(Component), ExternalComponent(ExternalComponent), Slot(Slot),
    LwcComponent(LwcComponent), Text(Text), Comment(Comment), ForEach(ForEach), ForOf(ForOf),
    If(If), IfBlock(IfBlock), ElseifBlock(ElseifBlock), ElseBlock(ElseBlock),
    ScopedSlotFragment(ScopedSlotFragment),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Root {
    pub location: ElementSourceLocation,
    pub directives: Vec<RootDirective>,
    pub children: Vec<ChildNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node { Root(Root), Child(ChildNode) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_text_literal() {
        let r = Root { children: vec![ChildNode::Text(Text { value: TextValue::Literal("a".into()), raw: "a".into(), location: SourceLocation::default() })], ..Default::default() };
        assert!(matches!(&r.children[0], ChildNode::Text(Text { value: TextValue::Literal(s), .. }) if s == "a"));
    }
}
