//! Minimal sync `compileComponentForSSR`: rewrite `lwc` → `@lwc/ssr-runtime`,
//! inject template import + `setStaticInternals`. Catalogs `@api` / `@wire`;
//! strips decorator lines. Non-lwc modules pass through. Names anonymous default classes.

use lwc_shared::{CompileError, CompileOptions, CompileResult};
use std::path::Path;

mod class_rewrite;
mod dynamic_imports;

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

fn pascal_case(name: &str) -> String {
    name.split(|c| c == '-' || c == '_' || c == '.')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut cs = s.chars();
            match cs.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
            }
        })
        .collect()
}

fn class_has_extends(src: &str, class_name: &str) -> bool {
    for line in src.lines() {
        let t = line.trim();
        let rest = if let Some(r) = t.strip_prefix("export default class ") {
            r
        } else if let Some(r) = t.strip_prefix("class ") {
            r
        } else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if name == class_name {
            return rest.contains(" extends ");
        }
    }
    false
}

fn should_transform_component(src: &str, class_name: &str) -> bool {
    imports_from_lwc(src)
        || class_has_extends(src, class_name)
        || find_le_class_expression_binding(src).is_some()
}


fn validate_reserved_identifiers(src: &str, filename: &str) -> Option<CompileError> {
    // Reject user identifiers starting with __lwc (SSR reserved). Skip our own emit tokens by running on input src.
    let mut in_string: Option<char> = None;
    let mut i = 0;
    let b = src.as_bytes();
    while i < b.len() {
        let c = b[i] as char;
        if let Some(q) = in_string {
            if c == '\\' { i += 2; continue; }
            if c == q { in_string = None; }
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_string = Some(c);
            i += 1;
            continue;
        }
        // identifier start
        if c == '_' || c.is_ascii_alphabetic() {
            let start = i;
            i += 1;
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' { i += 1; } else { break; }
            }
            let ident = &src[start..i];
            if ident.starts_with("__lwc") {
                return Some(CompileError::lwc(
                    1202,
                    "Identifier name cannot start with \"__lwc\".",
                    filename,
                    None,
                    None,
                ));
            }
            continue;
        }
        i += 1;
    }
    None
}

