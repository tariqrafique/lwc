//! `lwc_js::parse::parse_source` (pipeline step 1): Babel-parity class, code, filename and Position, with oxc's
//! text (D-0248). Moved out of parse.rs so its unit tests never name `message` there (tests/oxc_diagnostic_boundary.rs R5).
use lwc_js::parse::parse_source;
use lwc_shared::CompileError;
use oxc::allocator::Allocator;

fn err(src: &str, _recover: bool) -> Option<(String, (u32, u32, u32))> {
    let alloc = Allocator::default();
    parse_source(&alloc, src, "foo.js").err().map(
        |CompileError {
             message,
             diagnostics: ds,
             ..
         }| {
            let l = ds[0].location.expect("parse errors carry a location");
            assert_eq!(
                (l.start, l.length),
                (None, None),
                "a Babel Position has no start/length"
            );
            (
                message.lines().next().unwrap().to_string(),
                (l.line, l.column, l.index.expect("Position.index")),
            )
        },
    )
}

// Expected positions: proofs/parse-location/shipped{,2,3}.json (9.5.3 oracle; `location` = Babel Position).
// Text is oxc's (D-0248), so only the position is pinned.
#[test]
fn reserved_word_as_identifier_matches_shipped_position_normal_mode() {
    // A clean parse: oxc's semantic check reports the word where Babel does.
    for (src, loc) in [
        ("let = 1;", (1, 0, 0)),
        ("var static = 1;", (1, 4, 4)),
        ("function f(yield){}", (1, 11, 11)),
        ("({let});", (1, 2, 2)),
        ("let x = 1;\nx = let;", (2, 4, 15)),
        ("const a = `${let}`;", (1, 13, 13)),
    ] {
        let (_, got) = err(src, false).unwrap_or_else(|| panic!("{src:?} should fail"));
        assert_eq!(got, loc, "{src:?}");
    }
    for ok in [
        "class A { static = 1; let = 2; }",
        "obj.public = 1; ({private: 1});",
        "import { let as l } from 'm';",
        "let /* let */ x = 'let';",
    ] {
        assert!(err(ok, false).is_none(), "{ok:?}");
    }
}

#[test]
fn reserved_word_before_a_parse_error_is_an_oxc_position_d0248() {
    // The semantic check runs only on a clean parse, so oxc's parser error wins: D-0248 position divergence
    // (9.5.3 throws "Unexpected reserved word 'let'." at the word; its position in the comment).
    for (src, ours) in [
        ("let = ;", (1, 6, 6)),                    // 9.5.3 (1, 0, 0)
        ("let /*c*/ = ;", (1, 12, 12)),            // 9.5.3 (1, 0, 0)
        ("x; let = ;", (1, 9, 9)),                 // 9.5.3 (1, 3, 3)
        ("const s = \"é\"; let = ;", (1, 21, 21)), // 9.5.3 (1, 15, 15)
    ] {
        assert_eq!(err(src, false).map(|e| e.1), Some(ours), "{src:?}");
    }
}

#[test]
fn recovery_mode_keeps_parser_position_as_shipped() {
    // Parser errors still win (same position as 9.5.3 recovery).
    assert_eq!(err("let = ;", true).unwrap().1, (1, 6, 6));
    assert_eq!(err("x; let = ;", true).unwrap().1, (1, 9, 9));
    // `let = 1;` is a clean parse plus a reserved-word early error. 9.5.3 recovery compiles it; D-0259
    // keeps the semantic check on, so we reject (D-0245, fixture rw-let-1-rec).
    assert_eq!(err("let = 1;", true).map(|e| e.1), Some((1, 0, 0)));
}

#[test]
fn default_export_abstract_class_is_an_lwc_owned_syntax_error() {
    let (msg, loc) = err("export default abstract class A {}", false).unwrap();
    assert_eq!(
        msg,
        "foo.js: 'abstract' classes are TypeScript syntax; LWC compiles JavaScript only. (1:23)"
    );
    assert_eq!(loc, (1, 23, 23));
    let rec = err("export default abstract class A {}", true)
        .expect("abstract guard runs in recovery too");
    assert_eq!(rec, (msg, loc));
}

#[test]
fn parse_error_message_carries_babel_code_frame() {
    let alloc = Allocator::default();
    let e = parse_source(&alloc, "a\nb\nc\nlet = 1;\nd\ne\nf\ng", "foo.js").unwrap_err();
    assert!(
        e.message.ends_with(
            " (4:0)\n\n  2 | b\n  3 | c\n> 4 | let = 1;\n    | ^\n  5 | d\n  6 | e\n  7 | f"
        ),
        "{}",
        e.message
    );
    let e = parse_source(&alloc, "const s = \"😀\"; let x = ;", "foo.js").unwrap_err();
    assert!(
        e.message.ends_with(
            "(1:24)\n\n> 1 | const s = \"😀\"; let x = ;\n    |                         ^"
        ),
        "{}",
        e.message
    );
}

#[test]
fn parse_error_location_is_utf16_position() {
    assert_eq!(
        err("const x = 1;\nconst y = 2 +;\n", false).unwrap().1,
        (2, 13, 26)
    );
    assert_eq!(
        err("const s = \"😀\"; let x = ;", false).unwrap().1,
        (1, 24, 24)
    );
    assert_eq!(err("let x = ;", false).unwrap().1, (1, 8, 8));
    assert_eq!(err("}", false).unwrap().1, (1, 0, 0));
}
