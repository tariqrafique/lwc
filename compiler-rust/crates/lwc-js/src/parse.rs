use lwc_shared::CompileError;
use oxc::allocator::Allocator;
use oxc::ast::ast::{BindingIdentifier, IdentifierReference, LabelIdentifier, Program};
use oxc::ast_visit::Visit;
use oxc::parser::Parser;
use oxc::span::SourceType;

/// Parses as an ES module; TS/JSX inferred from extension (.ts/.tsx/.jsx), decorators accepted.
pub fn parse<'a>(alloc: &'a Allocator, src: &'a str, filename: &str) -> Result<Program<'a>, CompileError> {
    let st = SourceType::from_path(filename).unwrap_or_else(|_| SourceType::mjs()).with_module(true);
    let ret = Parser::new(alloc, src, st).parse();
    if ret.diagnostics.has_errors() {
        // Shipped: Babel's parser SyntaxError (`msg (line:col)`, 1-based line, 0-based column), wrapped as LWC1007.
        let first = ret.diagnostics.errors().next().expect("has_errors");
        let at = first.labels.as_ref().first().map(|l| l.offset() as usize).unwrap_or(0).min(src.len());
        return Err(parse_error(src, filename, at, &first.message));
    }
    Ok(ret.program)
}

/// The user-source parse (pipeline step 1). Shipped Babel parses with `errorRecovery` only in
/// experimentalErrorRecoveryMode; without it Babel throws at its first *recoverable* error too, which
/// oxc does not report. The one class reproduced: a strict-mode reserved word used as an identifier
/// (`let = ;`, `var static`, `x = implements`) -> "Unexpected reserved word 'w'." at that identifier,
/// whenever it comes before oxc's own error (or oxc has none).
pub fn parse_source<'a>(alloc: &'a Allocator, src: &'a str, filename: &str, recover: bool) -> Result<Program<'a>, CompileError> {
    match parse(alloc, src, filename) {
        Ok(program) if !recover => match first_reserved_ident(&program) {
            Some((at, w)) => Err(parse_error(src, filename, at, &format!("Unexpected reserved word '{w}'."))),
            None => Ok(program),
        },
        Err(e) if !recover => Err(earlier_reserved_ident(src, filename, &e).unwrap_or(e)),
        r => r,
    }
}

/// oxc stops at its error offset with an empty program, so the prefix before it is re-parsed with a
/// minimal completion (`0`, `0;` or nothing); a reserved identifier there precedes oxc's error in Babel.
fn earlier_reserved_ident(src: &str, filename: &str, e: &CompileError) -> Option<CompileError> {
    let at = e.diagnostics.first()?.location.as_ref()?.span().0 as usize;
    let at = utf16_to_byte(src, at);
    let prefix = &src[..at];
    for tail in ["0", "0;", ""] {
        let patched = format!("{prefix}{tail}");
        let alloc = Allocator::default();
        let st = SourceType::from_path(filename).unwrap_or_else(|_| SourceType::mjs()).with_module(true);
        let ret = Parser::new(&alloc, &patched, st).parse();
        if ret.diagnostics.has_errors() { continue; }
        return first_reserved_ident(&ret.program)
            .filter(|(off, _)| *off < at)
            .map(|(off, w)| parse_error(src, filename, off, &format!("Unexpected reserved word '{w}'.")));
    }
    None
}

fn utf16_to_byte(src: &str, u16_idx: usize) -> usize {
    let mut n = 0;
    for (b, ch) in src.char_indices() {
        if n >= u16_idx { return b; }
        n += ch.len_utf16();
    }
    src.len()
}

fn parse_error(src: &str, filename: &str, at: usize, msg: &str) -> CompileError {
    let (line, col) = lwc_shared::babel_line_col(src, at as u32);
    // @babel/core appends the code frame (start only) after a blank line.
    let frame = lwc_shared::code_frame(src, (line, col), None);
    let mut e = CompileError::babel("SyntaxError", format!("{msg} ({line}:{col})\n\n{frame}"), filename, Some(line), Some(col));
    // Babel's `err.loc` is a Position `{ line, column, index }` (index in UTF-16 units); normalizeToCompilerError
    // copies it as the CompilerError location. Stored zero-width (start = end = index); napi shapes a
    // Babel-parse 1007 location as `{ line, column, index }`.
    let index = src[..at].encode_utf16().count() as u32;
    e.diagnostics[0].location = Some(lwc_shared::SourceLocation { start_line: line, start_column: col, end_line: line, end_column: col, start: Some(index), end: Some(index) });
    e
}

/// Babel `checkReservedWord` (strict mode: modules are always strict): these words may not name a
/// reference, binding or label. Property keys / import-export names are IdentifierName and are allowed.
const STRICT_RESERVED: [&str; 9] = ["implements", "interface", "let", "package", "private", "protected", "public", "static", "yield"];

