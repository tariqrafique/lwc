//! AB-L0063 (5) / D-0248: an oxc diagnostic becomes a `CompileError` in exactly one function,
//! `parse.rs::first_error`, and no other lwc-js code can hold an oxc diagnostic value, so nothing can key
//! behavior on oxc's message text. Structural token gate over EVERY `.rs` file under `crates/lwc-js/src`
//! (recursive; comments and literals stripped; `#[cfg(test)]` code included).
//!
//! Why naming the type is not the whole rule (a value can be used without naming its type): oxc diagnostic
//! values come only from oxc's producers, `Parser` (`.parse()` / `.parse_expression()`), `SemanticBuilder`
//! (`.build()`) and `Transformer` (`.build*()`); every other public oxc function that returns one lives in a
//! private module. So:
//! - R1 `OxcDiagnostic`, `Diagnostics`, `LabeledSpan`, `oxc::diagnostics`, `oxc_diagnostics`, `miette` are named
//!   only inside `first_error`;
//! - R2 the producers (and oxc's other entry points `Compiler`, `CompilerInterface`, `regular_expression`) are
//!   named only in parse.rs; their return types (`ParserReturn`, ..) nowhere, so no one can destructure them;
//! - R3 in parse.rs, a producer's `.diagnostics` is read only as an argument of a `first_error(..)` call, and the
//!   `Diagnostics` accessors (`errors`, `warnings`, `into_vec`, `has_errors`, ..) only inside `first_error`;
//! - R4 parse.rs exports only `pub fn`s whose return types are on [`EXPORTS`] (no `impl Trait` or diagnostic
//!   type can leave it), so no other module can obtain a diagnostic value through it;
//! - R5 the boundary pair `first_error` / `syntax_error` uses only the identifiers on [`BOUNDARY_IDENTS`] and no
//!   `==`, `!=`, `<=`, `>=`, `&&`, `||` (no `if`, `match`, `contains`, ..): the text is formatted, never inspected;
//!   and the rest of parse.rs never names `message`;
//! - R6 no `include!` and no `#[path = ..]` (code outside lwc-js/src cannot be pulled in unscanned).
//! - R7 `with_check_syntax_error` (the user-source early-error check) is named only inside `parse_source`.
//!   Intermediate [`parse`] of span-edited source must not run it (D-0259 / Arch). `SemanticBuilder` without that
//!   flag remains allowed in parse.rs helpers that build a scope/node table of an already-parsed program.
//! - R8 parse / parse_with / parse_source / Parser::new call sites (including `use … as` aliases) are
//!   allowed only inside the closed D-0264 function list ([`PARSE_SITES`]): parse.rs entries,
//!   `transform_inner` (the one pre-print parse), and the three remaining residuals (`pass_with`,
//!   `unforgeables::run`, `async_functions`). A new site or an orphaned list row fails (D-0262 / D-0264).
//!   Official expiry tracking is Harness's D-0260 registry file (not edited here).
//! - R8b find/replace of `babelHelpers.` is allowed only inside [`BABELHELPERS_FIND`]. After D-0265
//!   step 1 that list is empty: helper members are rewritten on the Program by AST shape, and
//!   helper-body templates are localized from recorded AST spans. A text find/replace on printed
//!   output fails the gate.
//!
//! Residual, by construction outside this gate: after the boundary the text lives inside the `CompileError`'s
//! `String` message, alongside LWC's own text; a comparison on `CompileError::message` elsewhere is not a
//! diagnostic value and is not caught here.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const TYPE_TOKENS: [&str; 5] = ["OxcDiagnostic", "Diagnostics", "LabeledSpan", "oxc_diagnostics", "miette"];
const PRODUCERS: [&str; 10] =
    ["Parser", "SemanticBuilder", "Transformer", "Compiler", "CompilerInterface", "regular_expression", "parse_expression", "oxc_parser", "oxc_semantic", "oxc_transformer"];
