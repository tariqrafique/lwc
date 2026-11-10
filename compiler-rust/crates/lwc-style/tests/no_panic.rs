//! No input panics. A JS throw must surface as `Err(CompileError)`: a Rust panic across napi is a
//! process abort. Sweeps `transform` (the facade napi calls; it wraps `transform_with_config`) under
//! all six style-diff configs over: upstream fixtures, `tests/parse-errors`, the JS-vs-Rust edge
//! probes, the style-diff corpus sources (LWC integration packages + `$STYLE_CORPUS`, when
//! present), every prefix of the small inputs (truncation = unclosed everything), and a
//! deterministic fragment fuzz.
use lwc_style::{transform, CompileOptions};
use repo_tracked::{rel_slash, tracked_files_filtered, workspace_root};
use std::{fs, panic, path::Path};

const PROBES: &[&str] = &[
    "a { color: red ; }", "a{};b{}", "a{} ; b{}", "a /* x */ b { c: d }", "a{color:red!important}", "a{color:red ! important}",
    "a{color:red /* c */ !IMPORTANT ;}", "color: red;", "@charset \"x\";", "@media screen{a{b:c}}", "@foo;", "@foo bar /*c*/ ;",
    "a{ *zoom: 1; _height: 1px }", "a{b:c/**/d}", "a{b:c, /**/d}", "a{--x: { a:b } ; c:d}", "a{--x:  y  }", "@media (x {}",
    "a{b:c}/*end*/", "@keyframes k { from{} } a{ animation: k 1s /* c */ }", "\u{feff}a{b:c}", "a{b:c;;}", "@media x{;}",
    "a{b:c}\n}", "@import \"x\"", "a\r\n{\r\n b : c ;\r\n}", "@media x { @foo }", "@a{b{c:d}};", "a{ b: c d; }", ":host{a:b};x{}",
    "a{b:url( x )}", "a{filter:progid:DXImage(x=1)}", "@page :first{margin:0}", "a{b:c d:e}",
    ":host{};", ":host{a:b};", ":host(.a){a:b};x{}", "@media x{:host{a:b};}", ":host-context(.a){};x{}", ":host{}:host{};;",
    ":dir(){}", ":dir(", "a:dir(ltr){}", "#id{}", "@import;", "@import '", "@import \"x\" screen;", "@import url(x);",
    "@keyframes{}", "@keyframes x{}", "a{animation:;}", "a{animation-name: x y z}", ":host(){}", "::slotted(){}", ":not(:host){}",
    "a{b:var(--x,)}", "\\", "a\\{}", "😀{b:c}", "\u{fffe}", "\r", "\r\n", "@", "@;", "{}", "};", ";", "/**/", "a{/**/}",
];

const FRAGMENTS: &[&str] = &[
    ":host", ":host(", ":host-context(", ":dir(", "ltr", "rtl", "::slotted(", ":not(", "#id", ".c", "a", " ", "\n", "\r\n", "{", "}",
    ";", ":", "(", ")", "[", "]", ",", "\"", "'", "/*", "*/", "\\", "@media x", "@import", "@keyframes k", "@supports (a:b)", "@",
    "--x", "b", "c", "!important", "url(", "progid", "*", "_", "animation:", "k", "1s", "😀", "é", "\u{feff}", "x-y", ">", "+", "~",
];

fn opts(i: usize) -> CompileOptions {
    // style-diff CONFIGS: default, scoped, dss, scoped+dss, erm, erm+scoped.
    let (scoped, dss, erm) = [(false, false, false), (true, false, false), (false, true, false), (true, true, false), (false, false, true), (true, false, true)][i];
    CompileOptions { name: "x".into(), namespace: "x".into(), scoped_styles: Some(scoped), disable_synthetic_shadow_support: dss, experimental_error_recovery_mode: erm, ..Default::default() }
}

fn css_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.is_dir() { if name != "node_modules" && name != "dist" && name != ".git" { css_files(&p, out) } }
        else if name.ends_with(".css") { if let Ok(s) = fs::read_to_string(&p) { out.push(s) } }
    }
}

fn sweep(inputs: &[String]) -> (usize, Vec<String>) {
    let mut panics = Vec::new();
    let mut runs = 0;
    for src in inputs {
        for i in 0..6 {
            runs += 1;
            let o = opts(i);
            if panic::catch_unwind(panic::AssertUnwindSafe(|| { let _ = transform(src, "x.css", &o); })).is_err() {
                panics.push(format!("config #{i}: {src:?}"));
            }
        }
    }
    (runs, panics)
}

