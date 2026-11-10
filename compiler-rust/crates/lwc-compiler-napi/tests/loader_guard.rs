//! Gate G2 (ship-packaging-plan.md §8, AB-D0005 #5): the ESM loader `index.js` only loads the binary and
//! re-exports it. It must not read the environment and must not mention any compile option key; option
//! decisions belong to Rust.
//!
//! Key set source (S1): the serde option structs in `src/lib.rs` (every `Deserialize` struct/enum field,
//! camelCased per `rename_all`, honouring `#[serde(rename)]` and `tag`), plus the instrumentation
//! callback keys that never cross serde. In S2 this switches to the generated AB-0008 option schema.
use std::collections::BTreeSet;

/// Option keys that are JS-only (callbacks), so not in the serde structs (D-0205).
const NON_SERDE_KEYS: [&str; 3] = ["instrumentation", "log", "incrementCounter"];

fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn attr_str(line: &str, key: &str) -> Option<String> {
    let i = line.find(&format!("{key} = \""))? + key.len() + 4;
    Some(line[i..].split('"').next()?.to_string())
}

/// Field names of one `ident: Type` field declaration (`pub ` optional).
fn field_ident(decl: &str) -> Option<&str> {
    let d = decl.trim().trim_start_matches("pub ").trim();
    let (id, _) = d.split_once(':')?;
    let id = id.trim();
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(id)
}

fn serde_option_keys(lib_rs: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let lines: Vec<&str> = lib_rs.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim();
        if !(l.starts_with("#[derive(") && l.contains("Deserialize")) {
            i += 1;
            continue;
        }
        // Container attributes until the item line.
        let (mut camel_case, mut j) = (false, i + 1);
        while j < lines.len() && lines[j].trim().starts_with("#[") {
            let a = lines[j];
            if a.contains("rename_all = \"camelCase\"") {
                camel_case = true;
            }
            if let Some(tag) = attr_str(a, "tag") {
                keys.insert(tag);
            }
            j += 1;
        }
        let item = lines[j].trim();
        assert!(item.starts_with("pub struct ") || item.starts_with("pub enum "), "unexpected item after derive: {item}");
        let is_enum = item.starts_with("pub enum ");
        let mut rename: Option<String> = None;
        j += 1;
        while j < lines.len() && lines[j].trim() != "}" {
            let f = lines[j].trim();
            if f.starts_with("#[serde(") {
                rename = attr_str(f, "rename");
            } else if is_enum {
                // Struct variants `Module { name: String }` carry option keys; tuple/unit variants don't.
                if let (Some(a), Some(b)) = (f.find('{'), f.rfind('}')) {
                    for decl in f[a + 1..b].split(',') {
                        if let Some(id) = field_ident(decl) {
                            keys.insert(if camel_case { camel(id) } else { id.to_string() });
                        }
                    }
                }
            } else if let Some(id) = field_ident(f) {
                keys.insert(rename.take().unwrap_or_else(|| if camel_case { camel(id) } else { id.to_string() }));
            }
            j += 1;
        }
        i = j;
    }
    keys
}

fn option_keys() -> BTreeSet<String> {
    let lib = std::fs::read_to_string(lwc_shared::cargo_manifest_dir!().join("src/lib.rs")).unwrap();
    let mut k = serde_option_keys(&lib);
    k.extend(NON_SERDE_KEYS.iter().map(|s| s.to_string()));
    k
}

/// `index.js` without `//` and `/* */` comments (the loader has no `//` inside strings or URLs).
fn code_only(js: &str) -> String {
    let mut out = String::new();
    let mut rest = js;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i..].split_once("*/").map_or("", |(_, r)| r);
    }
    out.push_str(rest);
    out.lines().map(|l| l.split_once("//").map_or(l, |(a, _)| a)).collect::<Vec<_>>().join("\n")
}

/// Every identifier-like token, so `.key`, `key:`, `{ key }`, `'key'` and `` `key` `` are all caught.
fn tokens(code: &str) -> BTreeSet<&str> {
    code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).filter(|t| !t.is_empty()).collect()
}

fn violations(js: &str, keys: &BTreeSet<String>) -> Vec<String> {
    let mut v = Vec::new();
    let compact: String = js.chars().filter(|c| !c.is_whitespace()).collect();
    for needle in ["process.env", "process[\"env\"]", "process['env']", "process?.env"] {
        if compact.contains(needle) {
            v.push(format!("environment access `{needle}`"));
        }
    }
    let code = code_only(js);
    let toks = tokens(&code);
    v.extend(keys.iter().filter(|k| toks.contains(k.as_str())).map(|k| format!("option key `{k}`")));
    v
}

#[test]
fn key_set_parses_the_option_structs() {
    let k = option_keys();
    for must in ["name", "namespace", "targetSSR", "apiVersion", "outputConfig", "sourcemap", "minify", "strictSpecifier", "tagName", "resolution", "type", "instrumentation"] {
        assert!(k.contains(must), "parser missed `{must}`: {k:?}");
    }
    assert!(!k.contains("message"), "non-option (napi object) struct leaked into the key set: {k:?}");
}

#[test]
fn guard_detects_planted_violations() {
    let k = option_keys();
    assert!(violations("const x = opts.namespace;", &k).iter().any(|v| v.contains("namespace")));
    assert!(violations("if (o['targetSSR']) {}", &k).iter().any(|v| v.contains("targetSSR")));
    assert!(violations("const { outputConfig } = o;", &k).iter().any(|v| v.contains("outputConfig")));
    assert!(violations("const p = process . env.X;", &k).iter().any(|v| v.contains("environment")));
    assert!(violations("// mentions namespace in a comment only\nexport const a = 1;", &k).is_empty());
}

#[test]
fn index_js_has_no_env_access_and_no_option_keys() {
    let js = std::fs::read_to_string(lwc_shared::cargo_manifest_dir!().join("index.js")).unwrap();
    let v = violations(&js, &option_keys());
    assert!(v.is_empty(), "index.js must not inspect options or the environment (AB-D0005 #5): {v:?}");
}