const RETURN_TYPES: [&str; 4] = ["ParserReturn", "SemanticBuilderReturn", "TransformerReturn", "CompilerReturn"];
const DIAG_ACCESSORS: [&str; 6] = ["errors", "warnings", "into_vec", "has_errors", "has_warnings", "take_errors"];
/// parse.rs `pub fn` return types (tokens joined without spaces); `None` = `()`.
const EXPORTS: [&str; 5] = ["SourceType", "Result<Program<'a>,CompileError>", "Scoping", "Semantic<'a>", "Result<(),CompileError>"];
const BOUNDARY: [&str; 2] = ["first_error", "syntax_error"];
const BOUNDARY_IDENTS: &[&str] = &[
    "fn", "first_error", "syntax_error", "src", "str", "filename", "diagnostics", "oxc", "Diagnostics", "Option", "CompileError",
    "let", "d", "errors", "next", "Some", "label", "labels", "as_ref", "first", "else", "return", "new", "format", "at",
    "offset", "as", "usize", "message", "text", "line", "column", "lwc_shared", "babel_line_col", "u32", "frame", "code_frame",
    "None", "index", "LineIndex", "utf16_offset", "location", "DiagnosticLocation", "start", "length", "vec",
    "CompilerDiagnostic", "clone", "DiagnosticLevel", "Error", "into", "name", "assert", "is_char_boundary",
];
const BAD_PUNCT: [&str; 6] = ["==", "!=", "<=", ">=", "&&", "||"];

/// Closed D-0264 list of functions that may call parse / parse_with / parse_source / Parser::new.
/// Keyed by `path::fn` (path relative to lwc-js/src). New sites and orphan rows both fail.
/// Three remaining residuals expire at D-0265 in the D-0260 registry (owner js).
const PARSE_SITES: &[(&str, &str, &str)] = &[
    ("parse.rs::parse_source", "user-source parse (semantic check + abstract guard)", "P1-entry"),
    ("parse.rs::parse", "callee of parse_source and the intermediate parse entry; no semantic check", "P1-entry"),
    ("parse.rs::parse_with", "the single Parser::new site", "P1-entry"),
    ("lib.rs::transform_inner", "one parse of span-edited source immediately before print", "P1-print"),
    ("lib.rs::pass_with", "re-parse of span-edited source between class-features edit passes", "R-span-edit"),
    ("transforms/unforgeables.rs::run", "parse_with of printed JS (preserve_parens: false) under LWS", "R-unforgeables-lws"),
    ("transforms/lower.rs::async_functions", "parse of printed JS to reprint after unforgeables", "R-async-reprint"),
];

/// Functions that may search/replace the text `babelHelpers.` (enclosing fn, not a variable name).
const BABELHELPERS_FIND: &[(&str, &str, &str)] = &[];

#[derive(Debug, Clone, PartialEq)]
enum Tok { Ident(String), Punct(String), Lit }

/// Rust tokens (idents incl. lifetimes, punctuation, literals collapsed), comments dropped, with 1-based lines.
fn tokens(src: &str) -> Vec<(Tok, usize)> {
    let b: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut out) = (0, 1, Vec::new());
    let nl = |s: &[char]| s.iter().filter(|c| **c == '\n').count();
    while i < b.len() {
        let c = b[i];
        if c == '\n' { line += 1; i += 1; continue; }
        if c.is_whitespace() { i += 1; continue; }
        if c == '/' && b.get(i + 1) == Some(&'/') { while i < b.len() && b[i] != '\n' { i += 1; } continue; }
        if c == '/' && b.get(i + 1) == Some(&'*') {
            let (start, mut depth) = (i, 0);
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') { depth += 1; i += 2; }
                else if b[i] == '*' && b.get(i + 1) == Some(&'/') { depth -= 1; i += 2; if depth == 0 { break; } }
                else { i += 1; }
            }
            line += nl(&b[start..i.min(b.len())]);
            continue;
        }
        // raw (byte) strings: r"..", r#".."#, br#".."#
        let raw_at = if c == 'r' { Some(i + 1) } else if c == 'b' && b.get(i + 1) == Some(&'r') { Some(i + 2) } else { None };
        if let Some(mut j) = raw_at {
            let mut hashes = 0;
            while b.get(j) == Some(&'#') { hashes += 1; j += 1; }
            if b.get(j) == Some(&'"') {
                let start = i;
                j += 1;
                loop {
                    assert!(j < b.len(), "unterminated raw string at line {line}");
                    if b[j] == '"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&'#')) { j += 1 + hashes; break; }
                    j += 1;
                }
                line += nl(&b[start..j]);
                out.push((Tok::Lit, line));
                i = j;
                continue;
            }
        }
        if c == '"' || (c == 'b' && b.get(i + 1) == Some(&'"')) {
            let start = i;
            i += if c == 'b' { 2 } else { 1 };
            while i < b.len() && b[i] != '"' { if b[i] == '\\' { i += 1; } i += 1; }
            i += 1;
            line += nl(&b[start..i.min(b.len())]);
            out.push((Tok::Lit, line));
            continue;
        }
        if c == '\'' {
            // char literal ('x', '\n', '\u{..}') vs lifetime ('a)
            if b.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < b.len() && b[i] != '\'' { i += 1; }
                i += 1;
                out.push((Tok::Lit, line));
                continue;
            }
            if b.get(i + 2) == Some(&'\'') { i += 3; out.push((Tok::Lit, line)); continue; }
            let s = i;
            i += 1;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') { i += 1; }
            out.push((Tok::Ident(b[s..i].iter().collect()), line));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') { i += 1; }
            out.push((Tok::Ident(b[s..i].iter().collect()), line));
            continue;
        }
        if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || (b[i] == '.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()))) { i += 1; }
            out.push((Tok::Lit, line));
            continue;
        }
        let two: String = b[i..(i + 2).min(b.len())].iter().collect();
        if ["::", "==", "!=", "<=", ">=", "&&", "||", "->", "=>"].contains(&two.as_str()) { out.push((Tok::Punct(two), line)); i += 2; continue; }
        out.push((Tok::Punct(c.to_string()), line));
        i += 1;
    }
    out
}

