//! compile-js/index.ts `compileJS`: one oxc parse + semantic and one pre-order [`Visit`] (`Index`)
//! decide `isLWC`, the component class and the default export, and rewrite imports (lwc_import,
//! remove_decorator_import, stylesheets), validate and collect `@api` / `@track` / `@wire` members
//! (decorators) and remove decorators, rewrite component class members (constructor, `super(...)`,
//! lifecycle methods) and reject reserved `__lwc` identifiers in any module, as source edits applied
//! with `lwc_shared::edits`. Non-LWC modules get only the import and decorator edits. The text passes
//! after that (base-class registration, `setStaticInternals`) are being ported slice by slice (AB-0002).

use lwc_shared::edits::{apply, Edit};
use lwc_shared::errors::{LWCClassErrors, SsrCompilerErrors};
use lwc_shared::js_literal::json_string;
use lwc_shared::meta::version_comment;
use lwc_shared::{CompileError, CompileOptions, CompileResult};
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast::AstKind;
use oxc::ast_visit::{walk, Visit};
use oxc::parser::Parser;
use oxc::semantic::{AstNodes, Scoping, SemanticBuilder};
use oxc::span::{GetSpan, SourceType, Span};
use oxc::syntax::scope::ScopeFlags;
use std::path::Path;

mod decorators;
mod errors;
mod import_manager;
mod lwc_import;
mod remove_decorator_import;
mod stylesheets;

use decorators::api::is_api_decorator;
use decorators::api::validate::{validate_api_method, validate_api_property, validate_property_unique, validate_unique_method, ApiDefinition, Definition, PublicProperties};
use decorators::validate_unique_decorator;
use decorators::wire::{catalog_wire_adapter, is_wire_decorator, wire_adapters_plumbing, WireAdapter, WiredMember};
use import_manager::{import_declaration, ImportManager, Spec};

/// Port of `@lwc/shared` `generateCustomElementTagName`.
pub fn generate_custom_element_tag_name(namespace: &str, name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        if c.is_ascii_lowercase() && i + 1 < chars.len() && chars[i + 1].is_ascii_uppercase() {
            out.push('-');
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        i += 1;
    }
    format!("{namespace}-{}", out.to_lowercase())
}

fn module_stem(filename: &str, fallback: &str) -> String {
    Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

/// compile-js/index.ts: the id given to a default-exported anonymous class declaration.
const DEFAULT_COMPONENT_NAME: &str = "DefaultComponentName";
/// compile-js/index.ts Program.leave: the binding an `export default <expression>` is extracted into.
const DEFAULT_EXPORT_BINDING: &str = "__lwcDefaultExport";
/// index.ts MethodDefinition `constructor` / Super: the param a component constructor gains and
/// forwards to `super(...)`.
const PROPS_ARG: &str = "propsAvailableAtConstruction";
/// index.ts Identifier visitor: user identifiers may not start with this.
const RESERVED_IDENTIFIER_PREFIX: &str = "__lwc";
/// register-base-class-props.ts `REGISTER_PUBLIC_PROPERTIES`, the local the runtime export is imported as.
const REGISTER_PUBLIC_PROPERTIES: &str = "__registerPublicProperties";
/// index.ts ImportExpression: the loader's `load` export is imported as this.
const LOAD: &str = "__lwcLoad";

/// compile-js/types.ts `ComponentMetaState`: the fields the ported index.ts visitors fill.
#[derive(Default)]
struct ComponentMetaState {
    lightning_element_identifier: Option<String>,
    /// index.ts `isLWC` + `lwcClassName`: `Some` once a class matched the component visitors.
    component: Option<ComponentClass>,
    /// index.ts `lwcDefaultExportName` (set for any module's default export, LWC or not).
    lwc_default_export_name: Option<String>,
    /// `export default <expression>`: (declaration span, expression span), extracted at Program leave.
    export_default_expression: Option<(Span, Span)>,
    /// Class bodies that get index.ts's `LWC_VERSION_COMMENT` (every class the visitors match).
    version_comment_bodies: Vec<Span>,
    /// index.ts `currentComponent`: set when a class matches, cleared on leaving that class only, so
    /// it stays set inside nested classes and is cleared by a nested match's leave.
    current_component: Option<Span>,
    /// index.ts `publicProperties`, filled while `current_component` is set.
    public_properties: PublicProperties,
    /// index.ts `moduleClassNodes`: every class, for base-class `@api` registration.
    module_classes: Vec<ModuleClass>,
    /// index.ts `lwcComponentNode`: the last class that matched.
    lwc_component_node: Option<Span>,
    /// index.ts `wireAdapters`, with the decorated member's span.
    wire_adapters: Vec<(WireAdapter, Span)>,
    /// Where index.ts's MethodDefinition `constructor` / Super visitors append `propsAvailableAtConstruction`
    /// (`params.push` / `arguments.push`): the offset, and whether the list already has an item.
    props_arg_sites: Vec<(u32, bool)>,
}

/// An entry of `moduleClassNodes`, with what `registerBaseClassProps` reads from it.
struct ModuleClass {
    span: Span,
    /// The ClassBody, whose `body.push` the static block is.
    body: Span,
    /// register-base-class-props.ts `ownApiProps`.
    api_props: Vec<String>,
}

/// The matched component class. `name` is upstream's nullable `lwcClassName`: an anonymous class
/// expression outside a variable declarator (`export default (class extends LightningElement {})`)
/// leaves it null, and upstream still compiles via the default export name (generate-markup.ts:54).
struct ComponentClass {
    name: Option<String>,
}

/// What [`index`] decided about the module.
#[derive(Debug, PartialEq)]
enum IndexResult {
    /// index.ts `!state.isLWC`: no `extends` default export and no `extends LightningElement` class expression.
    NotLwc,
    Component {
        /// generate-markup.ts:54 `(state.lwcDefaultExportName ?? state.lwcClassName)!`.
        exported_name: String,
        /// generate-markup.ts:73 `[...publicProperties.keys()]`.
        public_properties: Vec<String>,
        /// index.ts:423 `registerBaseClassProps` returned true: some other class got a static block,
        /// so the `registerPublicProperties` import is unshifted.
        registered_base_class_props: bool,
        /// generate-markup.ts:62 `state.wireAdapters`.
        wire_adapters: Vec<WireAdapter>,
    },
}

/// What index.ts's member visitors recorded for one member.
enum Cataloged {
    Nothing,
    Public,
    Wire(WireAdapter),
}

/// compile-js/index.ts `visitors`: one read-only pre-order walk that fills [`ComponentMetaState`]
/// and records source edits for [`lwc_shared::edits::apply`].
struct Index<'s, 'a> {
    src: &'s str,
    filename: &'s str,
    scoping: &'s Scoping,
    nodes: &'s AstNodes<'a>,
    state: ComponentMetaState,
    edits: Vec<Edit>,
    /// The class declaration that is `export default`'s direct child.
    export_default_class: Option<Span>,
    /// `const X = class …`: the class expression and X.
    declarator_class: Option<(Span, String)>,
    /// The first error a visitor throws; upstream's traversal stops there, so later checks are skipped.
    error: Option<CompileError>,
    /// For each enclosing function (innermost last): whether it is a constructor's value, i.e. what
    /// index.ts's Super visitor asks of `path.getFunctionParent()`. Arrows count as functions.
    functions: Vec<bool>,
    /// `dynamicImports.strictSpecifier` and `.loader` (`""` for none).
    strict_specifier: bool,
    loader: &'s str,
    /// index.ts `state.importManager`.
    import_manager: ImportManager,
    /// registerBaseClassProps injected a static block.
    registered_base_class_props: bool,
}

/// index.ts `isKeyIdentifier`: the key is an Identifier node (`x`, or computed `[x]`).
fn key_identifier<'k>(key: &'k PropertyKey) -> Option<&'k str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// register-base-class-props.ts `ownApiProps`: the class's own `@api` member names, in source order.
fn own_api_props(c: &Class) -> Vec<String> {
    let mut props: Vec<String> = Vec::new();
    for member in &c.body.body {
        let (key, decorators) = match member {
            ClassElement::PropertyDefinition(p) => (&p.key, &p.decorators),
            ClassElement::MethodDefinition(m) => (&m.key, &m.decorators),
            _ => continue,
        };
        if let Some(name) = key_identifier(key) {
            if decorators.iter().any(is_api_decorator) && !props.iter().any(|p| p == name) {
                props.push(name.into());
            }
        }
    }
    props
}