fn first_reserved_ident(program: &Program<'_>) -> Option<(usize, &'static str)> {
    #[derive(Default)]
    struct V { first: Option<(u32, &'static str)> }
    impl V {
        fn hit(&mut self, name: &str, start: u32) {
            if let Some(w) = STRICT_RESERVED.iter().find(|w| **w == name) {
                if self.first.is_none_or(|(s, _)| start < s) { self.first = Some((start, w)); }
            }
        }
    }
    impl<'a> Visit<'a> for V {
        fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) { self.hit(&it.name, it.span.start); }
        fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) { self.hit(&it.name, it.span.start); }
        fn visit_label_identifier(&mut self, it: &LabelIdentifier<'a>) { self.hit(&it.name, it.span.start); }
    }
    let mut v = V::default();
    v.visit_program(program);
    v.first.map(|(s, w)| (s as usize, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(src: &str, recover: bool) -> Option<(String, (u32, u32, u32))> {
        let alloc = Allocator::default();
        parse_source(&alloc, src, "foo.js", recover).err().map(|e| {
            let l = e.diagnostics[0].location.clone().expect("parse errors carry a location");
            assert_eq!((l.start, l.start_line, l.start_column), (l.end, l.end_line, l.end_column), "zero-width");
            (e.message.lines().next().unwrap().to_string(), (l.start_line, l.start_column, l.span().0))
        })
    }

    // Expected values: proofs/parse-location/shipped{,2,3}.json (9.5.3 oracle; `location` = Babel Position).
    #[test]
    fn reserved_word_as_identifier_matches_shipped_normal_mode() {
        let m = |w: &str, l: u32, c: u32| format!("foo.js: Unexpected reserved word '{w}'. ({l}:{c})");
        for (src, w, loc) in [
            ("let = ;", "let", (1, 0, 0)),
            ("let = 1;", "let", (1, 0, 0)),
            ("let /*c*/ = ;", "let", (1, 0, 0)),
            ("x; let = ;", "let", (1, 3, 3)),
            ("const s = \"é\"; let = ;", "let", (1, 15, 15)),
            ("var static = 1;", "static", (1, 4, 4)),
            ("function f(yield){}", "yield", (1, 11, 11)),
            ("({let});", "let", (1, 2, 2)),
            ("let x = 1;\nx = let;", "let", (2, 4, 15)),
            ("const a = `${let}`;", "let", (1, 13, 13)),
        ] {
            let (msg, got) = err(src, false).unwrap_or_else(|| panic!("{src:?} should fail"));
            assert!(msg.ends_with(&m(w, loc.0, loc.1)), "{src:?}: {msg}");
            assert_eq!(got, loc, "{src:?}");
        }
        for ok in ["class A { static = 1; let = 2; }", "obj.public = 1; ({private: 1});", "import { let as l } from 'm';", "let /* let */ x = 'let';"] {
            assert!(err(ok, false).is_none(), "{ok:?}");
        }
    }

    #[test]
    fn recovery_mode_keeps_parser_position_as_shipped() {
        // Babel errorRecovery skips the reserved-word error and throws at the next hard one.
        assert_eq!(err("let = ;", true).unwrap().1, (1, 6, 6));
        assert_eq!(err("x; let = ;", true).unwrap().1, (1, 9, 9));
        assert!(err("let = 1;", true).is_none());
    }

    #[test]
    fn parse_error_message_carries_babel_code_frame() {
        let alloc = Allocator::default();
        let e = parse_source(&alloc, "a\nb\nc\nlet = ;\nd\ne\nf\ng", "foo.js", false).unwrap_err();
        assert!(e.message.ends_with("foo.js: Unexpected reserved word 'let'. (4:0)\n\n  2 | b\n  3 | c\n> 4 | let = ;\n    | ^\n  5 | d\n  6 | e\n  7 | f"), "{}", e.message);
        let e = parse_source(&alloc, "const s = \"😀\"; let x = ;", "foo.js", false).unwrap_err();
        assert!(e.message.ends_with("(1:24)\n\n> 1 | const s = \"😀\"; let x = ;\n    |                         ^"), "{}", e.message);
    }

    #[test]
    fn parse_error_location_is_utf16_position() {
        assert_eq!(err("const x = 1;\nconst y = 2 +;\n", false).unwrap().1, (2, 13, 26));
        assert_eq!(err("const s = \"😀\"; let x = ;", false).unwrap().1, (1, 24, 24));
        assert_eq!(err("let x = ;", false).unwrap().1, (1, 8, 8));
        assert_eq!(err("}", false).unwrap().1, (1, 0, 0));
    }
}