#[test]
fn nothing_panics() {
    panic::set_hook(Box::new(|_| {}));
    let lwc = lwc_shared::oracle_root();
    let mut small: Vec<String> = PROBES.iter().map(|s| s.to_string()).collect();
    css_files(&lwc_shared::cargo_manifest_dir!().join("tests/parse-errors"), &mut small);
    let mut corpus = Vec::new();
    css_files(&Path::new(&lwc).join("packages/@lwc/style-compiler/src/__tests__/fixtures"), &mut corpus);
    for pkg in ["integration-wtr", "integration-wdio", "integration-types"] { css_files(&Path::new(&lwc).join("packages/@lwc").join(pkg), &mut corpus); }
    let slds = lwc_shared::style_corpus_root();
    for sub in ["lbc", "slds", "lbc-recipes"] { css_files(&Path::new(&slds).join(sub), &mut corpus); }

    let mut prefixes = Vec::new();
    for s in &small { for (i, _) in s.char_indices().skip(1) { prefixes.push(s[..i].to_string()); } }

    let mut fuzz = Vec::new();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
    for _ in 0..20_000 {
        let n = 1 + (next() % 12) as usize;
        fuzz.push((0..n).map(|_| FRAGMENTS[(next() % FRAGMENTS.len() as u64) as usize]).collect::<String>());
    }

    let mut all_panics = Vec::new();
    let mut total = 0;
    for (what, inputs) in [("probes+parse-errors", &small), ("corpus", &corpus), ("prefixes", &prefixes), ("fuzz", &fuzz)] {
        let (runs, p) = sweep(inputs);
        println!("{what}: {} inputs, {runs} runs, {} panics", inputs.len(), p.len());
        total += runs;
        all_panics.extend(p);
    }
    let _ = panic::take_hook();
    println!("total runs: {total}");
    assert!(all_panics.is_empty(), "{} panicking inputs:\n  {}", all_panics.len(), all_panics.iter().take(40).cloned().collect::<Vec<_>>().join("\n  "));
}

/// Every panic site left in non-test `src/` code, with the invariant that makes it unreachable from
/// input. A new `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!` fails this test
/// until it is converted to a `StyleError` or justified here (and in the review doc).
const AUDITED_PANIC_SITES: &[(&str, &str, &str)] = &[
    ("src/parser.rs", "fn current(&mut self) -> &mut Block { &mut self.stack.last_mut().expect(\"root frame\")", "stack starts with Root; end() refuses to pop at len 1"),
    ("src/parser.rs", "let block = &mut self.stack.last_mut().expect(\"root frame\")", "same Root-frame invariant"),
    ("src/parser.rs", "let Frame { node, block, .. } = self.stack.pop().expect(\"open frame\")", "guarded by `stack.len() == 1 => Err` on the line above"),
    ("src/parser.rs", "Open::Root => unreachable!(\"root is never popped\")", "only frames above Root are popped (len > 1 checked)"),
    ("src/parser.rs", "self.input.position(self.stack.last().expect(\"open frame\").start)", "inside `if self.stack.len() > 1`"),
    ("src/parser.rs", "let t = tokens.pop().expect(\"just pushed\")", "pops the token pushed immediately before"),
    ("src/parser.rs", "let t = tokens.pop().expect(\"non-empty\")", "loop guard `tokens.last().is_some_and(..)`"),
    ("src/parser.rs", "before += tokens.pop_front().expect(\"len > 1\")", "loop guard `tokens.len() > 1`"),
    ("src/parser.rs", "prop += tokens.pop_front().expect(\"front\")", "loop guard `tokens.front()` is Some"),
    ("src/parser.rs", "first_spaces.push(tokens.pop_front().expect(\"front\"))", "loop guard `tokens.front()` is Some"),
    ("src/parser.rs", "s = cache.pop().expect(\"j < len\")", "j <= cache.len()-1 holds as both shrink by one per step"),
    ("src/parser.rs", "spaces = tokens.pop().expect(\"non-empty\").into_text() + &spaces", "loop guard `tokens.last().is_some_and(..)` (raw spaces/after loops; 2 sites)"),
];

#[test]
fn panic_sites_are_audited() {
    let repo = workspace_root(&lwc_shared::cargo_manifest_dir!());
    let root = lwc_shared::cargo_manifest_dir!();
    let mut files = tracked_files_filtered(&repo, &["crates/lwc-style/src"], &["rs"]);
    files.sort();
    let pat = [".unwrap()", ".expect(", "panic!", "unreachable!", "todo!", "unimplemented!", "unwrap_unchecked"];
    let mut unaudited = Vec::new();
    let mut used = vec![0usize; AUDITED_PANIC_SITES.len()];
    for f in files {
        let rel = rel_slash(&root, &f);
        for (i, l) in std::fs::read_to_string(&f).unwrap().lines().enumerate() {
            if l.trim() == "#[cfg(test)]" { break }
            if l.trim_start().starts_with("//") || !pat.iter().any(|p| l.contains(p)) { continue }
            match AUDITED_PANIC_SITES.iter().position(|(af, snip, _)| *af == rel && l.contains(snip)) {
                Some(k) => used[k] += 1,
                None => unaudited.push(format!("{rel}:{}: {}", i + 1, l.trim())),
            }
        }
    }
    assert!(unaudited.is_empty(), "unaudited panic sites in lwc-style (return a StyleError instead, or justify in AUDITED_PANIC_SITES):\n  {}", unaudited.join("\n  "));
    let stale: Vec<_> = AUDITED_PANIC_SITES.iter().zip(&used).filter(|(_, n)| **n == 0).map(|((f, s, _), _)| format!("{f}: {s}")).collect();
    assert!(stale.is_empty(), "stale AUDITED_PANIC_SITES entries (site removed; drop the entry):\n  {}", stale.join("\n  "));
    assert_eq!(used.iter().sum::<usize>(), 13, "panic-site count changed; update AUDITED_PANIC_SITES and the review doc");
}