fn is_ident(t: &Tok, s: &str) -> bool { matches!(t, Tok::Ident(x) if x == s) }
fn is_punct(t: &Tok, s: &str) -> bool { matches!(t, Tok::Punct(x) if x == s) }

/// Token index ranges `[fn, closing brace]` of each `fn <name>` in `toks`.
fn fn_ranges(toks: &[(Tok, usize)], name: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    for i in 0..toks.len().saturating_sub(1) {
        if !(is_ident(&toks[i].0, "fn") && is_ident(&toks[i + 1].0, name)) { continue; }
        let open = (i..toks.len()).find(|&j| is_punct(&toks[j].0, "{")).expect("fn body");
        let mut depth = 0;
        let close = (open..toks.len()).find(|&j| {
            if is_punct(&toks[j].0, "{") { depth += 1; }
            if is_punct(&toks[j].0, "}") { depth -= 1; }
            depth == 0
        }).expect("fn body closes");
        out.push((i, close));
    }
    out
}

/// Gate over `files` (path relative to src -> text). Returns violations.
fn gate(files: &BTreeMap<String, String>) -> Vec<String> {
    let mut bad = vec![];
    let mut boundary_fns = 0;
    for (path, text) in files {
        let toks = tokens(text);
        let is_parse = path == "parse.rs";
        let regions: Vec<(&str, (usize, usize))> =
            if is_parse { BOUNDARY.iter().flat_map(|n| fn_ranges(&toks, n).into_iter().map(move |r| (*n, r))).collect() } else { vec![] };
        if is_parse { boundary_fns += regions.len(); }
        let parse_source_fns = if is_parse { fn_ranges(&toks, "parse_source") } else { vec![] };
        for n in BOUNDARY {
            if !is_parse && !fn_ranges(&toks, n).is_empty() { bad.push(format!("{path}: defines `{n}` outside parse.rs (one conversion boundary)")); }
        }
        let region_of = |i: usize| regions.iter().find(|(_, (a, b))| (*a..=*b).contains(&i)).map(|(n, _)| *n);
        let mut v = |line: usize, why: String| bad.push(format!("{path}:{line}: {why}"));
        // first_error(..) argument spans in parse.rs (call sites, not the definition)
        let mut arg_spans = vec![];
        if is_parse {
            for i in 1..toks.len().saturating_sub(1) {
                if is_ident(&toks[i].0, "first_error") && !is_ident(&toks[i - 1].0, "fn") && is_punct(&toks[i + 1].0, "(") {
                    let mut depth = 0;
                    let close = (i + 1..toks.len()).find(|&j| {
                        if is_punct(&toks[j].0, "(") { depth += 1; }
                        if is_punct(&toks[j].0, ")") { depth -= 1; }
                        depth == 0
                    }).unwrap_or(toks.len() - 1);
                    arg_spans.push((i + 1, close));
                }
            }
        }
        for (i, (t, line)) in toks.iter().enumerate() {
            let region = region_of(i);
            let in_first_error = region == Some("first_error");
            let next = toks.get(i + 1).map(|x| &x.0);
            let prev = if i > 0 { Some(&toks[i - 1].0) } else { None };
            if let Tok::Ident(id) = t {
                let id = id.as_str();
                // R1
                let oxc_diag_path = id == "diagnostics" && i >= 2 && is_punct(&toks[i - 1].0, "::") && is_ident(&toks[i - 2].0, "oxc");
                if (TYPE_TOKENS.contains(&id) || oxc_diag_path) && !in_first_error {
                    v(*line, format!("R1: `{}` named outside parse.rs::first_error", if oxc_diag_path { "oxc::diagnostics" } else { id }));
                }
                // R2
                if PRODUCERS.contains(&id) && !is_parse { v(*line, format!("R2: oxc diagnostic producer `{id}` outside parse.rs")); }
                if RETURN_TYPES.contains(&id) { v(*line, format!("R2: producer return type `{id}` named")); }
                // R3
                if is_parse && region.is_none() {
                    let dot = prev.is_some_and(|p| is_punct(p, "."));
                    if id == "diagnostics" && dot && !arg_spans.iter().any(|(a, b)| (*a..=*b).contains(&i)) {
                        v(*line, "R3: `.diagnostics` read outside a first_error(..) argument".into());
                    }
                    if DIAG_ACCESSORS.contains(&id) { v(*line, format!("R3: diagnostics accessor `{id}` outside first_error")); }
                    if id == "message" { v(*line, "R5: parse.rs names `message` outside the boundary pair".into()); }
                }
                // R5
                if let Some(r) = region.filter(|_| !BOUNDARY_IDENTS.contains(&id)) { v(*line, format!("R5: `{id}` in the boundary fn {r}")); }
                // R6
                if id == "include" && next.is_some_and(|n| is_punct(n, "!")) { v(*line, "R6: include!".into()); }
                if id == "path" && i >= 2 && is_punct(&toks[i - 1].0, "[") && is_punct(&toks[i - 2].0, "#") { v(*line, "R6: #[path]".into()); }
                // R7: the early-error check lives only in parse_source
                if id == "with_check_syntax_error" && !parse_source_fns.iter().any(|(a, b)| (*a..=*b).contains(&i)) {
                    v(*line, "R7: `with_check_syntax_error` only inside parse.rs::parse_source".into());
                }
            }
            if let (Tok::Punct(p), Some(r)) = (t, region) && BAD_PUNCT.contains(&p.as_str()) {
                v(*line, format!("R5: `{p}` in the boundary fn {r}"));
            }
            // R4
            if is_parse && is_ident(t, "pub") {
                if !next.is_some_and(|n| is_ident(n, "fn")) { v(*line, "R4: parse.rs exports a non-fn item".into()); continue; }
                let open = (i..toks.len()).find(|&j| is_punct(&toks[j].0, "{") || is_ident(&toks[j].0, "where")).unwrap_or(toks.len());
                let ret = (i..open).find(|&j| is_punct(&toks[j].0, "->"));
                if let Some(r) = ret {
                    let ty: String = toks[r + 1..open].iter().map(|(t, _)| match t { Tok::Ident(s) | Tok::Punct(s) => s.as_str(), Tok::Lit => "<lit>" }).collect();
                    if !EXPORTS.contains(&ty.as_str()) { v(*line, format!("R4: parse.rs pub fn returns `{ty}`, not on EXPORTS")); }
                }
            }
        }
    }
    if boundary_fns != BOUNDARY.len() { bad.push(format!("parse.rs must define first_error and syntax_error exactly once each ({boundary_fns} found)")); }
    bad.extend(parse_site_violations(files, PARSE_SITES));
    bad.extend(babelhelpers_find_violations(files, BABELHELPERS_FIND));
    bad
}


