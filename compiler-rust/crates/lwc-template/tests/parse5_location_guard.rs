//! AB-X0014 guard: a parse5 location read under `src/parser/` returns `Located<T>` and raises TS's internal
//! parsing error at the read (parser/index.ts parseRoot / parseTextNode / parseComment / parseAttributes /
//! elementSourceLocation). An `.unwrap()` / `.expect(..)` on one turns that error back into a panic.
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::path::{Path, PathBuf};

/// Reads of a parse5 location, raw (`source_code_location*`, `start_tag`, `end_tag`, `attrs.get`) or
/// through the parser's readers.
const READS: &[&str] = &[
    "source_code_location", "start_tag", "end_tag", ".attrs.get(", "node_loc(", "required_loc(", "attr_loc(",
    "attr_loc_in(", "raw_attr(", "element_location(", "elem_parse5_loc(",
];
const PANICS: &[&str] = &[".unwrap()", ".expect(", ".unwrap_unchecked("];

/// Every read whose expression (up to the next `;` or `}`, across lines) panics on a missing location.
/// `rel` is the path under `src/parser/`.
fn violations(rel: &str, src: &str) -> Vec<String> {
    // Code only (line comments dropped), whitespace removed; `line_of[i]` is the source line of byte `i`.
    let (mut code, mut line_of) = (String::new(), Vec::new());
    for (i, line) in src.lines().enumerate() {
        let line = line.find("//").map_or(line, |c| &line[..c]);
        for ch in line.chars().filter(|c| !c.is_whitespace()) {
            code.push(ch);
            line_of.extend(std::iter::repeat_n(i + 1, ch.len_utf8()));
        }
    }
    let mut hits = Vec::new();
    for read in READS {
        for (at, _) in code.match_indices(read) {
            let rest = &code[at..];
            let expr = &rest[..rest.find([';', '}']).unwrap_or(rest.len())];
            if PANICS.iter().any(|p| expr.contains(p)) {
                let hit = format!("{rel}:{}: {expr}", line_of[at]);
                if !hits.contains(&hit) { hits.push(hit); }
            }
        }
    }
    hits
}

fn scan_files(root: &Path, files: &[PathBuf], hits: &mut Vec<String>) {
    for p in files {
        if p.extension().is_some_and(|e| e == "rs") {
            let rel = rel_slash(root, p);
            hits.extend(violations(&rel, &std::fs::read_to_string(p).unwrap()));
        }
    }
}

fn parser_violations_files(root: &Path, files: &[PathBuf]) -> Vec<String> {
    let mut hits = Vec::new();
    scan_files(root, files, &mut hits);
    hits.sort();
    hits
}

#[test]
fn no_panicking_parse5_location_read_in_parser() {
    let repo = workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")));
    let parser = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parser");
    let files = tracked_files_filtered(&repo, &["crates/lwc-template/src/parser"], &["rs"]);
    let hits = parser_violations_files(&parser, &files);
    assert!(hits.is_empty(), "parse5 location reads return `Located<T>`; propagate with `?` instead of panicking:\n{}", hits.join("\n"));
}

/// The scan catches planted copies (one line, a chain split over lines, a nested file) and passes the
/// `?` form, comments, and unwraps unrelated to locations.
#[test]
fn guard_catches_planted_violations() {
    let root = std::env::temp_dir().join(format!("lwc-parse5-loc-guard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("utils")).unwrap();
    std::fs::write(root.join("ok.rs"), concat!(
        "fn a(n: &NodeRef, d: &Ctx) -> Located<()> { let l = attr_loc(n, \"key\", d)?; Ok(()) }\n",
        "// attr_loc(n, name, d).unwrap() is what the guard bans\n",
        "fn b(v: Option<u8>) -> u8 { v.unwrap() }\n",
        "fn c(l: &parse5::ElementLocation) -> bool { l.end_tag.is_none() }\n",
    )).unwrap();
    assert_eq!(parser_violations_files(&root, &[root.join("ok.rs")]), Vec::<String>::new());
    std::fs::write(root.join("bad.rs"), concat!(
        "fn a(n: &NodeRef) -> SourceLocation { loc(&n.borrow().source_code_location().unwrap().location) }\n",
        "fn b(l: &parse5::ElementLocation) -> SourceLocation {\n",
        "    loc(l.start_tag\n",
        "        .as_ref()\n",
        "        .expect(\"located\"))\n",
        "}\n",
    )).unwrap();
    std::fs::write(root.join("utils/deep.rs"), "fn c(n: &NodeRef, d: &Ctx) -> RawAttr { raw_attr(n, a, d).unwrap() }\n").unwrap();
    let hits = parser_violations_files(&root, &[root.join("ok.rs"), root.join("bad.rs"), root.join("utils/deep.rs")]);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(hits.len(), 3, "{hits:#?}");
    assert!(hits[0].starts_with("bad.rs:1: source_code_location().unwrap()"), "{hits:#?}");
    assert!(hits[1].starts_with("bad.rs:3: start_tag.as_ref().expect("), "{hits:#?}");
    assert!(hits[2].starts_with("utils/deep.rs:1: raw_attr(n,a,d).unwrap()"), "{hits:#?}");
}
