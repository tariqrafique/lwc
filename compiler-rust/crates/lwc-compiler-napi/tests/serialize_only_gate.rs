//! AB-R003: lwc-compiler-napi only serializes errors. The facades in lwc-compiler build them
//! (code, level, url, aggregate shape, options errors); no origin sniffing, url table, aggregate switch
//! or error construction may return here.
use regex::Regex;
use std::path::Path;

/// Each banned pattern, with what it catches. Naming the types is fine (signatures, `match` arms);
/// building a value of them is not.
const BANNED: &[(&str, &str)] = &[
    (r"level_url|error_url|Origin::|AGGREGATE_", "error-shape derivation"),
    // `CompilerError { .. }`, `CompilerAggregateError { .. }`, `UnnormalizedError { .. }` struct literals.
    (r"\b(CompilerError|CompilerAggregateError|UnnormalizedError)\s*\{", "error struct literal"),
    // `CompilerError::new(..)`, `::generate(..)`, `::from_diagnostic(..)`, `::from(..)`, any associated ctor.
    (r"\b(CompilerError|CompilerAggregateError|UnnormalizedError|TransformError)\s*::\s*[a-z_]\w*\s*\(", "error constructor call"),
    // `TransformError::Compiler(..)` etc. as an expression; a `match` / `let` pattern binding one name is exempt below.
    (r"\bTransformError\s*::\s*(Compiler|Aggregate|Unnormalized)\s*\(", "TransformError variant construction"),
    (r"\bnormalize_to_compiler_error\b", "normalization"),
    // `.into()` / `From` into an error type: `-> TransformError` built by conversion.
    (r"impl\s+From<[^>]*>\s+for\s+(CompilerError|CompilerAggregateError|UnnormalizedError|TransformError)\b", "error conversion impl"),
];

/// `TransformError::Compiler(e) =>` / `| ..` / `let TransformError::Aggregate(a) = ..`: a pattern, not a construction.
fn is_pattern(line: &str, at: usize) -> bool {
    let rest = &line[at..];
    let pat = Regex::new(r"^TransformError\s*::\s*\w+\s*\(\s*\w+\s*\)\s*(=>|\||if\b|=[^=>])").unwrap();
    pat.is_match(rest)
}

fn hits(text: &str) -> Vec<(usize, &'static str)> {
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for (re, what) in BANNED {
            let re = Regex::new(re).unwrap();
            if re.find_iter(code).any(|m| !(what.starts_with("TransformError variant") && is_pattern(code, m.start()))) {
                out.push((i + 1, *what));
            }
        }
    }
    out
}

/// Every `.rs` file under `dir`, nested modules included, with each banned line as `path:line: what: text`.
fn scan(dir: &Path) -> Vec<String> {
    let mut found = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                for (n, what) in hits(&text) {
                    let rel = p.strip_prefix(dir).unwrap().display();
                    found.push(format!("{rel}:{n}: {what}: {}", text.lines().nth(n - 1).unwrap()));
                }
            }
        }
    }
    found.sort();
    found
}

#[test]
fn napi_src_derives_no_error_shape() {
    let found = scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(found.is_empty(), "error built or shaped in napi (belongs in the lwc-compiler facades):\n{}", found.join("\n"));
}

/// The walk is recursive: a banned construction in a nested module file is caught.
#[test]
fn nested_module_files_are_scanned() {
    let root = std::env::temp_dir().join(format!("serialize-only-gate-{}", std::process::id()));
    let nested = root.join("errors").join("inner");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join("lib.rs"), "mod errors;\n").unwrap();
    std::fs::write(nested.join("deep.rs"), "fn f() -> TransformError {\n    TransformError::Compiler(CompilerError::from_diagnostic(d, None))\n}\n").unwrap();
    let found = scan(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.starts_with(&format!("errors{0}inner{0}deep.rs:2: ", std::path::MAIN_SEPARATOR))), "{found:?}");
}

/// The gate catches each construction form and lets signatures and `match` arms through.
#[test]
fn gate_patterns() {
    let caught = [
        "let e = CompilerError { level: d.level, ..x };",
        "TransformError::Compiler(CompilerError::from_diagnostic(d, None))",
        "CompilerError::generate(info, &[], \"f\")",
        "CompilerError::new(\"m\")",
        "CompilerAggregateError { errors, message }",
        "Err(TransformError::Unnormalized(e.into()))",
        "TransformError::Aggregate(a)",
        "TransformError::from(e)",
        "normalize_to_compiler_error(&info, e, None)",
        "UnnormalizedError { message, code: None }",
        "impl From<CompileError> for TransformError {",
        "let level = single_error::level_url(o, c, l);",
    ];
    for l in caught {
        assert!(!hits(l).is_empty(), "not caught: {l}");
    }
    let allowed = [
        "fn serialize(e: TransformError) -> JsCompileError {",
        "        TransformError::Compiler(e) => compiler_error(e),",
        "        TransformError::Aggregate(a) => aggregate(a),",
        "fn compiler_error(e: CompilerError) -> JsCompileError {",
        "fn aggregate(a: CompilerAggregateError) -> JsCompileError {",
        "    result(serialize(lwc_compiler::options_error(e)))",
        "use lwc_shared::{CompileError, CompilerAggregateError, CompilerError, TransformError, UnnormalizedError};",
        "    JsCompileError {",
        "// CompilerError { .. } in a comment",
    ];
    for l in allowed {
        assert!(hits(l).is_empty(), "false positive: {l} -> {:?}", hits(l));
    }
}