impl Index<'_, '_> {
    /// `class Cmp extends …{}; export default Cmp`: a reference to the class binding sits directly
    /// under an ExportDefaultDeclaration.
    fn is_default_exported_binding(&self, c: &Class) -> bool {
        let Some(sym) = c.id.as_ref().and_then(|id| id.symbol_id.get()) else { return false };
        self.scoping.get_resolved_reference_ids(sym).iter().any(|r| {
            matches!(self.nodes.parent_kind(self.scoping.get_reference(*r).node_id()), AstKind::ExportDefaultDeclaration(_))
        })
    }

    /// Offset just past the `class` keyword (after any class decorators).
    fn class_keyword_end(&self, c: &Class) -> u32 {
        let start = c.decorators.iter().map(|d| d.span.end).max().unwrap_or(c.span.start);
        let rest = &self.src[start as usize..];
        start + (rest.len() - rest.trim_start().len()) as u32 + "class".len() as u32
    }

    /// `const X = class …` gives X, otherwise the class's own id.
    fn class_binding_name(&self, c: &Class) -> Option<String> {
        match &self.declarator_class {
            Some((span, name)) if !c.is_declaration() && *span == c.span => Some(name.clone()),
            _ => c.id.as_ref().map(|id| id.name.to_string()),
        }
    }

    /// index.ts PropertyDefinition / MethodDefinition up to the member-name switch: in a component and
    /// for an identifier key, `validateUniqueDecorator`, then for an `@api` first decorator the api
    /// checks and `state.publicProperties.set`, or for a `@wire(...)` one `catalogWireAdapters`.
    /// Whether the member was wired.
    fn catalog_member(
        &mut self,
        key: &PropertyKey,
        computed: bool,
        span: Span,
        decorators: &[Decorator],
        kind: ApiDefinition,
        validate: impl FnOnce(&Definition, &PublicProperties) -> Result<(), CompileError>,
    ) -> bool {
        if self.state.current_component.is_none() || self.error.is_some() { return false; }
        let Some(name) = key_identifier(key) else { return false };
        let def = Definition { name, span, computed };
        let checked = validate_unique_decorator(decorators, self.src, self.filename).and_then(|()| match decorators.first() {
            Some(d) if is_api_decorator(d) => validate(&def, &self.state.public_properties).map(|()| Cataloged::Public),
            Some(d) if is_wire_decorator(d) => self.catalog_wire(&def, decorators, kind).map(Cataloged::Wire),
            _ => Ok(Cataloged::Nothing),
        });
        match checked {
            Ok(Cataloged::Public) => self.state.public_properties.push((name.into(), kind, span)),
            Ok(Cataloged::Wire(adapter)) => {
                self.state.wire_adapters.push((adapter, span));
                return true;
            }
            Ok(Cataloged::Nothing) => {}
            Err(e) => self.error = Some(e),
        }
        false
    }

    /// index.ts's `isWireDecorator` branches. A wired getter or setter is replaced by a property
    /// definition and cataloged when the traversal visits that.
    fn catalog_wire(&self, def: &Definition, decorators: &[Decorator], kind: ApiDefinition) -> Result<WireAdapter, CompileError> {
        let (span, is_method) = match kind {
            ApiDefinition::Property => (Some(def.span), false),
            ApiDefinition::Method(_) if def.computed => {
                return Err(CompileError::new("@wire cannot be used on computed properties in SSR context.", self.filename))
            }
            ApiDefinition::Method(MethodDefinitionKind::Method) => (Some(def.span), true),
            ApiDefinition::Method(_) => (None, false),
        };
        catalog_wire_adapter(&WiredMember { decorators, name: def.name, span, is_method }, self.src, self.filename, self.scoping)
    }

