//! registerComponent wrapping (port of babel-plugin-component/src/component.ts).
use crate::constants::*;
use crate::tag_name;
use lwc_shared::CompileOptions;
use lwc_shared::api_version;
use lwc_shared::edits::Edit;
use oxc::ast::ast::{ExportDefaultDeclarationKind as K, Program, Statement};

fn base_name(filename: &str) -> &str {
    let file = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    match file.rfind('.') {
        Some(i) if i > 0 => &file[..i],
        _ => file,
    }
}

/// Unique local name, Babel-style (`_x`, `_x2`, ...), avoiding any substring clash in the source.
pub(crate) fn unique(src: &str, hint: &str) -> String {
    let base = format!("_{hint}");
    if !src.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|c| !src.contains(c.as_str()))
        .unwrap()
}

pub fn run(
    program: &Program<'_>,
    src: &str,
    filename: &str,
    o: &CompileOptions,
    edits: &mut Vec<Edit>,
) {
    if o.is_explicit_import {
        return;
    }
    for stmt in &program.body {
        let Statement::ExportDefaultDeclaration(decl) = stmt else {
            continue;
        };
        // needsComponentRegistration
        let (target_start, target_end, class_id) = match &decl.declaration {
            K::ClassDeclaration(c) => (
                c.span.start,
                c.span.end,
                c.id.as_ref().map(|i| i.name.to_string()),
            ),
            K::Identifier(id) if id.name != "undefined" && id.name != "null" => {
                (id.span.start, id.span.end, None)
            }
            K::CallExpression(e) => (e.span.start, e.span.end, None),
            K::ConditionalExpression(e) => (e.span.start, e.span.end, None),
            _ => continue,
        };
        let reg = unique(src, REGISTER_COMPONENT_ID);
        let tmpl = unique(src, TEMPLATE_KEY);
        let spec = format!("{REGISTER_COMPONENT_ID} as {reg}");
        // The template import is a new statement at the top (ahead of a leading
        // license comment). `registerComponent` joins the existing `lwc` import,
        // which sits after that comment. A fresh import above the license is
        // what makes the bundler drop the comment.
        let mut imports = format!(
            "import {tmpl} from {};\n",
            lwc_shared::js::string_literal(
                format!("./{}.html", base_name(filename)).encode_utf16()
            )
        );
        if !merge_lwc_specifier(edits, program, src, &spec) {
            imports.push_str(&format!(
                "import {{ {spec} }} from \"{LWC_PACKAGE_ALIAS}\";\n"
            ));
        }
        let mut props = vec![
            format!("{TEMPLATE_KEY}: {tmpl}"),
            format!(
                "{COMPONENT_NAME_KEY}: {}",
                lwc_shared::js::string_literal(
                    tag_name::generate_custom_element_tag_name(&o.namespace, &o.name)
                        .encode_utf16()
                )
            ),
            format!(
                "{API_VERSION_KEY}: {}",
                api_version::get_api_version_from_number(o.api_version)
            ),
        ];
        if !o.component_feature_flag_module_path.is_empty() {
            let ff = unique(src, COMPONENT_FEATURE_FLAG_KEY);
            let path =
                lwc_shared::js::string_literal(o.component_feature_flag_module_path.encode_utf16());
            imports.insert_str(0, &format!("import {ff} from {path};\n"));
            props.push(format!(
                "{COMPONENT_FEATURE_FLAG_KEY}: {{ value: Boolean({ff}), path: {path} }}"
            ));
        }
        if o.enable_synthetic_element_internals {
            props.push(format!("{SYNTHETIC_ELEMENT_INTERNALS_KEY}: true"));
        }
        if o.enable_private_methods {
            props.push(format!("{ENABLE_PRIVATE_METHODS_KEY}: true"));
        }
        let meta = format!("{{ {} }}", props.join(", "));
        let export = format!("\nexport default {COMPONENT_CLASS_ID};");
        edits.push(Edit::insert(0, imports));
        match class_id {
            // Named class: hoist declaration in place, register by id.
            Some(id) => {
                edits.push(Edit::replace(decl.span.start, target_start, ""));
                edits.push(Edit::replace(
                    target_end,
                    decl.span.end,
                    format!("\nconst {COMPONENT_CLASS_ID} = {reg}({id}, {meta});{export}"),
                ));
            }
            // Anonymous class / expression: wrap in place.
            None => {
                edits.push(Edit::replace(
                    decl.span.start,
                    target_start,
                    format!("const {COMPONENT_CLASS_ID} = {reg}("),
                ));
                edits.push(Edit::replace(
                    target_end,
                    decl.span.end,
                    format!(", {meta});{export}"),
                ));
            }
        }
        return; // only one default export per module
    }
}

/// Add `spec` to an existing `from "lwc"` import. A decorator rewrite of that
/// import is updated in place so the two edits do not overlap.
fn merge_lwc_specifier(
    edits: &mut Vec<Edit>,
    program: &Program<'_>,
    src: &str,
    spec: &str,
) -> bool {
    let Some(stmt) = program.body.iter().find_map(|s| match s {
        Statement::ImportDeclaration(d) if d.source.value == LWC_PACKAGE_ALIAS => Some(d.as_ref()),
        _ => None,
    }) else {
        return false;
    };
    if let Some(e) = edits
        .iter_mut()
        .find(|e| e.start == stmt.span.start && e.end == stmt.span.end)
    {
        if e.text.is_empty() {
            e.text = format!("import {{ {spec} }} from \"{LWC_PACKAGE_ALIAS}\";");
        } else if let Some(idx) = e.text.rfind('}') {
            e.text.insert_str(idx, &format!(", {spec}"));
        } else {
            return false;
        }
        return true;
    }
    let text = &src[stmt.span.start as usize..stmt.span.end as usize];
    let Some(rel) = text.rfind('}') else {
        return false;
    };
    edits.push(Edit::insert(
        stmt.span.start + rel as u32,
        format!(", {spec}"),
    ));
    true
}
