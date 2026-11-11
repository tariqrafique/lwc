//! LWC component JS compiler (Rust port of `@lwc/compiler` javascript.ts +
//! `@lwc/babel-plugin-component`), built on oxc.
//!
//! Pipeline: parse_source once (oxc) -> span edits for LWC / private fields / class
//! properties on that Program (no re-parse between them) -> one parse of the edited
//! source -> rest/spread, LWS unforgeables and async on that AST -> print once.
//! Version stamp and the helper prelude are inserts on the print (D-0262 / D-0265).
//! See `transforms/mod.rs` for order.

pub mod codegen;
pub mod constants;
pub mod helpers;
pub mod origin;
pub mod parse;
pub mod srcmap;
pub mod tag_name;
pub mod transforms;

pub use origin::Origin;
pub use parse::{parse_count, reset_parse_count};

use lwc_shared::api_version::{self, ApiFeature};
use lwc_shared::edits;
use lwc_shared::{CompileError, CompileOptions, CompileResult, CompilerDiagnostic};
use oxc::ast::ast::{Class, Declaration, ExportDefaultDeclarationKind, Program, Statement};
use oxc::ast_visit::{Visit, walk};
use oxc::span::GetSpan;
use std::collections::HashSet;

/// `@babel/core`'s `err.code` on an error thrown out of `transformSync`: a parser throw is wrapped as
/// `BABEL_PARSE_ERROR`, any plugin/visitor throw as `BABEL_TRANSFORM_ERROR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BabelCode {
    Parse,
    Transform,
}

/// How shipped `babel.transformSync` ends without output, before javascript.ts surfaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsError {
    /// Thrown (javascript.ts `catch (e)`), shaped as Babel throws it: a raw Babel error
    /// ([`CompileError::babel`]) or an LWC plugin error framed by `buildCodeFrameError`
    /// ([`CompileError::framed`], `lwcCode` on its diagnostic).
    Thrown {
        error: CompileError,
        code: BabelCode,
    },
    /// Recovery mode: `result.metadata.lwcErrors`, unframed `LWC####: msg` diagnostics whose filename
    /// is Babel's resolved `file.opts.filename`.
    LwcErrors(Vec<CompilerDiagnostic>),
}

impl From<CompileError> for JsError {
    fn from(error: CompileError) -> Self {
        JsError::Thrown {
            error,
            code: BabelCode::Transform,
        }
    }
}

impl std::fmt::Display for JsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsError::Thrown { error, .. } => write!(
                f,
                "{}: {}",
                error.name.as_deref().unwrap_or("Error"),
                error.message
            ),
            JsError::LwcErrors(ds) => ds
                .iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .fmt(f),
        }
    }
}