    /// `path.replaceWith(b.propertyDefinition(key, null, computed, static))` for a wired getter or
    /// setter: its implementation is dropped and its decorators go with the rest.
    fn replace_wired_accessor(&mut self, m: &MethodDefinition) {
        let decorators_end = m.decorators.iter().map(|d| d.span.end).max().unwrap_or(m.span.start);
        let rest = &self.src[decorators_end as usize..m.span.end as usize];
        let start = decorators_end + (rest.len() - rest.trim_start().len()) as u32;
        let name = key_identifier(&m.key).expect("wired members have an identifier key");
        self.edits.push(Edit::replace(start, m.span.end, format!("{}{name};", if m.r#static { "static " } else { "" })));
    }

    /// Program.leave's insertBefore re-traverses the extracted `export default <expression>`: its first
    /// `@api` member meets its own `publicProperties` entry and throws.
    fn revisit_extracted_public_property(&mut self, expr: Span) {
        if self.error.is_some() { return; }
        let props = &self.state.public_properties;
        let Some((name, field, span)) = props.iter().find(|(_, _, s)| expr.start <= s.start && s.end <= expr.end) else { return };
        let def = Definition { name, span: *span, computed: false };
        let err = match field {
            ApiDefinition::Property => validate_property_unique(&def, props, self.src, self.filename),
            ApiDefinition::Method(kind) => validate_unique_method(&def, *kind, props, self.src, self.filename),
        };
        self.error = err.err();
    }

    /// index.ts MethodDefinition's `switch (node.key.name)`, in a component and for an identifier key
    /// (computed `[constructor]` and `static constructor` included, string keys not):
    /// `constructor` appends the props param, the three callbacks are removed. `connectedCallback`
    /// and the `had*Callback` flags only set state nothing in upstream reads.
    fn member_name_switch(&mut self, m: &MethodDefinition) {
        if self.state.current_component.is_none() { return; }
        match key_identifier(&m.key) {
            Some("constructor") => {
                let p = &m.value.params;
                let last = p.items.last().map(|i| i.span.end).into_iter().chain(p.rest.as_ref().map(|r| r.span.end)).max();
                self.state.props_arg_sites.push(last.map_or((p.span.end - 1, false), |e| (e, true)));
            }
            // `path.remove()`: the edit covers the member's own decorator edits; its children are still visited.
            Some("renderedCallback" | "disconnectedCallback" | "errorCallback") => self.edits.push(Edit::replace(m.span.start, m.span.end, "")),
            _ => {}
        }
    }

    /// index.ts ImportExpression (`transformSync` always passes `dynamicImports`, so the visitor's
    /// `!dynamicImports` return is unreachable): with `strictSpecifier`, a source that is not a string
    /// literal throws LWC1121 at the node; with a loader, `importManager.add({ load: '__lwcLoad' }, loader)`
    /// and the node becomes `__lwcLoad(source)` (the options argument is dropped, its children still visited).
    fn import_expression(&mut self, it: &ImportExpression) {
        if self.strict_specifier {
            let arg = match it.source.without_parentheses() {
                Expression::StringLiteral(_) => None,
                Expression::NumericLiteral(n) => Some(js_number(n.value)),
                Expression::BooleanLiteral(b) => Some(b.value.to_string()),
                Expression::NullLiteral(_) => Some("null".into()),
                Expression::BigIntLiteral(b) => Some(b.value.to_string()),
                Expression::RegExpLiteral(r) => Some(format!("/{}/{}", r.regex.pattern.text, r.regex.flags)),
                _ => Some("undefined".into()),
            };
            if let Some(arg) = arg {
                let info = &LWCClassErrors::INVALID_DYNAMIC_IMPORT_SOURCE_STRICT;
                self.error = Some(errors::generate_error(self.src, self.filename, it.span, info, &[&arg]));
                return;
            }
        }
        if self.loader.is_empty() { return; }
        self.import_manager.add("load", LOAD, self.loader);
        let source = it.source.span();
        self.edits.push(Edit::replace(it.span.start, source.start, format!("{LOAD}(")));
        self.edits.push(Edit::replace(source.end, it.span.end, ")"));
    }

    /// index.ts Identifier visitor: any Identifier node (keys, labels, specifiers and member
    /// properties included, private names not) named `__lwc…` throws at the node. The trusted names
    /// upstream allows are only ever its own output, which this walk over the input never sees.
    fn check_reserved_identifier(&mut self, name: &str, span: Span) {
        if self.error.is_none() && name.starts_with(RESERVED_IDENTIFIER_PREFIX) {
            self.error = Some(errors::generate_error(self.src, self.filename, span, &SsrCompilerErrors::RESERVED_IDENTIFIER_PREFIX, &[]));
        }
    }

    /// index.ts Program.leave: `export default <expr>` becomes
    /// `const __lwcDefaultExport = <expr>;\nexport default __lwcDefaultExport;` in LWC files only.
    fn leave_program(&mut self) {
        if self.state.component.is_none() { return; }
        let extracted = self.state.export_default_expression;
        if let Some((decl, expr)) = extracted {
            self.edits.push(Edit::replace(decl.start, expr.start, format!("const {DEFAULT_EXPORT_BINDING} = ")));
            self.edits.push(Edit::replace(expr.end, decl.end, format!(";\nexport default {DEFAULT_EXPORT_BINDING};")));
            self.state.lwc_default_export_name = Some(DEFAULT_EXPORT_BINDING.into());
            self.revisit_extracted_public_property(expr);
            // The re-traversal catalogs the expression's `@wire` members a second time.
            let again: Vec<_> = self.state.wire_adapters.iter().filter(|(_, s)| expr.start <= s.start && s.end <= expr.end).cloned().collect();
            self.state.wire_adapters.extend(again);
        }
        // index.ts:423 registerBaseClassProps, after the traversal: `body.push(static { ... })` on every
        // other module class with own `@api` members (no name needed, so anonymous and nested classes
        // too). A body item, so it prints before the version comment below. moduleClassNodes is a Set:
        // the re-traversal of an extracted expression does not add a class twice.
        for m in &self.state.module_classes {
            if Some(m.span) == self.state.lwc_component_node || m.api_props.is_empty() { continue; }
            let names: Vec<String> = m.api_props.iter().map(|p| json_string(p.encode_utf16())).collect();
            let block = format!("static {{ {REGISTER_PUBLIC_PROPERTIES}(this, [{}]); }}", names.join(", "));
            self.edits.push(self.body_end_edit(m.body, &[block]));
            self.registered_base_class_props = true;
        }
        // index.ts:136-148 / :185-197: a Block comment appended to the class body's trailingComments,
        // printed as the body's last item before `}`. Program.leave's insertBefore re-traverses the
        // extracted expression, so a class matched inside it is entered (and commented) twice.
        let inside = |s: u32| extracted.is_some_and(|(_, e)| e.start <= s && s <= e.end);
        for body in std::mem::take(&mut self.state.version_comment_bodies) {
            self.edits.push(self.version_comment_edit(body, if inside(body.start) { 2 } else { 1 }));
        }
        // The re-traversal pushes the props param / argument a second time too.
        for (at, after_item) in std::mem::take(&mut self.state.props_arg_sites) {
            let n = if inside(at) { 2 } else { 1 };
            let list = vec![PROPS_ARG; n].join(", ");
            self.edits.push(Edit::insert(at, if after_item { format!(", {list}") } else { list }));
        }
    }

    /// `n` copies of the version comment, as the class body's trailing comments.
    fn version_comment_edit(&self, body: Span, n: usize) -> Edit {
        self.body_end_edit(body, &vec![version_comment(); n])
    }

    /// `items` before the class body's `}`, laid out like astring prints a ClassBody's last items and
    /// trailing comments: each on its own line, two spaces in from the `}` line's indent.
    fn body_end_edit(&self, body: Span, items: &[String]) -> Edit {
        let close = body.end - 1;
        let line_start = self.src[..close as usize].rfind('\n').map_or(0, |i| i + 1);
        let line = &self.src[line_start..close as usize];
        let indent = &line[..line.len() - line.trim_start().len()];
        let mut text = String::new();
        for (i, item) in items.iter().enumerate() {
            if i > 0 { text.push_str(indent); }
            text.push_str("  ");
            text.push_str(item);
            text.push('\n');
        }
        text.push_str(indent);
        Edit::insert(close, text)
    }
}

impl<'a> Visit<'a> for Index<'_, 'a> {
    fn visit_import_declaration(&mut self, d: &ImportDeclaration<'a>) {
        let from_lwc = lwc_import::replace_lwc_import(d, self.src, &mut self.state, &mut self.edits);
        stylesheets::catalog_and_replace_style_imports(d, self.src, &mut self.edits);
        remove_decorator_import::remove_decorator_import(d, from_lwc, self.src, &mut self.edits);
        walk::walk_import_declaration(self, d);
    }

    fn visit_export_from_declaration(&mut self, d: &ExportFromDeclaration<'a>) {
        lwc_import::replace_lwc_export(&d.source, self.src, &mut self.edits);
        walk::walk_export_from_declaration(self, d);
    }

    fn visit_export_all_declaration(&mut self, d: &ExportAllDeclaration<'a>) {
        lwc_import::replace_lwc_export(&d.source, self.src, &mut self.edits);
        walk::walk_export_all_declaration(self, d);
    }

    fn visit_export_default_declaration(&mut self, d: &ExportDefaultDeclaration<'a>) {
        match &d.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                let name = c.id.as_ref().map_or(DEFAULT_COMPONENT_NAME, |id| id.name.as_str());
                self.state.lwc_default_export_name = Some(name.into());
                self.export_default_class = Some(c.span);
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(_) | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {}
            kind => {
                let expr = kind.as_expression().expect("remaining variants are expressions");
                match expr.without_parentheses() {
                    Expression::ClassExpression(c) => {
                        let name = c.id.as_ref().map(|id| id.name.to_string()).or_else(|| self.state.component.as_ref().and_then(|p| p.name.clone()));
                        self.state.lwc_default_export_name = Some(name.unwrap_or_else(|| DEFAULT_COMPONENT_NAME.into()));
                    }
                    Expression::Identifier(i) => self.state.lwc_default_export_name = Some(i.name.to_string()),
                    Expression::FunctionExpression(_) => {}
                    _ => self.state.export_default_expression = Some((d.span, expr.span())),
                }
            }
        }
        walk::walk_export_default_declaration(self, d);
    }

    fn visit_variable_declarator(&mut self, d: &VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(Expression::ClassExpression(c))) =
            (&d.id, d.init.as_ref().map(|e| e.without_parentheses()))
        {
            self.declarator_class = Some((c.span, id.name.to_string()));
        }
        walk::walk_variable_declarator(self, d);
    }

    fn visit_class(&mut self, c: &Class<'a>) {
        let mut matched = false;
        if c.is_declaration() {
            if c.heritage.is_some() && (self.export_default_class == Some(c.span) || self.is_default_exported_binding(c)) {
                let name = match &c.id {
                    Some(id) => id.name.to_string(),
                    None => {
                        // A class declaration can omit a name only when it is default-exported.
                        self.edits.push(Edit::insert(self.class_keyword_end(c), format!(" {DEFAULT_COMPONENT_NAME}")));
                        DEFAULT_COMPONENT_NAME.into()
                    }
                };
                self.state.component = Some(ComponentClass { name: Some(name) });
                self.state.version_comment_bodies.push(c.body.span);
                matched = true;
            }
        } else if let Some(Expression::Identifier(sup)) = c.heritage.as_ref().map(|h| &h.expression) {
            if self.state.lightning_element_identifier.as_deref() == Some(sup.name.as_str()) {
                let name = self.class_binding_name(c);
                // Upstream leaves an earlier `lwcClassName` in place when this class has no name.
                let prev = self.state.component.take().and_then(|p| p.name);
                self.state.component = Some(ComponentClass { name: name.or(prev) });
                self.state.version_comment_bodies.push(c.body.span);
                matched = true;
            }
        }
        if matched {
            self.state.current_component = Some(c.span);
            self.state.lwc_component_node = Some(c.span);
        }
        self.state.module_classes.push(ModuleClass { span: c.span, body: c.body.span, api_props: own_api_props(c) });
        walk::walk_class(self, c);
        if self.state.current_component == Some(c.span) {
            self.state.current_component = None;
        }
    }

    fn visit_property_definition(&mut self, p: &PropertyDefinition<'a>) {
        let (src, filename) = (self.src, self.filename);
        self.catalog_member(&p.key, p.computed, p.span, &p.decorators, ApiDefinition::Property, |def, props| {
            validate_api_property(p, def, props, src, filename)
        });
        walk::walk_property_definition(self, p);
    }

    fn visit_method_definition(&mut self, m: &MethodDefinition<'a>) {
        let (src, filename) = (self.src, self.filename);
        let wired = self.catalog_member(&m.key, m.computed, m.span, &m.decorators, ApiDefinition::Method(m.kind), |def, props| {
            validate_api_method(m, def, props, src, filename)
        });
        if wired && m.kind != MethodDefinitionKind::Method {
            // The replacement has no body to traverse; only its decorators are visited.
            self.replace_wired_accessor(m);
            for d in &m.decorators { self.visit_decorator(d); }
            return;
        }
        if self.error.is_none() { self.member_name_switch(m); }
        walk::walk_method_definition(self, m);
    }

    /// astring prints no decorators. estree-toolkit has no visitor keys for them, so upstream never
    /// traverses a decorator's expression (no Identifier check, no nested class) and neither does this.
    fn visit_decorator(&mut self, d: &Decorator<'a>) {
        let rest = &self.src[d.span.end as usize..];
        let ws = (rest.len() - rest.trim_start().len()) as u32;
        self.edits.push(Edit::replace(d.span.start, d.span.end + ws, ""));
    }

    fn visit_function(&mut self, f: &Function<'a>, flags: ScopeFlags) {
        self.functions.push(flags.is_constructor());
        walk::walk_function(self, f, flags);
        self.functions.pop();
    }

    fn visit_arrow_function_expression(&mut self, f: &ArrowFunctionExpression<'a>) {
        self.functions.push(false);
        walk::walk_arrow_function_expression(self, f);
        self.functions.pop();
    }

    /// index.ts Super: in a component, a `super(...)` call whose function parent is a constructor
    /// gets the props argument (an arrow in between stops it; nested blocks do not).
    fn visit_call_expression(&mut self, c: &CallExpression<'a>) {
        if matches!(c.callee, Expression::Super(_)) && self.state.current_component.is_some() && self.functions.last() == Some(&true) && self.error.is_none() {
            let site = c.arguments.last().map_or((c.span.end - 1, false), |a| (a.span().end, true));
            self.state.props_arg_sites.push(site);
        }
        walk::walk_call_expression(self, c);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if self.error.is_none() { self.import_expression(it); }
        walk::walk_import_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, id: &IdentifierReference<'a>) {
        self.check_reserved_identifier(&id.name, id.span);
    }

    fn visit_binding_identifier(&mut self, id: &BindingIdentifier<'a>) {
        self.check_reserved_identifier(&id.name, id.span);
    }

    fn visit_identifier_name(&mut self, id: &IdentifierName<'a>) {
        self.check_reserved_identifier(&id.name, id.span);
    }

    fn visit_label_identifier(&mut self, id: &LabelIdentifier<'a>) {
        self.check_reserved_identifier(&id.name, id.span);
    }
}

/// index.ts `compileJS` up to `traverse`: parse once, build semantic once, walk once.
fn index(src: &str, filename: &str, options: &CompileOptions) -> Result<(IndexResult, Vec<Edit>, ImportManager), CompileError> {
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, src, SourceType::mjs()).parse();
    // meriyah throws on syntax errors only; oxc's post-parse module-record early errors (e.g. a
    // duplicate `export default`, which upstream compiles) leave a complete program and are not fatal.
    if ret.fatal_error {
        let message = ret.diagnostics.errors().next().map_or_else(String::new, |d| d.message.to_string());
        return Err(CompileError::new(message, filename));
    }
    let sem = SemanticBuilder::new().with_build_nodes(true).build(&ret.program).semantic;
    let mut v = Index {
        src,
        filename,
        scoping: sem.scoping(),
        nodes: sem.nodes(),
        state: ComponentMetaState::default(),
        edits: Vec::new(),
        export_default_class: None,
        declarator_class: None,
        error: None,
        functions: Vec::new(),
        strict_specifier: options.dynamic_import_strict_specifier,
        loader: &options.dynamic_import_loader,
        import_manager: ImportManager::default(),
        registered_base_class_props: false,
    };
    v.visit_program(&ret.program);
    if let Some(e) = v.error { return Err(e); }
    v.leave_program();
    if let Some(e) = v.error { return Err(e); }
    let ComponentMetaState { component, lwc_default_export_name, public_properties, wire_adapters, .. } = v.state;
    let (edits, imports, registered_base_class_props) = (v.edits, v.import_manager, v.registered_base_class_props);
    let Some(ComponentClass { name: class_name }) = component else { return Ok((IndexResult::NotLwc, edits, imports)) };
    let Some(exported_name) = lwc_default_export_name.or(class_name) else {
        // generate-markup.ts:54 `b.identifier(null!)`: estree-toolkit's builder assertion throws
        // (a plain Error, no LWC code; @lwc/compiler's SSR path does not wrap it).
        return Err(CompileError::new("Expected the value to be a `string` but got a `null`. The value is null.", filename));
    };
    let public_properties = public_properties.into_iter().map(|(name, ..)| name).collect();
    let wire_adapters = wire_adapters.into_iter().map(|(adapter, _)| adapter).collect();
    Ok((IndexResult::Component { exported_name, public_properties, registered_base_class_props, wire_adapters }, edits, imports))
}

