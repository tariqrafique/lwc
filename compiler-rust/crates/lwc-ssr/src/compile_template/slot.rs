//! Port of transformers/slot.ts (all modes; sync/async match transmogrified `compileTemplateForSSR(..., mode)`).
use super::{element::{self, ElView}, emit_children, expr, js_str, Cx, TemplateOptions};
use lwc_shared::template_ir::{Attribute, AttributeValue, ElementDirective, Expression, LiteralValue, Slot};

/// shared.ts bAttributeValue: absent -> `null`, literal string -> string, bool literal -> `''`,
/// expression -> `b.memberExpression(b.literal('instance'), expr)`, which astring prints as
/// `("instance").<raw expr>` (a property read on the string "instance"; ported verbatim).
pub(crate) fn attr_value(attrs: &[Attribute], name: &str, sc: &[String]) -> String {
    match attrs.iter().find(|a| a.name == name).map(|a| &a.value) {
        None => "null".into(),
        Some(AttributeValue::Literal(l)) => match &l.value { LiteralValue::String(s) => js_str(s), _ => "''".into() },
        Some(AttributeValue::Expression(e)) => match raw_expr(e) { Some(r) => format!("(\"instance\").{r}"), None => expr(e, sc) },
    }
}

fn raw_expr(e: &Expression) -> Option<String> {
    match e {
        Expression::Identifier(i) => Some(i.name.clone()),
        Expression::Member(m) => {
            let obj = raw_expr(&m.object)?;
            if m.computed { Some(format!("{obj}[{}]", raw_expr(&m.property)?)) } else { Some(format!("{obj}.{}", raw_expr(&m.property)?)) }
        }
        _ => None,
    }
}

pub(crate) fn emit_slot(s: &Slot, o: TemplateOptions, out: &mut Vec<String>, cx: &mut Cx, sc: &mut Vec<String>) {
    let bound = s.directives.iter().find_map(|d| if let ElementDirective::SlotBind(e) = d { Some(expr(e, sc)) } else { None });
    let scoped = bound.is_some();
    out.push("if (isLightDom) {".into());
    out.push(format!("const isScopedSlot = {scoped};"));
    out.push(format!("const isSlotted = {};", cx.slotted));
    let slot_name = attr_value(&s.attributes, "name", sc);
    out.push(format!("const slotName = {slot_name} ?? \"\";"));
    out.push("const lightGenerators = lightSlottedContent?.[slotName];".into());
    out.push("const scopedGenerators = scopedSlottedContent?.[slotName];".into());
    out.push("const mismatchedSlots = isScopedSlot ? lightGenerators : scopedGenerators;".into());
    out.push("const generators = isScopedSlot ? scopedGenerators : lightGenerators;".into());
    out.push(format!("const danglingSlotName = !isScopedSlot ? {} || slotAttributeValue : null;", attr_value(&s.attributes, "slot", sc)));
    let m = o.y("'<!---->'");
    let bk = format!("if (!isSlotted) {{ {m} if (isScopedSlot && generators) {{ {m} }} }}");
    out.push(bk.clone());
    out.push("if (generators) {".into());
    out.push("for (let i = 0; i < generators.length; i++) {".into());
    // Null scoped-data arg is omitted, as in the JS output.
    out.push(match &bound { Some(b) => o.yd(&format!("generators[i](contextfulParent, {b}, danglingSlotName)")), None => o.yd("generators[i](contextfulParent, danglingSlotName)") });
    out.push(format!("if (isScopedSlot && i < generators.length - 1) {{ {m} {m} }}"));
    out.push("}".into());
    out.push("} else if (!mismatchedSlots) {".into());
    emit_children(&s.children, o, out, cx, sc);
    out.push("}".into());
    out.push(bk);
    out.push("} else {".into());
    element::emit_element(&ElView { name: &s.name, namespace: &s.namespace, attributes: &s.attributes, properties: &s.properties,
        directives: &s.directives, children: &s.children, is_element: false }, o, out, cx, sc);
    out.push("}".into());
}