fn all_fn_ranges(toks: &[(Tok, usize)]) -> Vec<(String, usize, usize)> {
    let mut out = vec![];
    for i in 0..toks.len().saturating_sub(1) {
        if !is_ident(&toks[i].0, "fn") { continue; }
        let Tok::Ident(name) = &toks[i + 1].0 else { continue };
        let Some(open) = (i..toks.len()).find(|&j| is_punct(&toks[j].0, "{")) else { continue };
        let mut depth = 0;
        let Some(close) = (open..toks.len()).find(|&j| {
            if is_punct(&toks[j].0, "{") { depth += 1; }
            if is_punct(&toks[j].0, "}") { depth -= 1; }
            depth == 0
        }) else { continue };
        out.push((name.clone(), i, close));
    }
    out
}

fn enclosing<'a>(fns: &'a [(String, usize, usize)], i: usize) -> Option<&'a str> {
    fns.iter()
        .filter(|(_, a, b)| *a <= i && i <= *b)
        .min_by_key(|(_, a, b)| b - a)
        .map(|(n, _, _)| n.as_str())
}

const PARSE_CALLEES: [&str; 3] = ["parse", "parse_with", "parse_source"];

fn use_aliases(toks: &[(Tok, usize)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < toks.len() {
        if !is_ident(&toks[i].0, "use") { i += 1; continue; }
        let mut depth = 0;
        let mut j = i + 1;
        while j < toks.len() {
            if is_punct(&toks[j].0, "{") { depth += 1; }
            if is_punct(&toks[j].0, "}") { depth -= 1; }
            if depth == 0 && is_punct(&toks[j].0, ";") { break; }
            j += 1;
        }
        let mut last: Option<&str> = None;
        for k in i..=j.min(toks.len() - 1) {
            match &toks[k].0 {
                Tok::Ident(id) if id == "as" => {
                    if let Some(Tok::Ident(alias)) = toks.get(k + 1).map(|t| &t.0) {
                        if let Some(seg) = last {
                            if PARSE_CALLEES.contains(&seg) || seg == "Parser" {
                                map.insert(alias.clone(), seg.to_string());
                            }
                        }
                    }
                    last = None;
                }
                Tok::Ident(id) if !matches!(id.as_str(), "use" | "crate" | "super" | "self" | "as") => {
                    last = Some(id.as_str());
                }
                Tok::Punct(p) if matches!(p.as_str(), "," | "{" | "}" | ";") => last = None,
                _ => {}
            }
        }
        i = j + 1;
    }
    map
}

/// `Some(canonical)` if `toks[i]` is a parse / parse_with / parse_source call or `Parser::new`.
fn parse_callee(toks: &[(Tok, usize)], i: usize, aliases: &HashMap<String, String>) -> Option<String> {
    let Tok::Ident(id) = &toks[i].0 else { return None };
    let name = aliases.get(id).map(String::as_str).unwrap_or(id.as_str());
    if i > 0 && is_ident(&toks[i - 1].0, "fn") { return None; }
    if name == "Parser" {
        if toks.get(i + 1).is_some_and(|t| is_punct(&t.0, "::"))
            && toks.get(i + 2).is_some_and(|t| is_ident(&t.0, "new"))
            && toks.get(i + 3).is_some_and(|t| is_punct(&t.0, "("))
        {
            return Some("Parser::new".into());
        }
        return None;
    }
    if !PARSE_CALLEES.contains(&name) { return None; }
    if i > 0 && is_punct(&toks[i - 1].0, ".") { return None; }
    if !toks.get(i + 1).is_some_and(|t| is_punct(&t.0, "(")) { return None; }
    Some(name.into())
}

fn parse_site_violations(files: &BTreeMap<String, String>, allowed: &[(&str, &str, &str)]) -> Vec<String> {
    let allowed_keys: BTreeSet<&str> = allowed.iter().map(|a| a.0).collect();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut bad = Vec::new();
    for (path, src) in files {
        let toks = tokens(src);
        let fns = all_fn_ranges(&toks);
        let aliases = use_aliases(&toks);
        for i in 0..toks.len() {
            let Some(callee) = parse_callee(&toks, i, &aliases) else { continue };
            let line = toks[i].1;
            let Some(fname) = enclosing(&fns, i) else {
                bad.push(format!("{path}:{line}: R8: `{callee}` at module level (D-0262 / D-0264)"));
                continue;
            };
            let key = format!("{path}::{fname}");
            found.insert(key.clone());
            if !allowed_keys.contains(key.as_str()) {
                bad.push(format!("{path}:{line}: R8: `{callee}` in `{key}` is not on the D-0264 PARSE_SITES list"));
            }
        }
    }
    for (key, _, tag) in allowed {
        if !found.contains(*key) {
            bad.push(format!("R8: orphan PARSE_SITES row `{key}` ({tag})"));
        }
    }
    bad
}

fn fn_raw_src(src: &str, toks: &[(Tok, usize)], start: usize, end: usize) -> String {
    let lo = toks[start].1;
    let hi = toks[end].1;
    src.lines().skip(lo.saturating_sub(1)).take(hi.saturating_sub(lo) + 1).collect::<Vec<_>>().join("\n")
}

fn babelhelpers_find_violations(files: &BTreeMap<String, String>, allowed: &[(&str, &str, &str)]) -> Vec<String> {
    let allowed_keys: BTreeSet<&str> = allowed.iter().map(|a| a.0).collect();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut bad = Vec::new();
    for (path, src) in files {
        let toks = tokens(src);
        let fns = all_fn_ranges(&toks);
        for (name, a, b) in &fns {
            let raw = fn_raw_src(src, &toks, *a, *b);
            if !raw.contains("babelHelpers.") { continue; }
            let searches = raw.contains(".find(")
                || raw.contains(".rfind(")
                || raw.contains(".match_indices(")
                || raw.contains(".replace(")
                || raw.contains(".replacen(")
                || raw.contains(".replace_range(");
            if !searches { continue; }
            let key = format!("{path}::{name}");
            found.insert(key.clone());
            if !allowed_keys.contains(key.as_str()) {
                bad.push(format!("{path}:{}: R8b: find/replace of `babelHelpers.` in `{key}` is not on BABELHELPERS_FIND (D-0262 / D-0264)", toks[*a].1));
            }
        }
    }
    for (key, _, tag) in allowed {
        if !found.contains(*key) {
            bad.push(format!("R8b: orphan BABELHELPERS_FIND row `{key}` ({tag})"));
        }
    }
    bad
}

fn src_dir() -> std::path::PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("src") }