pub fn transform(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, JsError> {
    // LWC-plugin errors (`CompileError::lwc`) are framed here, once: recovery mode collects them into
    // `lwcErrors` (messages stay `LWC####: msg`, even for a single one); otherwise Babel throws them as
    // `buildCodeFrameError`. Babel-internal errors (`CompileError::babel`) are never collected, as in shipped.
    match transform_inner(src, filename, options) {
        Err(JsError::Thrown { error, .. })
            if error.is_unframed_lwc() && options.experimental_error_recovery_mode =>
        {
            Err(lwc_errors(error.diagnostics))
        }
        Err(JsError::Thrown { error, code }) => Err(JsError::Thrown {
            error: error.framed(filename, src),
            code,
        }),
        Err(JsError::LwcErrors(ds)) => Err(lwc_errors(ds)),
        r => r,
    }
}

/// `lwcErrors` diagnostics carry Babel's `file.opts.filename`, which `@babel/core` resolves against the cwd.
fn lwc_errors(mut ds: Vec<CompilerDiagnostic>) -> JsError {
    let cwd = std::env::current_dir().unwrap_or_default();
    for d in &mut ds {
        d.filename = d.filename.as_deref().map(|f| resolve_like_node(&cwd, f));
    }
    JsError::LwcErrors(ds)
}

/// Node `path.resolve(cwd, f)` for a relative or absolute POSIX path: `.` / `..` segments and repeated
/// slashes collapse, a trailing slash drops.
pub fn resolve_like_node(cwd: &std::path::Path, f: &str) -> String {
    let joined = if f.starts_with('/') {
        f.to_string()
    } else {
        format!("{}/{f}", cwd.to_string_lossy())
    };
    let mut out: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    format!("/{}", out.join("/"))
}

fn transform_inner(
    src: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<CompileResult, JsError> {
    // outputConfig.sourcemap: every text stage below records its offset map in `chain` (no-op when off).
    let mut chain = srcmap::Chain::new(options.sourcemap.is_enabled());
    // 1. One user-source parse. Private-method forward validates (and, flag on, records the rename).
    // The rename is not applied as text: reverse restores every `#m`, and class-properties does not
    // touch private methods. LWC's decorated-class check treats flag-on methods as already renamed.
    let alloc0 = oxc::allocator::Allocator::default();
    let program0 =
        parse::parse_source(&alloc0, src, filename).map_err(|error| JsError::Thrown {
            error,
            code: BabelCode::Parse,
        })?;
    let deferred_pm_check =
        match transforms::private_methods::forward(&program0, filename, options)? {
            transforms::private_methods::Forward::Done(_, _) => false,
            transforms::private_methods::Forward::Deferred => true,
        };
    // Identity origin: every span below is in `src`. (The old path re-parsed the renamed text and
    // mapped offsets back; skipping that re-parse leaves the offsets on the original source.)
    let (_same, identity) = edits::apply_mapped(src, Vec::new()).expect("empty edit list");
    let origin = Origin {
        src,
        pieces: &identity,
    };
    let mut all = transforms::run(&program0, src, filename, options, origin)
        .map_err(|e| unrename_locations(e, origin))?;
    if deferred_pm_check {
        transforms::private_methods::not_enabled(&program0, filename)?;
    }
    // Private fields then class-properties, both on this Program. Class-properties reads the
    // private-field edits as an overlay (marker keys, rewritten initializers) and lowers nested
    // classes in the same walk. No re-parse between them.
    // `export default class extends` becomes `registerComponent(class extends …)`. Private fields
    // then sees an expression: sequence-wrap the keys and hoist `var` at scope start.
    let wrapped_as_expr: HashSet<u32> = all
        .iter()
        .filter(|e| e.text.contains("registerComponent("))
        .map(|e| e.end)
        .collect();
    let mut pf = transforms::private_fields::run(&program0, src, &wrapped_as_expr);
    // `const foo = class { foo }` is named `class foo` only when it stays the declarator init.
    // registerDecorators wraps the class first; the old re-parse then did not see `const foo = class`
    // and left it anonymous. Drop that name insert when a wrap lands on the same class start.
    let wrapped_at: std::collections::HashSet<u32> = all
        .iter()
        .filter(|e| e.start == e.end && e.text.contains("registerDecorators"))
        .map(|e| e.start)
        .collect();
    pf.retain(|e| {
        !(e.start >= 5
            && e.start == e.end
            && e.text.starts_with(' ')
            && wrapped_at.contains(&(e.start - 5)))
    });
    // Class-properties slices field initializers. Those slices must already show LWC and
    // private-field edits, because the field itself is deleted and a global apply would drop
    // edits contained in that deletion (nested `registerDecorators`, `#field` rewrites).
    let mut prior = all.clone();
    prior.extend(pf.iter().cloned());
    let (cp, rendered) = transforms::class_properties::run_after(&program0, src, &prior);
    // Initializers moved into a constructor already contain the private-field edits inside them.
    pf.retain(|e| {
        !rendered
            .iter()
            .any(|&(s, end)| transforms::class_properties::edit_rendered_into(s, end, e))
    });
    // One apply, in the order the old re-parse pipeline produced. Scope-slot fills (a leading
    // newline) and inserts at a class/statement end land before an earlier insert at that offset:
    // the re-parse put them at the boundary, in front of text the earlier pass had already placed
    // there. Every other same-offset insert stays in pass order (the node start moved past it).
    let ends = boundary_ends(&program0);
    let mut tagged: Vec<(edits::Edit, u8)> = Vec::new();
    tagged.extend(all.into_iter().map(|e| (e, 0u8)));
    tagged.extend(pf.into_iter().map(|e| (e, 1u8)));
    tagged.extend(cp.into_iter().map(|e| (e, 2u8)));
    all = order_same_offset(tagged, &ends);
    let (edited, pieces) =
        edits::apply_mapped(src, all).expect("lwc-js edits never partially overlap");
    chain.push_edit(pieces);
    // 2. The one print parse. Rest/spread, async and unforgeables are AST passes on this Program.
    let do_rest = !api_version::is_api_feature_enabled(
        ApiFeature::DisableObjectRestSpreadTransformation,
        api_version::get_api_version_from_number(options.api_version),
    );
    let alloc = oxc::allocator::Allocator::default();
    let mut program = parse::parse(&alloc, &edited, filename).map_err(|e| {
        CompileError::new(
            format!(
                "internal: transformed output failed to re-parse: {}",
                e.message
            ),
            filename,
        )
    })?;
    if do_rest {
        transforms::lower::apply_object_rest_spread(&alloc, &edited, filename, &mut program)?;
    }
    // Localize before async so private-field helper names are fixed, then localize again for
    // helpers the async transform introduced (AB-L0075: one prelude from the union).
    let mut helper_sets = vec![helpers::localize(&alloc, &mut program)];
    let ran_async = options.enable_lightning_web_security_transforms
        && transforms::lower::program_has_async(&program);
    if ran_async {
        transforms::lower::apply_async_functions(&alloc, &edited, filename, &mut program)?;
        helper_sets.push(helpers::localize(&alloc, &mut program));
    }
    let (mut printed, map) = codegen::print_mapped(&program, &edited, true);
    let map = map.expect("codegen with_map always produces a map");
    if chain.enabled() {
        chain.push_codegen(&map, &edited, &printed);
    }
    let mut stamps = helpers::post_codegen_edits(&program, &edited, &printed, &map, filename)?;
    if options.enable_lightning_web_security_transforms {
        // Edits are in `edited` coordinates (the AST's spans). The codegen map moves them onto
        // the print. No parse of the print.
        let rw = transforms::unforgeables::run(&program, &edited, filename)?;
        let finds = rw.find;
        if !finds.is_empty() || !rw.edits.is_empty() {
            let mut spans = Vec::new();
            let mut search_from = 0usize;
            for (needle, repl) in &finds {
                let at = printed[search_from..]
                    .find(needle)
                    .map(|i| i + search_from)
                    .ok_or_else(|| {
                        CompileError::new(
                            format!("internal: unforgeable `{needle}` not in printed output"),
                            filename,
                        )
                    })?;
                let after = printed.as_bytes().get(at + needle.len()).copied();
                // A following `.` / `[` / `(` needs the wrapping parens (`(...).href`). A statement
                // context does not; async's old reprint dropped those (codegen omits paren nodes).
                let text = if ran_async && !matches!(after, Some(b'.' | b'[' | b'(')) {
                    strip_wrapping_parens(repl).to_string()
                } else {
                    repl.clone()
                };
                spans.push(edits::Edit::replace(
                    at as u32,
                    (at + needle.len()) as u32,
                    text,
                ));
                search_from = at + needle.len();
            }
            // `var` injected at 0 is already in print coordinates. Other edits are source spans.
            let (at_zero, rest): (Vec<_>, Vec<_>) = rw
                .edits
                .into_iter()
                .partition(|e| e.start == 0 && e.end == 0);
            let rest = helpers::remap_through_codegen(&map, &edited, &printed, rest, filename)?;
            spans.extend(rest);
            spans.extend(at_zero);
            let (out, pieces) =
                edits::apply_mapped(&printed, spans).expect("lwc-js edits never partially overlap");
            chain.push_edit(pieces.clone());
            stamps = helpers::remap_edits(stamps, &pieces);
            printed = out;
        }
    }
    if let Some(pre) = helpers::prelude(&helper_sets) {
        stamps.push(edits::Edit::insert(0, pre));
    }
    let (code, pieces) =
        edits::apply_mapped(&printed, stamps).expect("lwc-js edits never partially overlap");
    chain.push_edit(pieces);
    let map = chain.finish(src, filename, &code);
    Ok(match (options.sourcemap, map) {
        (lwc_shared::Sourcemap::Inline, Some(m)) => {
            // babel: `code + "\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,..."`
            let sep = if code.ends_with('\n') { "" } else { "\n" };
            let code = format!("{code}{sep}//# sourceMappingURL={}", m.to_data_url());
            CompileResult {
                code,
                map: None,
                ..Default::default()
            }
        }
        (_, map) => CompileResult {
            code,
            map: map.map(|m| m.to_json_string()),
            ..Default::default()
        },
    })
}

/// Class ends, and the ends of the statements that declare them. An insert there is "after the
/// node": a later pass's insert at that offset belongs in front of an earlier one.
fn boundary_ends(program: &Program<'_>) -> HashSet<u32> {
    struct Ends(HashSet<u32>);
    impl Visit<'_> for Ends {
        fn visit_class(&mut self, it: &Class<'_>) {
            self.0.insert(it.span.end);
            walk::walk_class(self, it);
        }
        fn visit_statement(&mut self, it: &Statement<'_>) {
            let marks = match it {
                Statement::ClassDeclaration(_) => true,
                Statement::ExportDeclaration(e) => {
                    matches!(e.declaration, Declaration::ClassDeclaration(_))
                }
                Statement::ExportDefaultDeclaration(e) => matches!(
                    e.declaration,
                    ExportDefaultDeclarationKind::ClassDeclaration(_)
                ),
                _ => false,
            };
            if marks {
                self.0.insert(it.span().end);
            }
            walk::walk_statement(self, it);
        }
    }
    let mut v = Ends(HashSet::new());
    v.visit_program(program);
    v.0
}

/// Stable order for one `apply`. `pass` is 0 for the LWC edits, 1 for private fields, 2 for
/// class properties. Scope-slot text starts with a newline; those and end-boundary inserts sort
/// later-pass first. Other inserts keep pass order.
fn order_same_offset(mut edits: Vec<(edits::Edit, u8)>, ends: &HashSet<u32>) -> Vec<edits::Edit> {
    let later_first =
        |e: &edits::Edit| e.start == e.end && (e.text.starts_with('\n') || ends.contains(&e.start));
    edits.sort_by(|(a, pa), (b, pb)| {
        (a.start, a.end).cmp(&(b.start, b.end)).then_with(|| {
            if a.start != a.end || b.start != b.end {
                return std::cmp::Ordering::Equal;
            }
            match (later_first(a), later_first(b)) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (true, true) => pb.cmp(pa),
                (false, false) => pa.cmp(pb),
            }
        })
    });
    edits.into_iter().map(|(e, _)| e).collect()
}