fn validate_wire_decorators(src: &str, filename: &str) -> Option<CompileError> {
    // Collect imported binding names from import declarations.
    let mut module_bindings = std::collections::HashSet::new();
    let mut const_names = std::collections::HashSet::new();
    let mut let_names = std::collections::HashSet::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("import ") {
            if let (Some(a), Some(b)) = (t.find('{'), t.find('}')) {
                for part in t[a + 1..b].split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    let local = if let Some((_, right)) = part.split_once(" as ") {
                        right.trim()
                    } else {
                        part.trim()
                    };
                    if !local.is_empty() {
                        module_bindings.insert(local.to_string());
                    }
                }
            }
            // import Foo from '...'
            if t.contains(" from ") && !t.contains('{') {
                if let Some(rest) = t.strip_prefix("import ") {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    if !name.is_empty() && name != "type" {
                        module_bindings.insert(name.to_string());
                    }
                }
            }
        }
        if let Some(rest) = t.strip_prefix("const ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                const_names.insert(name);
            }
        }
        if let Some(rest) = t.strip_prefix("let ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                let_names.insert(name);
            }
        }
    }

    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if !(t.starts_with("@wire(") || t.starts_with("@wire (") || t == "@wire") {
            i += 1;
            continue;
        }

        // Gather consecutive @wire lines (1105) and full wire call text
        let mut wire_count = 0;
        let mut j = i;
        let mut block = String::new();
        while j < lines.len() {
            let tj = lines[j].trim();
            if tj.starts_with("@wire") {
                wire_count += 1;
                // accumulate this wire call
                let mut depth = tj.chars().filter(|c| *c == '(').count() as i32
                    - tj.chars().filter(|c| *c == ')').count() as i32;
                block.push_str(tj);
                block.push('\n');
                let mut k = j;
                while depth > 0 && k + 1 < lines.len() {
                    k += 1;
                    block.push_str(lines[k]);
                    block.push('\n');
                    depth += lines[k].chars().filter(|c| *c == '(').count() as i32;
                    depth -= lines[k].chars().filter(|c| *c == ')').count() as i32;
                }
                j = k + 1;
                // Same-line `@wire(A) prop;` — member already on this call; not stacked.
                let call_end_line = lines[k].trim();
                if let Some(rp) = call_end_line.rfind(')') {
                    let after = call_end_line[rp + 1..].trim().trim_start_matches(';').trim();
                    if !after.is_empty() && !after.starts_with('@') {
                        break;
                    }
                }
                // peek next non-empty — stacked `@wire` only when no member yet
                let mut p = j;
                while p < lines.len() && lines[p].trim().is_empty() {
                    p += 1;
                }
                if p < lines.len() && lines[p].trim().starts_with("@wire") {
                    continue;
                }
                break;
            } else if tj.is_empty() {
                j += 1;
            } else {
                break;
            }
        }
        if wire_count > 1 {
            return Some(CompileError::lwc(
                1105,
                "Method or property can only have 1 @wire decorator",
                filename,
                None,
                None,
            ));
        }

        // Member after wire (and possible other decorators already checked)
        let mut m = j;
        while m < lines.len() && lines[m].trim().is_empty() {
            m += 1;
        }
        if m < lines.len() {
            let member = lines[m].trim();
            // MethodDefinition with computed key (get/set/method) — PropertyDefinition fields are allowed.
            let is_methodish = member.starts_with("get ")
                || member.starts_with("set ")
                || (member.contains('[') && member.contains('(') && !member.contains('='));
            if is_methodish && member.contains('[') {
                return Some(CompileError::new(
                    "@wire cannot be used on computed properties in SSR context.",
                    filename,
                ));
            }
        }

        // Parse @wire( args )
        let open = block.find('(');
        let close = block.rfind(')');
        if open.is_none() {
            // @wire without call — treat as no args
            return Some(CompileError::lwc(
                1092,
                "@wire expects an adapter as first parameter. @wire(adapter: WireAdapter, config?: any).",
                filename,
                None,
                None,
            ));
        }
        let open = open.unwrap();
        let close = close.unwrap_or(block.len());
        let args = block[open + 1..close].trim();
        if args.is_empty() {
            return Some(CompileError::lwc(
                1092,
                "@wire expects an adapter as first parameter. @wire(adapter: WireAdapter, config?: any).",
                filename,
                None,
                None,
            ));
        }

        // Split adapter / config at top-level comma
        let mut depth = 0i32;
        let mut split = None;
        for (idx, c) in args.char_indices() {
            match c {
                '(' | '{' | '[' => depth += 1,
                ')' | '}' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    split = Some(idx);
                    break;
                }
                _ => {}
            }
        }
        let (adapter, config) = if let Some(idx) = split {
            (args[..idx].trim(), Some(args[idx + 1..].trim()))
        } else {
            (args.trim(), None)
        };

        // Adapter checks
        if adapter.starts_with('\'')
            || adapter.starts_with('"')
            || adapter.starts_with('`')
            || adapter.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            return Some(CompileError::lwc(
                1097,
                "@wire expects a function identifier as first parameter.",
                filename,
                None,
                None,
            ));
        }
        if adapter.contains('[') {
            return Some(CompileError::lwc(
                1131,
                "@wire identifier cannot contain computed properties",
                filename,
                None,
                None,
            ));
        }
        // nested member: a.b.c
        if adapter.matches('.').count() >= 2 {
            return Some(CompileError::lwc(
                1132,
                "@wire identifier cannot contain nested member expressions",
                filename,
                None,
                None,
            ));
        }
        // identifier or simple member a.b
        let root = adapter.split('.').next().unwrap_or(adapter).trim();
        if !module_bindings.contains(root) {
            // local const/let/function → SSR golden uses 1200 for non-module binding
            if const_names.contains(root) || let_names.contains(root) || root.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                // if it's a bare identifier not imported
                if !adapter.contains('.') {
                    return Some(CompileError::lwc(
                        1200,
                        "Computed property in @wire config must be a constant or primitive literal.",
                        filename,
                        None,
                        None,
                    ));
                }
            }
        }

        if let Some(cfg) = config {
            let cfg = cfg.trim();
            if !cfg.starts_with('{') {
                return Some(CompileError::lwc(
                    1094,
                    "@wire expects a configuration object expression as second parameter.",
                    filename,
                    None,
                    None,
                ));
            }
            // template literal computed key
            if cfg.contains("[`") || cfg.contains("[`") {
                return Some(CompileError::lwc(
                    1199,
                    "Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.",
                    filename,
                    None,
                    None,
                ));
            }
            if cfg.contains("[`") {
                return Some(CompileError::lwc(
                    1199,
                    "Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.",
                    filename,
                    None,
                    None,
                ));
            }
            // detect [`...`]
            if cfg.contains("[`") || cfg.find("[`").is_some() {
                return Some(CompileError::lwc(1199, "Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.", filename, None, None));
            }
            if cfg.contains('[') {
                // [/key1/] regexp
                if cfg.contains("[/") {
                    return Some(CompileError::lwc(
                        1200,
                        "Computed property in @wire config must be a constant or primitive literal.",
                        filename,
                        None,
                        None,
                    ));
                }
                // [[symbol]] nested array expression
                if cfg.contains("[[") {
                    return Some(CompileError::lwc(
                        1200,
                        "Computed property in @wire config must be a constant or primitive literal.",
                        filename,
                        None,
                        None,
                    ));
                }
                // [key1] where key1 is let
                for name in &let_names {
                    if cfg.contains(&format!("[{name}]")) {
                        return Some(CompileError::lwc(
                            1200,
                            "Computed property in @wire config must be a constant or primitive literal.",
                            filename,
                            None,
                            None,
                        ));
                    }
                }
                // template literal key [`key1`]
                if cfg.contains("[`") || cfg.contains("[`") {
                    return Some(CompileError::lwc(
                        1199,
                        "Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.",
                        filename,
                        None,
                        None,
                    ));
                }
            }
            // explicit template literal check
            if let Some(idx) = cfg.find('[') {
                let after = &cfg[idx + 1..];
                if after.starts_with('`') {
                    return Some(CompileError::lwc(
                        1199,
                        "Cannot use a template literal as a computed property key. Instead, use a string or extract the value to a constant.",
                        filename,
                        None,
                        None,
                    ));
                }
            }
        }

        i = j;
    }
    None
}