fn real_files() -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let p = e.unwrap().path();
            if p.is_dir() { walk(root, &p, out); continue; }
            if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("unreadable {}: {e}", p.display()));
                out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into_owned(), text);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&src_dir(), &src_dir(), &mut out);
    out
}

#[test]
fn lwc_js_src_confines_oxc_diagnostics_to_first_error() {
    let files = real_files();
    assert!(files.len() >= 15 && files.contains_key("parse.rs") && files.contains_key("transforms/lower.rs"), "walked {:?}", files.keys());
    let bad = gate(&files);
    assert!(bad.is_empty(), "oxc diagnostics outside parse.rs::first_error (D-0248, AB-L0063):\n{}", bad.join("\n"));
}

/// Arch's misses of the old line-grep gate (AB-L0063 (5)), each planted where a diagnostic could be at hand.
const SHAPES: [(&str, &str); 7] = [
    ("matches!", "if matches!(d.message.as_ref(), \"Unexpected token\") { return None; }"),
    (".message().starts_with", "if d.message().starts_with(\"Expected\") { return None; }"),
    ("alias m.contains", "let m = &d.message;\n    if m.contains(\"abstract\") { return None; }"),
    ("multi-line call", "if d\n        .message\n        .contains(\n            \"x\",\n        ) { return None; }"),
    (".to_lowercase().contains", "if d.message.to_lowercase().contains(\"x\") { return None; }"),
    ("format!(..).contains", "if format!(\"{}\", d.message).contains(\"x\") { return None; }"),
    ("d.to_string().contains", "if d.to_string().contains(\"x\") { return None; }"),
];

