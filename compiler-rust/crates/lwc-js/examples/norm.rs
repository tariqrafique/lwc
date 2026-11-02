//! Dev helper: reprint JS through oxc (parens normalized) so babel and oxc outputs can be diffed.
use oxc::{allocator::Allocator, codegen::Codegen, parser::{ParseOptions, Parser}, span::SourceType};
fn main() {
    for f in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&f).unwrap();
        let alloc = Allocator::default();
        let r = Parser::new(&alloc, &src, SourceType::mjs()).with_options(ParseOptions { preserve_parens: false, ..Default::default() }).parse();
        if r.diagnostics.has_errors() { println!("PARSE-ERR {f}"); continue; }
        std::fs::write(&f, Codegen::new().build(&r.program).code).unwrap();
    }
}