fn validate_decorators(src: &str, filename: &str) -> Option<CompileError> {
    let lines: Vec<&str> = src.lines().collect();
    let mut api_gets = std::collections::HashSet::new();
    let mut api_sets = std::collections::HashSet::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        let member_after_api: Option<String> = if t == "@api" || t.starts_with("@api ") || t.starts_with("@api(") {
            if let Some(rest) = t.strip_prefix("@api").map(str::trim) {
                if !rest.is_empty() && !rest.starts_with('(') {
                    Some(rest.to_string())
                } else {
                    let mut j = i + 1;
                    while j < lines.len() && lines[j].trim().is_empty() {
                        j += 1;
                    }
                    if j < lines.len() {
                        Some(lines[j].trim().to_string())
                    } else {
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(m) = member_after_api {
            if let Some(r) = m.strip_prefix("get ") {
                let name: String = r
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    api_gets.insert(name);
                }
            } else if let Some(r) = m.strip_prefix("set ") {
                let name: String = r
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    api_sets.insert(name);
                }
            }
        }
        if t.starts_with("@wire") {
            let mut prev = i;
            while prev > 0 {
                prev -= 1;
                let p = lines[prev].trim();
                if p.is_empty() {
                    continue;
                }
                // Bare decorator only (`@api` / `@track`), not `@track foo = []`.
                if p == "@api" || p == "@track" || p.starts_with("@api(") || p.starts_with("@track(") {
                    let other = if p.starts_with("@api") { "api" } else { "track" };
                    return Some(CompileError::lwc(
                        1095,
                        format!("@wire method or property cannot be used with @{other}"),
                        filename,
                        None,
                        None,
                    ));
                }
                break;
            }
            let mut j = i + 1;
            let mut depth = t.chars().filter(|c| *c == '(').count() as i32
                - t.chars().filter(|c| *c == ')').count() as i32;
            while depth > 0 && j < lines.len() {
                depth += lines[j].chars().filter(|c| *c == '(').count() as i32;
                depth -= lines[j].chars().filter(|c| *c == ')').count() as i32;
                j += 1;
            }
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() {
                let n = lines[j].trim();
                if n.starts_with("@api") || n.starts_with("@track") {
                    let other = if n.starts_with("@api") { "api" } else { "track" };
                    return Some(CompileError::lwc(
                        1095,
                        format!("@wire method or property cannot be used with @{other}"),
                        filename,
                        None,
                        None,
                    ));
                }
            }
        }
        i += 1;
    }
    for name in api_gets.intersection(&api_sets) {
        return Some(CompileError::lwc(
            1112,
            format!(
                "@api get {name} and @api set {name} detected in class declaration. Only one of the two needs to be decorated with @api."
            ),
            filename,
            None,
            None,
        ));
    }
    None
}



fn is_export_named_as_default(src: &str) -> bool {
    src.lines().any(|l| {
        let t = l.trim();
        t.starts_with("export {") && t.contains(" as default")
    })
}

/// Rewrite `from 'lwc'` → `@lwc/ssr-runtime` without injecting setStaticInternals.



fn line_imports_lightning_element(t: &str) -> bool {
    // Avoid matching identifiers like `notLightningElement`.
    for part in t.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$') {
        if part == "LightningElement" {
            return true;
        }
    }
    false
}