/// Violations the plant adds (the real tree is clean, asserted, so every one is the plant's).
fn planted(files: &BTreeMap<String, String>, path: &str, anchor: &str, code: &str) -> Vec<String> {
    assert!(gate(files).is_empty(), "baseline must be clean");
    gate(&plant(files, path, anchor, code))
}

fn plant(files: &BTreeMap<String, String>, path: &str, anchor: &str, code: &str) -> BTreeMap<String, String> {
    let mut f = files.clone();
    let text = f.get_mut(path).unwrap_or_else(|| panic!("no {path}"));
    let at = text.find(anchor).unwrap_or_else(|| panic!("anchor {anchor:?} not in {path}")) + anchor.len();
    text.insert_str(at, &format!("\n    {code}\n"));
    f
}

#[test]
fn every_planted_message_match_fails_the_gate() {
    let files = real_files();
    // (where, anchor, how a `d` is at hand there)
    let sites: [(&str, &str, &str); 4] = [
        // inside the boundary fn itself, where `d` is the diagnostic
        ("parse.rs", "let d = diagnostics.errors().next()?;", ""),
        // elsewhere in parse.rs, reading the producer's diagnostics directly
        ("parse.rs", "let ret = Parser::new(alloc, src, source_type()).with_options(options).parse();", "let d = ret.diagnostics.errors().next().unwrap();\n    "),
        // another module, running its own producer without naming any diagnostic type
        ("transforms/lower.rs", "let alloc = Allocator::default();", "let ret = oxc::parser::Parser::new(&alloc, code, crate::parse::source_type()).parse();\n    let d = ret.diagnostics.errors().next().unwrap();\n    "),
        // another module, SemanticBuilder errors (no Parser, no type named)
        ("transforms/decorators.rs", "use oxc::semantic::NodeId;", "let d = oxc::semantic::SemanticBuilder::new().with_check_syntax_error(true).build(program).diagnostics.errors().next().unwrap();\n    "),
    ];
    let mut report = vec![];
    for (path, anchor, acquire) in sites {
        for (name, code) in SHAPES {
            let bad = planted(&files, path, anchor, &format!("{acquire}{code}"));
            assert!(!bad.is_empty(), "plant {name:?} in {path} passed the gate");
            report.push(format!("{path} / {name}: {}", bad.join(" | ")));
        }
    }
    // after an early #[cfg(test)]: the gate does not stop there
    for path in ["parse.rs", "lib.rs"] {
        let mut f = files.clone();
        let text = f.get_mut(path).unwrap();
        text.insert_str(0, "#[cfg(test)]\nmod early {}\n");
        text.push_str("\n#[cfg(test)]\nmod late { fn f(d: &oxc::diagnostics::OxcDiagnostic) -> bool { d.message.contains(\"x\") } }\n");
        assert!(gate(&files).is_empty(), "baseline must be clean");
        let bad = gate(&f);
        assert!(!bad.is_empty(), "plant after early #[cfg(test)] in {path} passed");
        report.push(format!("{path} / after #[cfg(test)]: {}", bad.join(" | ")));
    }
    eprintln!("{}", report.join("\n"));
}

