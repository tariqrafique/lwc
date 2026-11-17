use oxc::ast::ast::Program;
use oxc::codegen::Codegen;

/// oxc codegen with comments preserved; this is the normalized output form.
fn source_literals() -> oxc::codegen::CodegenOptions {
    // D-0292: source number, string, and BigInt tokens print their `raw`.
    // Synthesized nodes have no `raw` and keep oxc's shortest / re-quote path.
    oxc::codegen::CodegenOptions {
        preserve_raw_literals: true,
        ..oxc::codegen::CodegenOptions::default()
    }
}

pub fn print(program: &Program<'_>, src: &str) -> String {
    Codegen::new()
        .with_options(source_literals())
        .with_source_text(src)
        .build(program)
        .code
}

/// [`print`], plus oxc's token map when `with_map` (the map's source is `src`).
pub fn print_mapped(
    program: &Program<'_>,
    src: &str,
    with_map: bool,
) -> (String, Option<oxc_sourcemap::SourceMap<'static>>) {
    let mut opts = source_literals();
    if with_map {
        opts.source_map_path = Some(std::path::PathBuf::from("input.js"));
    }
    let ret = Codegen::new()
        .with_options(opts)
        .with_source_text(src)
        .build(program);
    (ret.code, ret.map.map(|m| m.into_owned()))
}