fn dedupe_lightning_element_import(src: &str) -> String {
    // Import bindings only (not `extends LightningElement`).
    let has_le_elsewhere = {
        let mut in_import = false;
        let mut from_runtime = false;
        let mut found = false;
        for line in src.lines() {
            let t = line.trim();
            if t.starts_with("import ") {
                in_import = true;
                from_runtime = t.contains("@lwc/ssr-runtime");
                if line_imports_lightning_element(t) && !from_runtime {
                    found = true;
                }
                if t.contains(';') {
                    in_import = false;
                }
                continue;
            }
            if in_import {
                if t.contains("@lwc/ssr-runtime") {
                    from_runtime = true;
                }
                if line_imports_lightning_element(t) && !from_runtime {
                    found = true;
                }
                if t.contains(';') || t.starts_with("} from") {
                    in_import = false;
                }
            }
        }
        found
    };
    if !has_le_elsewhere {
        return src.to_string();
    }
    let mut out = String::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("import ") && t.contains("@lwc/ssr-runtime") && t.contains("LightningElement") {
            if let (Some(a), Some(b)) = (t.find('{'), t.find('}')) {
                let keep: Vec<_> = t[a + 1..b]
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty() && !p.starts_with("LightningElement"))
                    .collect();
                if keep.is_empty() {
                    continue;
                }
                out.push_str(&format!("import {{ {} }} from '@lwc/ssr-runtime';\n", keep.join(", ")));
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
fn hoist_ssr_runtime_imports(src: &str) -> String {
    let mut runtime_lines = Vec::new();
    let mut tmpl_lines = Vec::new();
    let mut other = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("import ") && t.contains("@lwc/ssr-runtime") {
            runtime_lines.push(line.to_string());
        } else if t.starts_with("import ") && t.contains("__lwcTmpl") {
            tmpl_lines.push(line.to_string());
        } else {
            other.push(line.to_string());
        }
    }
    let mut out = String::new();
    for l in &runtime_lines {
        out.push_str(l);
        out.push('\n');
    }
    for l in &tmpl_lines {
        out.push_str(l);
        out.push('\n');
    }
    for l in &other {
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn rewrite_scoped_css_imports(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    for line in src.lines() {
        let mut l = line.to_string();
        if l.contains(".scoped.css'") && !l.contains(".scoped.css?scoped=true'") {
            l = l.replace(".scoped.css'", ".scoped.css?scoped=true'");
        }
        if l.contains(".scoped.css\"") && !l.contains(".scoped.css?scoped=true\"") {
            l = l.replace(".scoped.css\"", ".scoped.css?scoped=true\"");
        }
        out.push_str(&l);
        out.push('\n');
    }
    out
}

fn rewrite_lwc_imports_only(src: &str) -> String {
    src.replace("from 'lwc'", &format!("from '{}'", crate::runtime_abi::RUNTIME_MODULE))
        .replace("from \"lwc\"", &format!("from \"{}\"", crate::runtime_abi::RUNTIME_MODULE))
}

fn imports_from_lwc(src: &str) -> bool {
    src.contains("from 'lwc'") || src.contains("from \"lwc\"")
}

/// Returns (class_name, src_with_anonymous_named).
fn resolve_default_class(src: &str, options_name: &str) -> Result<(String, String), ()> {
    // export default class Name ...
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("export default class ") {
            // anonymous: "extends ..." or "{" 
            let first = rest.trim_start();
            if first.starts_with("extends ") || first.starts_with('{') {
                let name = pascal_case(options_name);
                if name.is_empty() {
                    return Err(());
                }
                let mut out = String::with_capacity(src.len() + name.len() + 1);
                for l in src.lines() {
                    let lt = l.trim();
                    if lt.starts_with("export default class ") {
                        let r = lt.strip_prefix("export default class ").unwrap().trim_start();
                        if r.starts_with("extends ") || r.starts_with('{') {
                            // preserve indent
                            let indent = &l[..l.len() - l.trim_start().len()];
                            out.push_str(indent);
                            out.push_str("export default class ");
                            out.push_str(&name);
                            out.push(' ');
                            out.push_str(r);
                            out.push('\n');
                            continue;
                        }
                    }
                    out.push_str(l);
                    out.push('\n');
                }
                return Ok((name, out));
            }
            let name: String = first
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() && name != "extends" {
                return Ok((name, src.to_string()));
            }
        }
    }
    // class Name ...; export default Name;
    let mut class_names = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("class ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                class_names.push(name);
            }
        }
        if let Some(rest) = t.strip_prefix("export default ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if class_names.iter().any(|c| c == &name) {
                return Ok((name, src.to_string()));
            }
        }
        if t.starts_with("export {") && t.contains(" as default") {
            if let (Some(a), Some(b)) = (t.find('{'), t.find('}')) {
                for part in t[a + 1..b].split(',') {
                    let part = part.trim();
                    if let Some((left, right)) = part.split_once(" as ") {
                        if right.trim() == "default" {
                            let name = left.trim().to_string();
                            if !name.is_empty() {
                                return Ok((name, src.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }
    Err(())
}

#[derive(Debug, Clone)]
struct WireAdapter {
    adapter_id: String,
    config_obj: String,
    field: String,
    is_method: bool,
}

fn rewrite_dollar_config(config: &str) -> String {
    let mut out = String::new();
    let bytes = config.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\'' || c == '"' {
            let quote = c;
            let start = i;
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch == '\\' {
                    i += 2;
                    continue;
                }
                if ch == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let lit = &config[start..i];
            let inner = &lit[1..lit.len().saturating_sub(1)];
            if inner.starts_with('$') && inner.len() > 1 {
                let mut expr = String::from("instance");
                for p in inner[1..].split('.') {
                    expr.push_str(&format!("[\"{p}\"]"));
                }
                out.push_str(&expr);
            } else {
                out.push_str(lit);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn collect_wire_adapters(src: &str) -> Vec<WireAdapter> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("@wire(") || t.starts_with("@wire (") {
            let mut block = t.to_string();
            let mut depth = block.chars().filter(|c| *c == '(').count() as i32
                - block.chars().filter(|c| *c == ')').count() as i32;
            let mut j = i;
            while depth > 0 && j + 1 < lines.len() {
                j += 1;
                block.push('\n');
                block.push_str(lines[j]);
                depth += lines[j].chars().filter(|c| *c == '(').count() as i32;
                depth -= lines[j].chars().filter(|c| *c == ')').count() as i32;
            }
            let Some(open) = block.find('(') else {
                i += 1;
                continue;
            };
            let Some(close) = block.rfind(')') else {
                i += 1;
                continue;
            };
            let args = block[open + 1..close].trim();
            let (adapter_id, config_raw) = if let Some((a, rest)) = args.split_once(',') {
                (a.trim().to_string(), rest.trim().to_string())
            } else {
                (args.trim().to_string(), "{}".into())
            };
            let config_obj = if config_raw.is_empty() {
                "{}".into()
            } else {
                rewrite_dollar_config(&config_raw)
            };

            // Same-line `@wire(A) field;` — member after `)` on the call's end line.
            let mut same_line_member: Option<String> = None;
            let end_t = lines[j].trim();
            if let Some(rp) = end_t.rfind(')') {
                let after = end_t[rp + 1..].trim().trim_start_matches(';').trim();
                if !after.is_empty() && !after.starts_with('@') {
                    same_line_member = Some(after.to_string());
                }
            }

            let mut k = j + 1;
            while k < lines.len() && lines[k].trim().is_empty() {
                k += 1;
            }
            let mut m = if let Some(ref sl) = same_line_member {
                sl.as_str()
            } else if k >= lines.len() {
                i = j + 1;
                continue;
            } else {
                lines[k].trim()
            };
            let mut is_method = false;
            for prefix in ["async ", "static "] {
                if let Some(r) = m.strip_prefix(prefix) {
                    m = r.trim_start();
                }
            }
            if let Some(r) = m.strip_prefix("get ") {
                m = r;
            } else if let Some(r) = m.strip_prefix("set ") {
                m = r;
            } else if m.contains('(') {
                is_method = true;
            }
            let field: String = m
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !field.is_empty() && !adapter_id.is_empty() {
                out.push(WireAdapter {
                    adapter_id,
                    config_obj,
                    field,
                    is_method,
                });
            }
            i = if same_line_member.is_some() { j + 1 } else { k + 1 };
            continue;
        }
        i += 1;
    }
    out
}

fn emit_wire_adapters_lit(adapters: &[WireAdapter]) -> String {
    if adapters.is_empty() {
        return "null".into();
    }
    let mut parts = Vec::new();
    for a in adapters {
        let action = if a.is_method {
            format!("instance.{}(newValue)", a.field)
        } else {
            format!("instance.{} = newValue", a.field)
        };
        parts.push(format!(
            "{{ adapter: {}, dataCallback: (instance) => (newValue) => {{ {action}; }}, config: (instance) => ({}) }}",
            a.adapter_id, a.config_obj
        ));
    }
    format!("[{}]", parts.join(", "))
}

fn collect_public_props_in_range(src: &str, start_line: usize, end_line: usize) -> Vec<String> {
    let mut props = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = start_line;
    while i < end_line.min(lines.len()) {
        let t = lines[i].trim();
        let is_api = t == "@api" || t.starts_with("@api ") || t.starts_with("@api(");
        if is_api {
            if let Some(rest) = t.strip_prefix("@api").map(str::trim) {
                if !rest.is_empty() && !rest.starts_with('(') {
                    let mut m = rest;
                    for prefix in ["async ", "static ", "get ", "set ", "*"] {
                        if let Some(r) = m.strip_prefix(prefix) {
                            m = r.trim_start();
                        }
                    }
                    let name: String = m
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                        .collect();
                    if !name.is_empty() && !props.contains(&name) {
                        props.push(name);
                    }
                    i += 1;
                    continue;
                }
            }
            let mut j = i + 1;
            while j < end_line.min(lines.len()) && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < end_line.min(lines.len()) {
                let mut m = lines[j].trim();
                for prefix in ["async ", "static ", "get ", "set ", "*"] {
                    if let Some(r) = m.strip_prefix(prefix) {
                        m = r.trim_start();
                    }
                }
                let name: String = m
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() && !props.contains(&name) {
                    props.push(name);
                }
            }
        }
        i += 1;
    }
    props
}

fn collect_public_props_for_class(src: &str, class_name: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let name = if let Some(r) = t.strip_prefix("export default class ") {
            r.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect::<String>()
        } else if let Some(r) = t.strip_prefix("class ") {
            r.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect::<String>()
        } else if let Some(n) = class_expr_binding_name(t) {
            n
        } else {
            continue;
        };
        if name == class_name {
            start = Some(i);
            break;
        }
    }
    let Some(s) = start else {
        return collect_public_props_in_range(src, 0, lines.len());
    };
    // find matching closing brace at class nesting 0→done — approximate: next top-level class or EOF
    let mut end = lines.len();
    let mut depth = 0i32;
    let mut seen_body = false;
    for (i, line) in lines.iter().enumerate().skip(s) {
        for c in line.chars() {
            if c == '{' {
                depth += 1;
                seen_body = true;
            } else if c == '}' {
                depth -= 1;
                if seen_body && depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        }
        if end != lines.len() {
            break;
        }
    }
    collect_public_props_in_range(src, s, end)
}

/// Non-exported classes with @api → (class_name, props).
fn class_expr_binding_name(t: &str) -> Option<String> {
    // `const Base = class extends …` / `let Mid = class …`
    for prefix in ["const ", "let ", "var "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if name.is_empty() {
                return None;
            }
            if rest[name.len()..].trim_start().starts_with("= class") {
                return Some(name);
            }
        }
    }
    None
}

fn collect_base_class_api(src: &str, default_export_name: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("export default class ") {
            i += 1;
            continue;
        }
        let name_opt = if let Some(rest) = t.strip_prefix("class ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        } else {
            class_expr_binding_name(t)
        };
        let Some(name) = name_opt else {
            i += 1;
            continue;
        };
        if name == default_export_name {
            i += 1;
            continue;
        }
        let mut depth = 0i32;
        let mut seen = false;
        let mut end = lines.len();
        for (j, line) in lines.iter().enumerate().skip(i) {
            for c in line.chars() {
                if c == '{' {
                    depth += 1;
                    seen = true;
                } else if c == '}' {
                    depth -= 1;
                    if seen && depth == 0 {
                        end = j + 1;
                        break;
                    }
                }
            }
            if end != lines.len() {
                break;
            }
        }
        let props = collect_public_props_in_range(src, i, end);
        if !props.is_empty() {
            out.push((name, props));
        }
        i = end;
    }
    out
}

fn strip_decorators(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_wire = false;
    let mut depth = 0i32;
    for line in src.lines() {
        let t = line.trim();
        if in_wire {
            for c in t.chars() {
                if c == '(' {
                    depth += 1;
                } else if c == ')' {
                    depth -= 1;
                }
            }
            if depth <= 0 {
                in_wire = false;
            }
            continue;
        }
        // same-line @api member — keep member
        if let Some(rest) = t.strip_prefix("@api") {
            let rest = rest.trim_start();
            if !rest.is_empty() && !rest.starts_with('(') {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(indent);
                out.push_str(rest);
                out.push('\n');
                continue;
            }
            // bare @api or @api()
            continue;
        }
        if let Some(rest) = t.strip_prefix("@track") {
            let rest = rest.trim_start();
            if !rest.is_empty() && !rest.starts_with('(') {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(indent);
                out.push_str(rest);
                out.push('\n');
                continue;
            }
            continue;
        }
        if t.starts_with("@wire") {
            depth = 0;
            for c in t.chars() {
                if c == '(' {
                    depth += 1;
                } else if c == ')' {
                    depth -= 1;
                }
            }
            if depth > 0 {
                in_wire = true;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn inject_base_register(src: &str, bases: &[(String, Vec<String>)]) -> String {
    if bases.is_empty() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + 256);
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        let mut matched = None;
        if !t.starts_with("export default class ") {
            let name_opt = if let Some(rest) = t.strip_prefix("class ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            } else {
                class_expr_binding_name(t)
            };
            if let Some(name) = name_opt {
                if let Some((_, props)) = bases.iter().find(|(n, _)| n == &name) {
                    matched = Some((name, props.clone()));
                }
            }
        }
        if let Some((_name, props)) = matched {
            // copy until closing brace of class, inject static block before final }
            let mut depth = 0i32;
            let mut seen = false;
            while i < lines.len() {
                let line = lines[i];
                let mut inject_here = false;
                let mut new_depth = depth;
                for c in line.chars() {
                    if c == '{' {
                        new_depth += 1;
                        seen = true;
                    } else if c == '}' {
                        new_depth -= 1;
                        if seen && new_depth == 0 {
                            inject_here = true;
                        }
                    }
                }
                if inject_here {
                    // line contains the closing }; inject before last }
                    let props_lit = props
                        .iter()
                        .map(|p| format!("'{p}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let indent = "    ";
                    // find last } on line
                    if let Some(pos) = line.rfind('}') {
                        out.push_str(&line[..pos]);
                        out.push_str(&format!(
                            "{indent}static {{ __registerPublicProperties(this, [{props_lit}]); }}\n"
                        ));
                        out.push_str(&line[pos..]);
                        out.push('\n');
                    } else {
                        out.push_str(line);
                        out.push('\n');
                    }
                    i += 1;
                    break;
                } else {
                    out.push_str(line);
                    out.push('\n');
                    depth = new_depth;
                    i += 1;
                }
            }
            continue;
        }
        out.push_str(lines[i]);
        out.push('\n');
        i += 1;
    }
    out
}

fn rewrite_lwc_imports(src: &str, need_register: bool) -> String {
    let mut out = String::with_capacity(src.len() + 128);
    let mut saw_runtime = false;
    for line in src.lines() {
        let t = line.trim();
        let from_lwc = t.contains("from 'lwc'") || t.contains("from \"lwc\"");
        if from_lwc && t.starts_with("import ") {
            if let (Some(a), Some(b)) = (t.find('{'), t.find('}')) {
                let inner = &t[a + 1..b];
                let mut keep: Vec<String> = Vec::new();
                for part in inner.split(',') {
                    let name = part.trim().split_whitespace().next().unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    if matches!(name, "api" | "track" | "wire") {
                        continue;
                    }
                    keep.push(part.trim().to_string());
                }
                if !keep.iter().any(|k| k.contains("setStaticInternals")) {
                    keep.push("setStaticInternals as __setStaticInternals".into());
                }
                if need_register && !keep.iter().any(|k| k.contains("registerPublicProperties")) {
                    keep.push("registerPublicProperties as __registerPublicProperties".into());
                }
                out.push_str(&format!(
                    "import {{ {} }} from '{}';\n",
                    keep.join(", "),
                    crate::runtime_abi::RUNTIME_MODULE
                ));
                saw_runtime = true;
                continue;
            }
            let rewritten = t
                .replace(
                    "from 'lwc'",
                    &format!("from '{}'", crate::runtime_abi::RUNTIME_MODULE),
                )
                .replace(
                    "from \"lwc\"",
                    &format!("from \"{}\"", crate::runtime_abi::RUNTIME_MODULE),
                );
            out.push_str(&rewritten);
            out.push('\n');
            saw_runtime = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !saw_runtime {
        let extra = if need_register {
            ", registerPublicProperties as __registerPublicProperties"
        } else {
            ""
        };
        out.insert_str(
            0,
            &format!(
                // JS addGenerateMarkupFunction imports only what it emits; no `lwc` import means no LightningElement use.
                "import {{ setStaticInternals as __setStaticInternals{extra} }} from '{}';\n",
                crate::runtime_abi::RUNTIME_MODULE
            ),
        );
    }
    out
}



fn lightning_element_aliases(src: &str) -> std::collections::HashSet<String> {
    let mut aliases = std::collections::HashSet::new();
    aliases.insert("LightningElement".to_string());
    for line in src.lines() {
        let t = line.trim();
        if !(t.starts_with("import ") || t.starts_with("export ")) {
            continue;
        }
        let from_lwc = t.contains("from 'lwc'")
            || t.contains("from \"lwc\"")
            || t.contains("from '@lwc/ssr-runtime'")
            || t.contains("from \"@lwc/ssr-runtime\"");
        if !from_lwc {
            continue;
        }
        let (Some(a), Some(b)) = (t.find('{'), t.find('}')) else {
            continue;
        };
        for part in t[a + 1..b].split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            // LightningElement as LE  OR  LightningElement
            if let Some((left, right)) = part.split_once(" as ") {
                if left.trim() == "LightningElement" {
                    aliases.insert(right.trim().to_string());
                }
            } else if part == "LightningElement" {
                aliases.insert("LightningElement".to_string());
            }
        }
    }
    aliases
}

fn find_le_class_expression_binding(src: &str) -> Option<String> {
    let aliases = lightning_element_aliases(src);
    for line in src.lines() {
        let t = line.trim();
        let Some(name) = class_expr_binding_name(t) else {
            continue;
        };
        // `= class extends LightningElement` (or alias)
        if let Some(idx) = t.find("= class") {
            let after = t[idx + "= class".len()..].trim_start();
            if let Some(rest) = after.strip_prefix("extends ") {
                let super_name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if aliases.contains(&super_name) || super_name == "LightningElement" {
                    return Some(name);
                }
            }
        }
    }
    None
}

fn rewrite_export_default_expression(src: &str) -> Option<String> {
    // `export default <expr>;` where expr is not `class` and not a bare identifier.
    let mut out = String::with_capacity(src.len() + 64);
    let mut found = false;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("export default ") {
            let rest = rest.trim().trim_end_matches(';').trim();
            if rest.starts_with("class ") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            // bare identifier?
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if ident == rest {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            out.push_str(&format!("const __lwcDefaultExport = {rest};\n"));
            out.push_str("export default __lwcDefaultExport;\n");
            found = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if found {
        Some(out)
    } else {
        None
    }
}

fn ensure_default_tmpl_import(code: &str, stem: &str) -> String {
    let needle_dq = format!("from './{stem}.html'");
    let needle_sq = format!("from \"./{stem}.html\"");
    let has = code.contains(&needle_dq) || code.contains(&needle_sq);
    if has {
        // Rewrite `import local from './stem.html'` → `import __lwcTmpl from './stem.html'`
        // and rewrite uses of `local` when it's a simple default import alias.
        let mut local_name: Option<String> = None;
        let mut out = String::new();
        for line in code.lines() {
            let t = line.trim();
            if t.starts_with("import ") && (t.contains(&needle_dq) || t.contains(&needle_sq)) {
                // import tmpl from './x.html';  OR import __lwcTmpl from ...
                if let Some(rest) = t.strip_prefix("import ") {
                    let rest = rest.trim_start();
                    if !rest.starts_with('{') && !rest.starts_with('*') {
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                            .collect();
                        if !name.is_empty() && name != "__lwcTmpl" {
                            local_name = Some(name.clone());
                            out.push_str(&format!("import __lwcTmpl from './{stem}.html';\n"));
                            continue;
                        }
                    }
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        if let Some(local) = local_name {
            // Replace identifier uses of local with __lwcTmpl (word-boundary-ish).
            let mut replaced = String::new();
            let bytes = out.as_bytes();
            let loc = local.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if i + loc.len() <= bytes.len() && &bytes[i..i + loc.len()] == loc {
                    let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_' && bytes[i - 1] != b'$';
                    let after_i = i + loc.len();
                    let after_ok = after_i >= bytes.len() || (!bytes[after_i].is_ascii_alphanumeric() && bytes[after_i] != b'_' && bytes[after_i] != b'$');
                    // Don't rewrite the import we already fixed
                    if before_ok && after_ok {
                        replaced.push_str("__lwcTmpl");
                        i = after_i;
                        continue;
                    }
                }
                replaced.push(bytes[i] as char);
                i += 1;
            }
            return replaced;
        }
        return out;
    }
    let tmpl_import = format!("import __lwcTmpl from './{stem}.html';\n");
    if let Some(pos) = code.find("@lwc/ssr-runtime';\n") {
        let mut c = code.to_string();
        let insert_at = pos + "@lwc/ssr-runtime';\n".len();
        c.insert_str(insert_at, &tmpl_import);
        c
    } else if let Some(pos) = code.find("@lwc/ssr-runtime\";\n") {
        let mut c = code.to_string();
        let insert_at = pos + "@lwc/ssr-runtime\";\n".len();
        c.insert_str(insert_at, &tmpl_import);
        c
    } else {
        format!("{tmpl_import}{code}")
    }
}


/// Sync-only compileComponentForSSR.
pub fn compile_component_for_ssr(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, CompileError> {
    // compile-js ImportExpression visitor runs over the original program, before any other output.
    if let Some(err) = dynamic_imports::check_strict_specifiers(src, filename, options.dynamic_import_strict_specifier) {
        return Err(err);
    }
    // `export { X as default }` + lwc import: JS only rewrites the import (no setStaticInternals).
    if is_export_named_as_default(src) && imports_from_lwc(src) {
        // strip setStaticInternals that rewrite adds if we didn't want full transform —
        // rewrite_lwc_imports always injects setStaticInternals; use import-only rewriter.
        let code = rewrite_lwc_imports_only(src);
        return Ok(CompileResult {
            code,
            map: None,
            warnings: vec![],
            css_scope_tokens: None,
        });
    }

    let (class_name, named_src, leaf_for_props) = match resolve_default_class(src, &options.name) {
        Ok((name, src2)) => (name.clone(), src2, name),
        Err(_) => {
            // `const Component = class extends LE {}; export default <expr>`
            if let (Some(leaf), Some(rewritten)) = (
                find_le_class_expression_binding(src),
                rewrite_export_default_expression(src),
            ) {
                (
                    "__lwcDefaultExport".to_string(),
                    rewritten,
                    leaf,
                )
            } else if imports_from_lwc(src) {
                return Ok(CompileResult {
                    code: rewrite_lwc_imports_only(src),
                    map: None,
                    warnings: vec![],
                    css_scope_tokens: None,
                });
            } else {
                return Ok(CompileResult {
                    code: src.to_string(),
                    map: None,
                    warnings: vec![],
                    css_scope_tokens: None,
                });
            }
        }
    };

    // Wire adapters: default class with no `extends` and no `lwc` import → pass through.
    if !should_transform_component(&named_src, &class_name) {
        return Ok(CompileResult {
            code: if imports_from_lwc(src) {
                rewrite_lwc_imports_only(src)
            } else {
                src.to_string()
            },
            map: None,
            warnings: vec![],
            css_scope_tokens: None,
        });
    }

    if let Some(err) = validate_reserved_identifiers(src, filename) {
        return Err(err);
    }
    let named_src = class_rewrite::rewrite_component_classes(&named_src).map_err(|message| CompileError {
        message,
        filename: Some(filename.into()),
        diagnostics: vec![],
        name: None,
        line: None,
        column: None,
    })?;
    if let Some(err) = validate_wire_decorators(&named_src, filename) {
        return Err(err);
    }
    if let Some(err) = validate_decorators(&named_src, filename) {
        return Err(err);
    }

    let public_props = collect_public_props_for_class(&named_src, &leaf_for_props);
    let props_lit = if public_props.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            public_props
                .iter()
                .map(|p| format!("'{p}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let wires = match class_rewrite::collect_wire_adapters(&named_src) {
        Some(w) => w.into_iter().map(|(adapter_id, config_obj, field, is_method)| WireAdapter { adapter_id, config_obj, field, is_method }).collect(),
        None => collect_wire_adapters(&named_src),
    };
    let wire_lit = emit_wire_adapters_lit(&wires);
    let bases = collect_base_class_api(&named_src, &leaf_for_props);
    let need_register = !bases.is_empty();

    let mut code = strip_decorators(&named_src);
    code = inject_base_register(&code, &bases);
    code = rewrite_lwc_imports(&code, need_register);
    code = rewrite_scoped_css_imports(&code);
    code = dynamic_imports::rewrite_with_loader(&code, &options.dynamic_import_loader);
    code = hoist_ssr_runtime_imports(&code);

        let stem = module_stem(filename, &options.name);
    let tag = generate_custom_element_tag_name(&options.namespace, &options.name);
    code = ensure_default_tmpl_import(&code, &stem);
code = dedupe_lightning_element_import(&code);
    code = hoist_ssr_runtime_imports(&code);
    if !code.contains("__setStaticInternals(") {
        code.push_str(&format!(
            "\n__setStaticInternals({class_name}, '{tag}', {props_lit}, {wire_lit}, '{}', __lwcTmpl);\n",
            options.ssr_mode.js_value()
        ));
    }
    Ok(CompileResult {
        code,
        map: None,
        warnings: vec![],
        css_scope_tokens: None,
    })
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
                "__setStaticInternals(TextStatic, 'x-text-static', [], null, 'sync', __lwcTmpl)"
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
        assert!(r.code.contains("export default class CommentsText extends"), "{}", r.code);
        assert!(
            r.code.contains("__setStaticInternals(CommentsText, 'x-comments-text'"),
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
                "__setStaticInternals(Child, 'x-child', ['fieldApi', 'setterGetterApi'], null, 'sync', __lwcTmpl)"
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
        assert!(r.code.contains("__registerPublicProperties(this, ['value', 'level'])"), "{}", r.code);
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
        assert!(r.code.contains("__setStaticInternals(Component, 'x-component'"), "{}", r.code);
        assert!(r.code.contains("@lwc/ssr-runtime"), "{}", r.code);
    }

    #[test]
    fn wire_api_conflict_1095() {
        let src = "import { LightningElement, api, wire } from 'lwc';\nimport a from 'x/a';\nexport default class C extends LightningElement {\n    @api\n    @wire(a)\n    foo;\n}\n";
        let opts = CompileOptions { name: "c".into(), namespace: "x".into(), ..Default::default() };
        let err = compile_component_for_ssr(src, "x/c/c.js", &opts).unwrap_err();
        assert!(err.message.contains("LWC1095"), "{}", err.message);
    }

    #[test]
    fn tag_kebab() {
        assert_eq!(generate_custom_element_tag_name("x", "fooBar"), "x-foo-bar");
        assert_eq!(pascal_case("comments-text"), "CommentsText");
    }
}