#[test]
fn bypasses_that_never_name_oxc_diagnostic_fail_the_gate() {
    let files = real_files();
    for (name, path, anchor, code) in [
        ("parse.rs leaks diagnostics as an impl Iterator", "parse.rs", "Semantic<'a> {\n    SemanticBuilder::new().with_build_nodes(true).build(program).semantic\n}",
            "pub fn leak<'a>(alloc: &'a Allocator, src: &'a str) -> impl Iterator<Item = String> + 'a { Vec::<String>::new().into_iter() }"),
        ("parse.rs returns the producer result", "parse.rs", "Semantic<'a> {\n    SemanticBuilder::new().with_build_nodes(true).build(program).semantic\n}",
            "pub fn raw<'a>(alloc: &'a Allocator, src: &'a str) -> oxc::parser::ParserReturn<'a> { Parser::new(alloc, src, source_type()).parse() }"),
        ("parse.rs exports a struct", "parse.rs", "Semantic<'a> {\n    SemanticBuilder::new().with_build_nodes(true).build(program).semantic\n}", "pub struct Leak(pub Vec<String>);"),
        ("filter the converted text in parse.rs", "parse.rs", "let program = parse(alloc, src, filename)?;",
            "if first_error(src, filename, &SemanticBuilder::new().build(&program).diagnostics).is_some_and(|e| e.message.contains(\"x\")) { }"),
        ("module alias", "lib.rs", "pub mod parse;", "use oxc::diagnostics as dx;"),
        ("glob import + Transformer", "transforms/lower.rs", "use oxc_sourcemap::SourceMap;", "use oxc::transformer::*; fn t() { let _ = Transformer::new; }"),
        ("include!", "lib.rs", "pub mod parse;", "include!(\"../../lwc-shared/src/x.rs\");"),
        ("#[path]", "lib.rs", "pub mod parse;", "#[path = \"../../elsewhere.rs\"] mod elsewhere;"),
        ("boundary fn branches on the text", "parse.rs", "let d = diagnostics.errors().next()?;", "let n = d.message.len(); assert!(n > 0);"),
        ("second conversion fn", "lib.rs", "pub mod parse;", "fn first_error() {}"),
        ("semantic check on intermediate parse()", "parse.rs", "pub fn parse<'a>(alloc: &'a Allocator, src: &'a str, filename: &str) -> Result<Program<'a>, CompileError> {
    parse_with(alloc, src, filename, ParseOptions::default())
}",
            "let _ = SemanticBuilder::new().with_check_syntax_error(true);"),
    ] {
        let bad = planted(&files, path, anchor, code);
        assert!(!bad.is_empty(), "{name}: passed the gate");
        eprintln!("{name}: {}", bad.join(" | "));
    }
}

#[test]
fn tokenizer_strips_comments_and_literals() {
    let t = tokens("a // OxcDiagnostic\n/* OxcDiagnostic /* nested */ */ \"OxcDiagnostic\" r#\"OxcDiagnostic\"# 'x' '\\n' b\"Parser\" fn f<'a>() {}");
    let ids: Vec<&str> = t.iter().filter_map(|(t, _)| if let Tok::Ident(s) = t { Some(s.as_str()) } else { None }).collect();
    assert_eq!(ids, ["a", "fn", "f", "'a"]);
    assert_eq!(t.last().unwrap().1, 2, "line count across comments");
}