/// One balanced `(...)` pair wrapping all of `s`, if it has one.
fn strip_wrapping_parens(s: &str) -> &str {
    let b = s.as_bytes();
    if b.first() != Some(&b'(') || b.last() != Some(&b')') || s.len() < 2 {
        return s;
    }
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i != s.len() - 1 {
                    return s;
                }
            }
            _ => {}
        }
    }
    &s[1..s.len() - 1]
}

fn unrename_locations(e: JsError, origin: Origin<'_>) -> JsError {
    if origin.is_identity() {
        return e;
    }
    match e {
        JsError::Thrown { mut error, code } => {
            unrename_diagnostics(&mut error.diagnostics, origin);
            JsError::Thrown { error, code }
        }
        JsError::LwcErrors(mut ds) => {
            unrename_diagnostics(&mut ds, origin);
            JsError::LwcErrors(ds)
        }
    }
}

/// Maps every diagnostic's node span back to the original source; returns the first one's new span.
fn unrename_diagnostics(
    ds: &mut [CompilerDiagnostic],
    origin: Origin<'_>,
) -> Option<lwc_shared::SourceLocation> {
    let index = lwc_shared::LineIndex::new(origin.src);
    let mut first = None;
    for (i, d) in ds.iter_mut().enumerate() {
        // Every transform error is `CompileError::generate_at`, whose location is its node span converted once.
        let Some(l) = d.node_location else { continue };
        let (start, end) = (origin.map(l.start, false), origin.map(l.end, true));
        let span = index.source_location(start, end);
        d.node_location = Some(span);
        d.location = Some(span.into());
        if i == 0 {
            first = Some(span);
        }
    }
    first
}