/// ECMAScript `Number::toString(10)` for a finite, non-negative literal value (`String(value)`).
fn js_number(n: f64) -> String {
    if n == 0.0 { return "0".into(); }
    // Rust's shortest round-trip digits: `d[.ddd]e<x>`, so the value is 0.digits * 10^(x + 1).
    let sci = format!("{n:e}");
    let (mantissa, exp) = sci.split_once('e').expect("LowerExp prints an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let (k, e) = (digits.len() as i32, exp.parse::<i32>().expect("integer exponent") + 1);
    if k <= e && e <= 21 {
        digits + &"0".repeat((e - k) as usize)
    } else if 0 < e && e <= 21 {
        format!("{}.{}", &digits[..e as usize], &digits[e as usize..])
    } else if -6 < e && e <= 0 {
        format!("0.{}{digits}", "0".repeat(-e as usize))
    } else {
        let m = if k == 1 { digits } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{m}e{}{}", if e - 1 < 0 { '-' } else { '+' }, (e - 1).abs())
    }
}

/// generate-markup.ts `addGenerateMarkupFunction`: unshift the `__lwcTmpl` default import and then the
/// `setStaticInternals` import, and push the `__setStaticInternals(...)` call.
fn add_generate_markup_function(
    code: &str,
    exported_name: &str,
    public_properties: &[String],
    wire_adapters: &[WireAdapter],
    filename: &str,
    options: &CompileOptions,
) -> String {
    let lit = |s: &str| json_string(s.encode_utf16());
    let stem = module_stem(filename, &options.name);
    let tag = generate_custom_element_tag_name(&options.namespace, &options.name);
    let props: Vec<String> = public_properties.iter().map(|p| lit(p)).collect();
    let mut out = import_declaration(&[Spec::Named(crate::runtime_abi::SET_STATIC_INTERNALS, "__setStaticInternals")], crate::runtime_abi::RUNTIME_MODULE);
    out += &import_declaration(&[Spec::Default("__lwcTmpl")], &format!("./{stem}.html"));
    out += code;
    out += &format!(
        "\n__setStaticInternals({exported_name}, {}, [{}], {} ?? null, {}, __lwcTmpl);\n",
        lit(&tag),
        props.join(", "),
        wire_adapters_plumbing(wire_adapters),
        lit(options.ssr_mode.js_value()),
    );
    out
}

/// Sync-only compileComponentForSSR: index.ts `compileJS` after the traverse.
pub fn compile_component_for_ssr(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    let (index_result, edits, imports) = index(src, filename, options)?;
    let body = apply(src, edits).map_err(|e| CompileError::new(e.to_string(), filename))?;
    // Program.leave: `program.body.unshift(...importManager.getImportDeclarations())`, in every module.
    let mut code = imports.declarations() + &body;
    let result = |code| CompileResult { code, map: None, warnings: vec![], css_scope_tokens: None };
    let IndexResult::Component { exported_name, public_properties, registered_base_class_props, wire_adapters } = index_result else {
        // Not an `extends` default export or an `extends LightningElement` class expression.
        return Ok(result(code));
    };
    if registered_base_class_props {
        let spec = Spec::Named(crate::runtime_abi::REGISTER_PUBLIC_PROPERTIES, REGISTER_PUBLIC_PROPERTIES);
        code = import_declaration(&[spec], crate::runtime_abi::RUNTIME_MODULE) + &code;
    }
    Ok(result(add_generate_markup_function(&code, &exported_name, &public_properties, &wire_adapters, filename, options)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_static_js() {
        let src = "import { LightningElement } from 'lwc';\n\nexport default class TextStatic extends LightningElement {}\n";
        let opts = CompileOptions {
            name: "text-static".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/text-static/text-static.js", &opts).unwrap();
        assert!(
            r.code.contains(
                "__setStaticInternals(TextStatic, \"x-text-static\", [], null ?? null, \"sync\", __lwcTmpl)"
            ),
            "{}",
            r.code
        );
    }

    #[test]
    fn anonymous_default_class() {
        let src = "import { LightningElement } from 'lwc';\n\nexport default class extends LightningElement {\n    foo = '';\n}\n";
        let opts = CompileOptions {
            name: "comments-text".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/comments-text/comments-text.js", &opts).unwrap();
        assert!(r.code.contains("export default class DefaultComponentName extends"), "{}", r.code);
        assert!(
            r.code.contains("__setStaticInternals(DefaultComponentName, \"x-comments-text\""),
            "{}",
            r.code
        );
        assert!(!r.code.contains("from 'lwc'"), "{}", r.code);
    }

    #[test]
    fn api_public_props() {
        let src = "import { LightningElement, api } from 'lwc';\nexport default class Child extends LightningElement {\n    @api\n    fieldApi;\n    @api\n    get setterGetterApi() { return 1; }\n}\n";
        let opts = CompileOptions {
            name: "child".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/child/child.js", &opts).unwrap();
        assert!(
            r.code.contains(
                "__setStaticInternals(Child, \"x-child\", [\"fieldApi\", \"setterGetterApi\"], null ?? null, \"sync\", __lwcTmpl)"
            ),
            "{}",
            r.code
        );
    }

    #[test]
    fn same_line_api_and_base_register() {
        let src = "import { LightningElement, api } from 'lwc';\nclass Base extends LightningElement {\n    @api value;\n    @api level;\n}\nexport default class ApiPropInFileSuperclass extends Base {}\n";
        let opts = CompileOptions {
            name: "api-prop-in-file-superclass".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/api-prop-in-file-superclass/api-prop-in-file-superclass.js", &opts).unwrap();
        assert!(!r.code.contains("@api"), "{}", r.code);
        assert!(r.code.contains("value;"), "{}", r.code);
        assert!(r.code.contains("__registerPublicProperties(this, [\"value\", \"level\"])"), "{}", r.code);
        assert!(r.code.contains("registerPublicProperties as __registerPublicProperties"), "{}", r.code);
        assert!(r.code.contains("__setStaticInternals(ApiPropInFileSuperclass,"), "{}", r.code);
    }

    #[test]
    fn plain_adapter_passthrough() {
        let src = "export default class adapter {\n    constructor(dc) { this.dc = dc; }\n}\n";
        let opts = CompileOptions {
            name: "adapter".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/adapter/adapter.js", &opts).unwrap();
        assert_eq!(r.code, src);
    }

    #[test]
    fn render_mode_preserved() {
        let src = "import { LightningElement } from 'lwc';\nexport default class Basic extends LightningElement {\n    static renderMode = 'light';\n}\n";
        let opts = CompileOptions {
            name: "basic".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/basic/basic.js", &opts).unwrap();
        assert!(r.code.contains("static renderMode = 'light'"), "{}", r.code);
    }

    #[test]
    fn wire_adapters_emitted() {
        let src = "import { LightningElement, wire } from 'lwc';\nimport adapter from 'x/adapter';\nexport default class Wire extends LightningElement {\n    @wire(adapter, { foo: '$bar' })\n    wiredProp;\n}\n";
        let opts = CompileOptions {
            name: "wire".into(),
            namespace: "x".into(),
            ..Default::default()
        };
        let r = compile_component_for_ssr(src, "x/wire/wire.js", &opts).unwrap();
        assert!(r.code.contains("adapter: adapter"), "{}", r.code);
        assert!(r.code.contains("instance[\"bar\"]"), "{}", r.code);
    }

    /// decorators/wire.ts errors as @lwc/ssr-compiler 9.5.3 throws them, located at the member.
    #[test]
    fn wire_decorator_errors() {
        let cls = |members: &str, tail: &str| format!("export default class C extends LightningElement {{\n{members}\n}}\n{tail}");
        let at_member = Some((4, 2));
        let cases = [
            (cls("  @wire(a) @wire(a) p;", ""), "LWC1105: Method or property can only have 1 @wire decorator", at_member),
            (cls("  @wire() p;", ""), "LWC1092: @wire expects an adapter as first parameter. @wire(adapter: WireAdapter, config?: any).", at_member),
            (cls("  @wire('x') p;", ""), "LWC1097: @wire expects a function identifier as first parameter.", at_member),
            (cls("  @wire(a['b']) p;", ""), "LWC1131: @wire identifier cannot contain computed properties", at_member),
            (cls("  @wire(a.b.c) p;", ""), "LWC1132: @wire identifier cannot contain nested member expressions", at_member),
            (cls("  @wire(local) p;", "const local = 1;\n"), "LWC1200: Computed property in @wire config must be a constant or primitive literal.", at_member),
            (cls("  @wire(nope) p;", ""), "LWC1200: Computed property in @wire config must be a constant or primitive literal.", at_member),
            (cls("  @wire(a, cfg) p;", "const cfg = {};\n"), "LWC1094: @wire expects a configuration object expression as second parameter.", at_member),
            (cls("  @wire(a, { [`k`]: 1 }) p;", ""), "LWC1199: Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.", at_member),
            (cls("  @wire(a, { [K]: 1 }) p;", "let K = 'k';\n"), "LWC1200: Computed property in @wire config must be a constant or primitive literal.", at_member),
            (cls("  @wire(a, { [/r/]: 1 }) p;", ""), "LWC1200: Computed property in @wire config must be a constant or primitive literal.", at_member),
            // A wired getter is replaced by a synthetic property with no location.
            (cls("  @wire(nope) get g() { return 1; }", ""), "LWC1200: Computed property in @wire config must be a constant or primitive literal.", None),
            (cls("  @wire(a) [m]() {}", "const m = 'm';\n"), "@wire cannot be used on computed properties in SSR context.", None),
        ];
        for (src, message, at) in cases {
            let err = compile_decorated(&src).unwrap_err();
            assert_eq!(err.message, message, "{src}");
            assert_eq!(err.line.zip(err.column), at, "{src}");
        }
    }

    fn wire_plumbing(body: &str) -> String {
        let code = compile_decorated(body).unwrap().code;
        let start = code.find("__setStaticInternals(").unwrap();
        code[start..].split_whitespace().collect()
    }

    #[test]
    fn wire_adapters_cataloged_on_the_ast() {
        let cls = |members: &str| format!("export default class C extends LightningElement {{\n{members}\n}}\nconst K = 'k';\n");
        let entry = |adapter: &str, callback: &str, config: &str| {
            format!("{{adapter:{adapter},dataCallback:(instance)=>(newValue)=>{{{callback}}},config:(instance)=>({config})}}")
        };
        // Decorators split over lines, comments inside the call, `$` paths, constant computed keys.
        let out = wire_plumbing(&cls("  @wire(\n    a, // id\n    { foo: '$bar.baz', [K]: 1, n: { x: '$y' } }\n  )\n  p;\n  @wire(a.b) m(v) {}"));
        let expected = [
            entry("a", "instance.p=newValue;", "{foo:instance[\"bar\"][\"baz\"],[K]:1,n:{x:'$y'}}"),
            entry("a.b", "instance.m(newValue);", "{}"),
        ];
        assert!(out.contains(&format!("[{}]??null,", expected.join(","))), "{out}");
        // Wired accessors become fields; their bodies are dropped.
        let code = compile_decorated(&cls("  @wire(a) static get g() { return 1; }\n  @wire(a) set s(v) {}")).unwrap().code;
        assert!(code.contains("  static g;\n  s;\n"), "{code}");
        assert!(!code.contains("@wire") && !code.contains("return 1"), "{code}");
        // Members of a nested class count toward the enclosing component.
        let out = wire_plumbing(&cls("  @wire(a) p;\n  x = class { @wire(a) q; };"));
        assert!(out.contains(&entry("a", "instance.q=newValue;", "{}")), "{out}");
        // A bare `@wire` is not a wire decorator.
        assert!(wire_plumbing(&cls("  @wire p;")).contains("[],null??null,"));
    }

    #[test]
    fn export_as_default() {
        let src = "import { LightningElement } from 'lwc';
class Light extends LightningElement {
    static renderMode = 'light';
}
export { Light as default };
";
        let opts = CompileOptions { name: "light".into(), namespace: "x".into(), ..Default::default() };
        let r = compile_component_for_ssr(src, "x/light/light.js", &opts).unwrap();
        assert!(r.code.contains("@lwc/ssr-runtime"), "{}", r.code);
        assert!(!r.code.contains("setStaticInternals"), "{}", r.code);
        assert!(!r.code.contains("./light.html"), "{}", r.code);
    }

    #[test]
    fn extends_imported_base_without_lwc_import() {
        let src = "import FancyElement from 'x/fancyElement';\n\nexport default class extends FancyElement {}\n";
        let opts = CompileOptions { name: "component".into(), namespace: "x".into(), ..Default::default() };
        let r = compile_component_for_ssr(src, "x/component/component.js", &opts).unwrap();
        assert!(r.code.contains("__setStaticInternals(DefaultComponentName, \"x-component\""), "{}", r.code);
        assert!(r.code.contains("@lwc/ssr-runtime"), "{}", r.code);
    }

    #[test]
    fn wire_api_conflict_1095() {
        let src = "import { LightningElement, api, wire } from 'lwc';\nimport a from 'x/a';\nexport default class C extends LightningElement {\n    @api\n    @wire(a)\n    foo;\n}\n";
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let err = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap_err();
        assert!(err.message.contains("LWC1095"), "{}", err.message);
    }

    const DECORATED: &str = "import { LightningElement, api, track, wire } from 'lwc';\nimport a from 'x/a';\n";

    fn compile_decorated(body: &str) -> Result<CompileResult, CompileError> {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        compile_component_for_ssr(&format!("{DECORATED}{body}"), "x/c/c.js", &opts)
    }

    /// decorators/index.ts + api/validate.ts messages, as @lwc/ssr-compiler 9.5.3 throws them.
    #[test]
    fn api_track_decorator_errors() {
        let cls = |members: &str| format!("export default class C extends LightningElement {{\n{members}\n}}\n");
        let cases = [
            (cls("  @api get x() { return 1; }\n  @api set x(v) {}"), "LWC1112: @api get x and @api set x detected in class declaration. Only one of the two needs to be decorated with @api."),
            (cls("  @api get x() { return 1; }\n  @api get x() { return 2; }"), "LWC1112: @api get x and @api set x detected in class declaration. Only one of the two needs to be decorated with @api."),
            (cls("  @api x;\n  @api x;"), "LWC1096: Duplicate @api property \"x\"."),
            (cls("  @api x;\n  @api x() {}"), "LWC1096: Duplicate @api property \"x\"."),
            (cls("  @api x = true;"), "LWC1099: Boolean public property must default to false."),
            (cls("  @api x = (true);"), "LWC1099: Boolean public property must default to false."),
            (cls("  @api [x];"), "LWC1106: @api cannot be applied to a computed property, getter, setter or method."),
            (cls("  @api part;"), "LWC1111: Invalid property name \"part\". \"part\" is a future reserved attribute for web components."),
            (cls("  @api onclick;"), "LWC1108: Invalid property name \"onclick\". Properties starting with \"on\" are reserved for event handlers."),
            (cls("  @api dataFoo;"), "LWC1107: Invalid property name \"dataFoo\". Properties starting with \"data\" are reserved attributes."),
            (cls("  @api slot;"), "LWC1110: Invalid property name \"slot\". \"slot\" is a reserved attribute."),
            (cls("  @api get class() { return 1; }"), "LWC1110: Invalid property name \"class\". \"class\" is a reserved attribute."),
            (cls("  @api tabindex;"), "LWC1109: Ambiguous attribute name \"tabindex\". \"tabindex\" will never be called from template because its corresponding property is camel cased. Consider renaming to \"tabIndex\"."),
            (cls("  @api maxlength() {}"), "LWC1109: Ambiguous attribute name \"maxlength\". \"maxlength\" will never be called from template because its corresponding property is camel cased. Consider renaming to \"maxLength\"."),
            (cls("  @api is;\n  @api is;"), "LWC1110: Invalid property name \"is\". \"is\" is a reserved attribute."),
            (cls("  @api @track x;"), "LWC1093: @api method or property cannot be used with @track"),
            (cls("  @track\n  @api\n  x;"), "LWC1093: @api method or property cannot be used with @track"),
            (cls("  @wire(a)\n  @api\n  foo() {}"), "LWC1095: @wire method or property cannot be used with @api"),
            (cls("  @wire(a) @track foo;"), "LWC1095: @wire method or property cannot be used with @track"),
            // Program.leave re-traverses the extracted default-export expression and meets `x` again.
            ("export default wrap(class extends LightningElement {\n  @api x;\n});\n".into(), "LWC1096: Duplicate @api property \"x\"."),
            // The reserved-identifier error is thrown first.
            (format!("const __lwcX = 1;\n{}", cls("  @api x = true;")), "LWC1202: Identifier name cannot start with \"__lwc\"."),
        ];
        for (src, message) in cases {
            let err = compile_decorated(&src).unwrap_err();
            assert_eq!(err.message, message, "{src}");
        }
    }

    /// `publicProperties` in visit order, as the oracle's `__setStaticInternals` third argument.
    #[test]
    fn public_properties_from_the_walk() {
        let props = |body: &str| {
            let code = compile_decorated(body).unwrap().code;
            assert!(!code.contains("@api") && !code.contains("@track") && !code.contains("@foo"), "{code}");
            let tail = code.rsplit_once("__setStaticInternals(").unwrap().1;
            tail.split_once('[').unwrap().1.split_once(']').unwrap().0.to_string()
        };
        assert_eq!(props("export default class C extends LightningElement {\n  @api\n  foo;\n  @api bar; @track baz;\n}\n"), r#""foo", "bar""#);
        // A nested class keeps `currentComponent`; a nested component clears it on leave.
        assert_eq!(props("export default class C extends LightningElement {\n  @api outer;\n  m() { return class { @api inner; }; }\n  @api after;\n}\n"), r#""outer", "inner", "after""#);
        assert_eq!(props("export default class C extends LightningElement {\n  @api outer;\n  m() { return class extends LightningElement { @api inner; }; }\n  @api after;\n}\n"), r#""outer", "inner""#);
        assert_eq!(props("export default class C extends LightningElement {\n  @api ['x'];\n  @api #y;\n  @api y;\n  @api data;\n  @api static s;\n}\n"), r#""y", "data", "s""#);
        assert_eq!(props("function foo() {}\nexport default class C extends LightningElement {\n  @foo x;\n}\n"), "");
    }

    /// registerBaseClassProps: a static block at the end of every other class with own `@api` members,
    /// named or not, nested or not, before a matched class's version comment (@lwc/ssr-compiler 9.5.3).
    #[test]
    fn base_class_props_on_the_ast() {
        let src = format!("{DECORATED}class Base extends LightningElement {{\n  @api value;\n  @api level;\n}}\nexport default class C extends Base {{\n  @api own;\n}}\n");
        let IndexResult::Component { public_properties, registered_base_class_props, .. } = index(&src, "x/c/c.js", &CompileOptions::default()).unwrap().0 else { panic!() };
        assert_eq!((public_properties, registered_base_class_props), (vec!["own".to_string()], true));
        let block = |names: &str| format!("static {{ __registerPublicProperties(this, [{names}]); }}");
        let code = compile_decorated("const Base = class extends Object {\n  @api value;\n};\nexport default class C extends Base {}\n").unwrap().code;
        assert!(code.contains(&format!("  value;\n  {}\n}};", block("\"value\""))), "{code}");
        assert!(code.starts_with("import { setStaticInternals as __setStaticInternals } from \"@lwc/ssr-runtime\";\nimport __lwcTmpl from \"./c.html\";\nimport { registerPublicProperties as __registerPublicProperties } from \"@lwc/ssr-runtime\";\n"), "{code}");
        // Anonymous (`export default class extends` base of a named export is not the component) and nested.
        let code = compile_decorated("export default class B extends LightningElement {\n  @api a;\n  m() { return class { @api n; }; }\n}\nfoo(class { @api q; });\n").unwrap().code;
        assert!(code.contains(&format!("return class {{ n;   {}\n  }}; }}", block("\"n\""))), "{code}");
        assert!(code.contains(&format!("foo(class {{ q;   {}\n}});", block("\"q\""))), "{code}");
        assert!(!code.contains(&block("\"a\"")), "the component registers through setStaticInternals: {code}");
        // A matched base class: the block is a body item, the version comment trails it.
        let code = compile_decorated("const M = class extends LightningElement {\n  @api x;\n};\nexport default class C extends M {}\n").unwrap().code;
        assert!(code.contains(&format!("  x;\n  {}\n  {}\n}};", block("\"x\""), version_comment())), "{code}");
        // No `@api` anywhere else: no block, no import.
        assert!(!compile_decorated("export default class C extends LightningElement {\n  @api x;\n}\n").unwrap().code.contains("registerPublicProperties"));
    }

    /// index.ts ImportExpression, as @lwc/ssr-compiler 9.5.3 compiles it: strict specifiers throw LWC1121
    /// at the node in pre-order with LWC1202; a loader rewrites every `import()` in any module.
    #[test]
    fn dynamic_imports_on_the_ast() {
        let opts = |strict: bool, loader: &str| CompileOptions { name: "c".into(), namespace: "x".into(), dynamic_import_strict_specifier: strict, dynamic_import_loader: loader.into(), ..Default::default() };
        let err = |src: &str| compile_component_for_ssr(src, "x/c/c.js", &opts(true, "")).unwrap_err();
        let e = err("\nimport { LightningElement } from 'lwc';\nexport default class extends LightningElement {}\nexport default async function rando () {\n  await import(woohoo);\n}\n");
        let d = &e.diagnostics[0];
        let l = d.location.unwrap();
        assert_eq!((d.code, d.level, l.line, l.column, l.start, l.length), (Some(1121), lwc_shared::DiagnosticLevel::Error, 5, 8, Some(139), Some(14)));
        let thrown = lwc_shared::errors::CompilerError::from_diagnostic(d, None);
        let info = &LWCClassErrors::INVALID_DYNAMIC_IMPORT_SOURCE_STRICT;
        assert_eq!((thrown.level, Some(thrown.url.as_str())), (lwc_shared::DiagnosticLevel::Error, info.url));
        assert!(info.url.is_some_and(|u| u.ends_with("#lwc1121")));
        assert_eq!(e.message, "LWC1121: Invalid import. The argument \"undefined\" must be a stringLiteral for dynamic imports when strict mode is enabled.");
        let arg = |source: &str| err(&format!("export function f() {{ return import({source}); }}")).message.split('"').nth(1).unwrap().to_string();
        for (source, shown) in [("1e21", "1e+21"), ("1.5e-7", "1.5e-7"), ("100", "100"), ("0.5", "0.5"), ("10n", "10"), ("/a/g", "/a/g"), ("null", "null"), ("true", "true"), ("`x`", "undefined")] {
            assert_eq!(arg(source), shown, "{source}");
        }
        // Pre-order: an earlier reserved identifier wins, a later one loses.
        assert!(err("const __lwcX = 1; export function f() { return import(y); }").message.starts_with("LWC1202"));
        assert!(err("export function f() { return import(y); } const __lwcX = 1;").message.starts_with("LWC1121"));
        assert!(compile_component_for_ssr("export function f() { return import(('x')); }", "x/c/c.js", &opts(true, "")).is_ok());
        // The loader: one import per source, every call rewritten, options dropped, non-LWC modules too.
        let code = compile_component_for_ssr("export function f() { import('a'); return import(import('b'), { with: {} }); }\nconst s = 'import(x)';\n", "x/c/c.js", &opts(false, "my\"L")).unwrap().code;
        assert_eq!(code, "import { load as __lwcLoad } from \"my\\\"L\";\nexport function f() { __lwcLoad('a'); return __lwcLoad(__lwcLoad('b')); }\nconst s = 'import(x)';\n");
        // In a component it sits after registerPublicProperties; a removed member still adds it.
        let code = compile_component_for_ssr(&format!("{DECORATED}class B {{ @api p; }}\nexport default class C extends LightningElement {{\n  renderedCallback() {{ import('a'); }}\n}}\n"), "x/c/c.js", &opts(true, "L")).unwrap().code;
        assert!(code.contains("import { registerPublicProperties as __registerPublicProperties } from \"@lwc/ssr-runtime\";\nimport { load as __lwcLoad } from \"L\";\nimport "), "{code}");
        assert!(!code.contains("import('a')") && !code.contains("__lwcLoad('a')"), "{code}");
        assert!(code.ends_with("\n__setStaticInternals(C, \"x-c\", [], null ?? null, \"sync\", __lwcTmpl);\n"), "{code}");
    }

    /// generate-markup.ts: literals are JSON strings (`b.literal`), and the call is pushed unconditionally.
    #[test]
    fn generate_markup_literals() {
        let opts = CompileOptions { name: "fo'o".into(), namespace: "x".into(), ..Default::default() };
        let code = compile_component_for_ssr("import { LightningElement } from 'lwc';\nexport default class C extends LightningElement {}\n", "x/fo'o/fo'o.js", &opts).unwrap().code;
        assert!(code.starts_with("import { setStaticInternals as __setStaticInternals } from \"@lwc/ssr-runtime\";\nimport __lwcTmpl from \"./fo'o.html\";\n"), "{code}");
        assert!(code.ends_with("__setStaticInternals(C, \"x-fo'o\", [], null ?? null, \"sync\", __lwcTmpl);\n"), "{code}");
    }

    #[test]
    fn default_export_expression_extracted_for_lwc_only() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let src = "import { LightningElement } from 'lwc';\nconst Component = class extends LightningElement {};\nexport default wrap(Component);\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        assert!(r.code.contains("const __lwcDefaultExport = wrap(Component);\nexport default __lwcDefaultExport;"), "{}", r.code);
        assert!(r.code.contains("__setStaticInternals(__lwcDefaultExport, \"x-c\""), "{}", r.code);
        let adapter = "export default { Adapter };\n";
        assert_eq!(compile_component_for_ssr(adapter, "x/c/c.js", &opts).unwrap().code, adapter);
    }

    #[test]
    fn default_exported_binding_resolves_through_scope() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let src = "import { LightningElement } from 'lwc';\nfunction f() { class Cmp extends Object {} return Cmp; }\nexport default f;\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        assert!(!r.code.contains("__setStaticInternals"), "{}", r.code);
        let src = "import { LightningElement } from 'lwc';\nclass Cmp extends LightningElement {}\nexport default Cmp;\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        assert!(r.code.contains("__setStaticInternals(Cmp, \"x-c\""), "{}", r.code);
    }

    #[test]
    fn duplicate_default_export_is_not_fatal() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let src = "import { LightningElement } from 'lwc';\nexport default class extends LightningElement {}\nexport default async function rando() {}\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        assert!(r.code.contains("__setStaticInternals(DefaultComponentName, \"x-c\""), "{}", r.code);
        assert!(compile_component_for_ssr("export default class {", "x/c/c.js", &opts).is_err());
    }

    #[test]
    fn imports_rewritten_on_the_ast() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let src = "import { LightningElement as LE, api, track } from \"lwc\";\nimport { wire } from '@lwc/ssr-runtime';\nimport sheet from './c.scoped.css';\nimport { named } from './n.scoped.css';\nexport { createElement } from 'lwc';\nexport * from 'lwc';\nexport default class C extends LE {\n    @api x;\n}\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        let c = &r.code;
        assert!(c.starts_with("import { setStaticInternals as __setStaticInternals } from \"@lwc/ssr-runtime\";\nimport __lwcTmpl from \"./c.html\";\n"), "{c}");
        assert!(c.contains("import { LightningElement as LE } from \"@lwc/ssr-runtime\";"), "{c}");
        assert!(c.contains("\nimport \"@lwc/ssr-runtime\";\n"), "{c}");
        assert!(c.contains("from './c.scoped.css?scoped=true'"), "{c}");
        assert!(c.contains("from './n.scoped.css';"), "{c}");
        assert!(c.contains("export { createElement } from '@lwc/ssr-runtime';\nexport * from '@lwc/ssr-runtime';"), "{c}");
        assert!(c.contains("__setStaticInternals(C, \"x-c\", [\"x\"]"), "{c}");
    }

    #[test]
    fn non_lwc_module_gets_import_edits_only() {
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let src = "import { api, createElement } from 'lwc';\nexport const f = () => createElement;\n";
        let r = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap();
        assert_eq!(r.code, "import { createElement } from \"@lwc/ssr-runtime\";\nexport const f = () => createElement;\n");
    }

    #[test]
    fn tag_kebab() {
        assert_eq!(generate_custom_element_tag_name("x", "fooBar"), "x-foo-bar");
    }

    const LE: &str = "import { LightningElement } from 'lwc';\n";

    fn indexed(body: &str) -> (IndexResult, String) {
        let src = format!("{LE}{body}");
        let (r, edits, _) = index(&src, "x/c/c.js", &CompileOptions::default()).unwrap();
        let out = apply(&src, edits).unwrap();
        (r, out.split_once('\n').unwrap().1.to_string()) // drop the (rewritten) import line
    }

    fn component(exported: &str) -> IndexResult {
        IndexResult::Component { exported_name: exported.into(), public_properties: vec![], registered_base_class_props: false, wire_adapters: vec![] }
    }

    /// Names as upstream's `lwcDefaultExportName ?? lwcClassName` (oracle runs of @lwc/ssr-compiler 9.5.3).
    #[test]
    fn index_result_names() {
        assert_eq!(indexed("export default class Foo extends LightningElement {}\n").0, component("Foo"));
        assert_eq!(indexed("export default class extends LightningElement {}\n").0, component("DefaultComponentName"));
        assert_eq!(indexed("const Foo = class extends LightningElement {};\nexport default Foo;\n").0, component("Foo"));
        // Upstream keeps lwcClassName null here and still compiles against the default export name.
        assert_eq!(indexed("export default (class extends LightningElement {});\n").0, component("DefaultComponentName"));
        assert_eq!(indexed("export default foo(class extends LightningElement {});\n").0, component("__lwcDefaultExport"));
        assert_eq!(indexed("export default { a: 1 };\n").0, IndexResult::NotLwc);
    }

    /// index.ts MethodDefinition `constructor` / Super / lifecycle removal, as @lwc/ssr-compiler 9.5.3 prints them.
    #[test]
    fn class_members_on_the_walk() {
        let c = version_comment();
        let out = |body: &str| indexed(body).1.replace(&format!("  {c}\n"), "").replace(PROPS_ARG, "P");
        // Rest params and spread args; an arrow's super() is left alone, a nested block's is not.
        assert_eq!(
            out("export default class C extends LightningElement {\n  constructor(a, ...r) { super(a, ...r); const f = () => super(); if (1) { super(); } }\n  connectedCallback() {}\n  disconnectedCallback() {}\n  errorCallback(e) {}\n  @foo renderedCallback() {}\n}\n"),
            "export default class C extends LightningElement {\n  constructor(a, ...r, P) { super(a, ...r, P); const f = () => super(); if (1) { super(P); } }\n  connectedCallback() {}\n  \n  \n  \n}\n"
        );
        // currentComponent stays set in a nested class: it gets the rewrite too.
        assert_eq!(
            out("export default class C extends LightningElement {\n  m() { return class extends Object { constructor(a) { super(a); } renderedCallback() {} }; }\n}\n"),
            "export default class C extends LightningElement {\n  m() { return class extends Object { constructor(a, P) { super(a, P); }  }; }\n}\n"
        );
        // Computed identifier keys and static members count; string keys do not.
        assert_eq!(
            out("export default class C extends LightningElement {\n  [constructor](a) {}\n  static constructor() {}\n  'renderedCallback'() {}\n}\n"),
            "export default class C extends LightningElement {\n  [constructor](a, P) {}\n  static constructor(P) {}\n  'renderedCallback'() {}\n}\n"
        );
        // Program.leave's re-traversal of an extracted `export default <expression>` appends twice.
        assert_eq!(
            out("export default mix(class extends LightningElement {\n  constructor(a) { super(a); }\n  renderedCallback() {}\n});\n"),
            "const __lwcDefaultExport = mix(class extends LightningElement {\n  constructor(a, P, P) { super(a, P, P); }\n  \n});\nexport default __lwcDefaultExport;\n"
        );
        // A removed member's nested edits go with it; a class that is not a component is untouched.
        assert_eq!(
            out("class B extends LightningElement {\n  constructor() { super(); }\n  renderedCallback() {}\n}\nexport default class C extends B {\n  renderedCallback() { return class { constructor() {} }; }\n}\n"),
            "class B extends LightningElement {\n  constructor() { super(); }\n  renderedCallback() {}\n}\nexport default class C extends B {\n  \n}\n"
        );
    }

    /// index.ts Identifier visitor: LWC1202 at the identifier, in any module, first error wins
    /// (locations from @lwc/ssr-compiler 9.5.3: 1-based line, 0-based column, UTF-16 start).
    #[test]
    fn reserved_identifier_at_the_node() {
        const H: &str = "import { LightningElement, api, wire } from 'lwc';\n";
        let cls = |members: &str| format!("{H}export default class C extends LightningElement {{\n{members}\n}}\n");
        let at = |src: &str| {
            let e = index(src, "x/c/c.js", &CompileOptions::default()).unwrap_err();
            let l = e.diagnostics[0].location.unwrap();
            (e.message, l.line, l.column, l.start, l.length)
        };
        let msg = "LWC1202: Identifier name cannot start with \"__lwc\".".to_string();
        // Children of a removed member are still visited.
        assert_eq!(at(&cls("  renderedCallback() { __lwcX; }")), (msg.clone(), 3, 23, Some(124), Some(6)));
        assert_eq!(at("const __lwcX = 1;\nexport default {};\n"), (msg.clone(), 1, 6, Some(6), Some(6)));
        assert_eq!(at(&cls("  m() { return this.__lwcY; }")).1..=at(&cls("  m() { return this.__lwcY; }")).2, 3..=20);
        assert_eq!(at(&cls("  __lwcK = 1;")).2, 2);
        assert_eq!(at(&cls("  m() { __lwcL: for(;;) { break __lwcL; } }")).2, 8);
        assert_eq!(at(&format!("import {{ __lwcI }} from 'x';\n{}", cls(""))).1..=at(&format!("import {{ __lwcI }} from 'x';\n{}", cls(""))).2, 1..=9);
        assert_eq!(at(&format!("{H}const s = '\u{1F600}';\nexport default class C extends LightningElement {{\n  y = __lwcB;\n}}\n")), (msg.clone(), 4, 6, Some(123), Some(6)));
        // The member visitor runs before its key, and pre-order puts earlier members first.
        assert_eq!(at(&cls("  @api __lwcX = true;")).0, "LWC1099: Boolean public property must default to false.");
        assert_eq!(at(&cls("  @api x = true;\n  y = __lwcB;")).0, "LWC1099: Boolean public property must default to false.");
        assert_eq!(at(&cls("  y = __lwcB;\n  @api x = true;")).0, msg);
        // Not identifiers, or never traversed: strings, comments, private names, decorators.
        for ok in [cls("  m() { return '__lwcQ'; } // __lwcZ"), cls("  #__lwcP = 1;"), cls("  @foo(__lwcA) x;"), cls("  @api @foo(__lwcA) x;"), format!("{H}@__lwcD\nexport default class C extends LightningElement {{}}\n")] {
            assert!(index(&ok, "x/c/c.js", &CompileOptions::default()).is_ok(), "{ok}");
        }
    }

    /// generate-markup.ts:54 `b.identifier(null!)` throws: a component with no name is an error, never "".
    #[test]
    fn nameless_component_is_an_error() {
        let src = format!("{LE}foo(class extends LightningElement {{}});\n");
        let err = index(&src, "x/c/c.js", &CompileOptions::default()).unwrap_err();
        assert_eq!(err.message, "Expected the value to be a `string` but got a `null`. The value is null.");
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        assert!(compile_component_for_ssr(&src, "x/c/c.js", &opts).is_err());
    }

    /// index.ts:136-148 / :185-197 placement, as the oracle prints it: last item of every matched
    /// class body; twice for a class inside an extracted `export default <expression>`.
    #[test]
    fn version_comment_placement() {
        let c = version_comment();
        assert_eq!(indexed("export default class Foo extends LightningElement {\n  x = 1;\n}\n").1, format!("export default class Foo extends LightningElement {{\n  x = 1;\n  {c}\n}}\n"));
        assert_eq!(indexed("export default class Foo extends LightningElement {}\n").1, format!("export default class Foo extends LightningElement {{  {c}\n}}\n"));
        assert_eq!(indexed("const M = class extends LightningElement {};\nexport default class Foo extends LightningElement {}\n").1,
            format!("const M = class extends LightningElement {{  {c}\n}};\nexport default class Foo extends LightningElement {{  {c}\n}}\n"));
        assert_eq!(indexed("export default foo(class extends LightningElement {\n  x = 1;\n});\n").1,
            format!("const __lwcDefaultExport = foo(class extends LightningElement {{\n  x = 1;\n  {c}\n  {c}\n}});\nexport default __lwcDefaultExport;\n"));
        // Only matched classes: a nested non-component class gets none.
        assert_eq!(indexed("class Base extends Object {}\nexport default class Foo extends LightningElement {\n  m() { return class extends Base {}; }\n}\n").1,
            format!("class Base extends Object {{}}\nexport default class Foo extends LightningElement {{\n  m() {{ return class extends Base {{}}; }}\n  {c}\n}}\n"));
        // Not LWC: nothing.
        assert!(!indexed("export default { a: 1 };\n").1.contains(&c));
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let r = compile_component_for_ssr(&format!("{LE}export default class Foo extends LightningElement {{\n  x = 1;\n}}\n"), "x/c/c.js", &opts).unwrap();
        assert!(r.code.contains(&format!("  x = 1;\n  {c}\n}}")), "{}", r.code);
    }
}