#[test]
fn planted_parse_outside_closed_list_fails_the_gate() {
    let files = real_files();
    assert!(gate(&files).is_empty(), "baseline must be clean: {}", gate(&files).join("\n"));
    for (name, path, anchor, code) in [
        (
            "renamed variable in a non-listed helper",
            "transforms/lower.rs",
            "if !src.contains(\"...\") { return Ok(()); }",
            "let codegen_js = src; let _ = crate::parse::parse(&oxc::allocator::Allocator::default(), codegen_js, filename);",
        ),
        (
            "new helper function",
            "lib.rs",
            "pub mod parse;",
            "fn extra() { let _ = crate::parse::parse(&alloc, src, filename); }",
        ),
        (
            "use-as-p alias in a new function",
            "lib.rs",
            "pub mod parse;",
            "use crate::parse::parse as p; fn via_alias() { let _ = p(&alloc, src, filename); }",
        ),
        (
            "Parser as Px::new inside parse.rs",
            "parse.rs",
            "/// Scope tree of a parsed program (no syntax checks).",
            "use oxc::parser::Parser as Px; fn raw() { let _ = Px::new(&alloc, src, source_type()); }",
        ),
        (
            "Parser as Px::new outside parse.rs",
            "lib.rs",
            "pub mod parse;",
            "use oxc::parser::Parser as Px; fn raw() { let _ = Px::new(&alloc, src, Default::default()); }",
        ),
    ] {
        let bad = planted(&files, path, anchor, code);
        assert!(!bad.is_empty(), "{name}: passed the gate");
        assert!(bad.iter().any(|b| b.contains("R8")), "{name}: expected R8, got {}", bad.join(" | "));
        eprintln!("{name}: {}", bad.join(" | "));
    }

    let mut extra: Vec<(&str, &str, &str)> = PARSE_SITES.to_vec();
    extra.push(("no/such.rs::ghost", "orphaned row", "R-ghost"));
    let orphan = parse_site_violations(&files, &extra);
    assert!(orphan.iter().any(|b| b.contains("orphan") && b.contains("no/such.rs::ghost")),
        "orphaned allowlist row must fail, got {}", orphan.join(" | "));
    eprintln!("orphan row: {}", orphan.iter().filter(|b| b.contains("orphan")).cloned().collect::<Vec<_>>().join(" | "));
}

#[test]
fn planted_babelhelpers_text_replace_on_printed_output_fails_the_gate() {
    let files = real_files();
    assert!(gate(&files).is_empty(), "baseline must be clean: {}", gate(&files).join("\n"));
    for (name, path, anchor, code) in [
        (
            "replace on printed output in post_codegen_edits",
            "helpers.rs",
            "crate::transforms::version_stamp::run(program, &mut stamps);",
            "let _ = output.replace(\"babelHelpers.defineProperty\", \"_defineProperty\");",
        ),
        (
            "find on printed output in transform_inner",
            "lib.rs",
            "let first = helpers::localize(&alloc, &mut program);",
            "let _ = printed.find(\"babelHelpers.\");",
        ),
        (
            "match_indices scan of printed JS in a new helper",
            "helpers.rs",
            "pub fn remap_edits",
            "fn scan(output: &str) { let _ = output.match_indices(\"babelHelpers.\"); }",
        ),
    ] {
        let bad = planted(&files, path, anchor, code);
        assert!(!bad.is_empty(), "{name}: passed the gate");
        assert!(bad.iter().any(|b| b.contains("R8b")), "{name}: expected R8b, got {}", bad.join(" | "));
        eprintln!("{name}: {}", bad.join(" | "));
    }

    let extra: &[(&str, &str, &str)] = &[("helpers.rs::post_codegen_edits", "stale printed-output rename", "R-babelhelpers-print")];
    let orphan = babelhelpers_find_violations(&files, extra);
    assert!(orphan.iter().any(|b| b.contains("orphan") && b.contains("helpers.rs::post_codegen_edits")),
        "orphaned BABELHELPERS_FIND row must fail, got {}", orphan.join(" | "));
    eprintln!("orphan row: {}", orphan.iter().filter(|b| b.contains("orphan")).cloned().collect::<Vec<_>>().join(" | "));
}

#[test]
fn async_predicate_lives_in_one_fn_called_from_one_place() {
    let files = real_files();
    let mut defs = Vec::new();
    let mut callers = Vec::new();
    for (path, src) in &files {
        let toks = tokens(src);
        let fns = all_fn_ranges(&toks);
        for (name, a, b) in &fns {
            let raw = fn_raw_src(src, &toks, *a, *b);
            let key = format!("{path}::{name}");
            if raw.contains(".contains(\"async\")") {
                defs.push(key.clone());
            }
            if name != "source_has_async" && raw.contains("source_has_async(") {
                callers.push(key);
            }
        }
    }
    assert_eq!(defs, vec!["transforms/lower.rs::source_has_async".to_string()],
        "async cheap-exit predicate must live in exactly one fn, got {defs:?}");
    assert_eq!(callers, vec!["transforms/lower.rs::async_functions".to_string()],
        "source_has_async must be called from exactly one place, got {callers:?}");
}
