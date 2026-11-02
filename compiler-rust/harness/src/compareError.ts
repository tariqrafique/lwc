// Shared error comparator for every suite that compares a thrown compile error against a JS golden
// (csr.render, ssr.render, js.behavior, ssr.behavior, template.behavior). Policy (Migrate 2026-11-01,
// decisions.tsv `SEAM_GAP: babel-frame`):
//
// - PARSER errors: the golden is an uncoded Babel/JS syntax error: name `SyntaxError` (or code
//   `BABEL_PARSER_SYNTAX_ERROR`, or a message starting `SyntaxError: <filename>: `) and NO LWC code anywhere
//   (no numeric `code`, no `LWCnnnn` in the message). Babel's wording and code frame are not ported, so these
//   compare kind (error name) + filename + line + column only. A field is compared only when the golden has it.
// - Everything else (any LWC code, incl. LWC-coded errors Babel wrapped with a code frame via
//   buildCodeFrameError, e.g. `SyntaxError: <file>: LWC1112: ...`) stays an exact-text compare.
// - LWC-coded errors Babel wrapped via buildCodeFrameError (`SyntaxError: <file>: LWCnnnn: ...\n<code frame>`):
//   the FIRST LINE (name + filename + LWC text + any `(L:C)`) compares exactly; only the trailing code frame
//   (Babel artifact) is dropped. Harness decision 2026-11-01 under the same babel-frame SEAM_GAP.
// - No false pass: if the golden is an error, the actual must be an error too (`actual == null` → fail), and a
//   parser golden only matches an actual that is itself an uncoded SyntaxError.

export interface ErrorObservation {
    name?: string | null;
    message?: string | null;
    /** LWC diagnostic code (number) or a string code such as `BABEL_PARSER_SYNTAX_ERROR`. */
    code?: number | string | null;
    filename?: string | null;
    line?: number | null;
    column?: number | null;
}

/** A golden/actual error: the structured observation, or just its message text (csr/ssr goldens store text). */
export type ErrorLike = ErrorObservation | string;

export interface ParserFrame { name: string; filename: string | null; line: number | null; column: number | null }

export interface CompareResult { pass: boolean; mode: 'parser' | 'exact' | 'lwc-framed'; reason?: string }

const LWC_CODE_RE = /\bLWC\d+\b/;
// First message line: optional rollup `[plugin name] ` prefix, `SyntaxError: <filename>: <text>`, optional Babel
// `(line:column)` trailer. Filename = up to the first `: ` (Babel joins `${filename}: ${message}`).
const HEADER_RE = /^(?:\[plugin [^\]]+\] )?(SyntaxError): (.+?): (.*)$/;
const LOC_TRAILER_RE = /\((\d+):(\d+)\)\s*$/;

const toObs = (e: ErrorLike): ErrorObservation => (typeof e === 'string' ? { message: e } : e);
const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

/** Structured observation of a thrown value (JS @lwc/compiler, Babel, rollup-wrapped, or RustCompilerBackend error). */
export function observeError(e: any): ErrorObservation {
    if (e == null || typeof e !== 'object') return { name: null, message: String(e) };
    // rollup moves a plugin error's own `code` to `pluginCode` and sets code = 'PLUGIN_ERROR'.
    const rawCode = e.code === 'PLUGIN_ERROR' ? e.pluginCode : e.code;
    return {
        name: typeof e.name === 'string' ? e.name : null,
        message: String(e.message ?? e),
        code: typeof rawCode === 'number' || typeof rawCode === 'string' ? rawCode : null,
        filename: e.filename ?? e.loc?.file ?? e.id ?? null,
        line: num(e.loc?.line) ?? num(e.location?.line) ?? num(e.line),
        column: num(e.loc?.column) ?? num(e.location?.column) ?? num(e.column),
    };
}

/** Returns the parser frame when `e` is an uncoded parser (Babel SyntaxError) error, else null (exact compare). */
export function classifyParserError(e: ErrorLike | null | undefined): ParserFrame | null {
    if (e == null) return null;
    const o = toObs(e);
    const message = String(o.message ?? '');
    if (typeof o.code === 'number' || LWC_CODE_RE.test(message)) return null;
    const firstLine = message.split('\n')[0];
    const header = HEADER_RE.exec(firstLine);
    const isParser = o.name === 'SyntaxError' || o.code === 'BABEL_PARSER_SYNTAX_ERROR' || !!header;
    if (!isParser) return null;
    const trailer = LOC_TRAILER_RE.exec(firstLine);
    return {
        name: 'SyntaxError',
        filename: o.filename ?? header?.[2] ?? null,
        line: num(o.line) ?? (trailer ? Number(trailer[1]) : null),
        column: num(o.column) ?? (trailer ? Number(trailer[2]) : null),
    };
}

/**
 * Compare a golden error against the actual one. `golden == null` means "no error expected": passes only if
 * `actual == null`. `exact` picks the exact-text relation for non-parser errors (`includes` for suites whose
 * fixture text is a substring of the thrown message, e.g. ssr.behavior's error.txt).
 */
export function compareError(
    golden: ErrorLike | null | undefined,
    actual: ErrorLike | null | undefined,
    { exact = 'equal' }: { exact?: 'equal' | 'includes' } = {},
): CompareResult {
    const frame = classifyParserError(golden);
    if (golden == null) return actual == null ? { pass: true, mode: 'exact' } : { pass: false, mode: 'exact', reason: 'golden has no error, actual threw' };
    if (actual == null) return { pass: false, mode: frame ? 'parser' : 'exact', reason: 'golden is an error, actual did not throw' };
    if (!frame) {
        const g = String(toObs(golden).message ?? '');
        const gl = g.split('\n');
        if (gl.length > 1 && HEADER_RE.test(gl[0]) && LWC_CODE_RE.test(gl[0])) {
            const a1 = String(toObs(actual).message ?? '').split('\n')[0];
            const pass = exact === 'includes' ? a1.includes(gl[0]) : a1 === gl[0];
            return pass ? { pass, mode: 'lwc-framed' } : { pass, mode: 'lwc-framed', reason: 'first line (SyntaxError: <file>: LWCnnnn: ...) differs' };
        }
        const a = String(toObs(actual).message ?? '');
        const pass = exact === 'includes' ? a.includes(g) : a === g;
        return pass ? { pass, mode: 'exact' } : { pass, mode: 'exact', reason: 'message text differs' };
    }
    const got = classifyParserError(actual);
    if (!got) return { pass: false, mode: 'parser', reason: 'golden is an uncoded parser error, actual is not' };
    for (const k of ['filename', 'line', 'column'] as const) {
        if (frame[k] != null && got[k] !== frame[k]) return { pass: false, mode: 'parser', reason: `${k}: want ${frame[k]}, got ${got[k]}` };
    }
    return { pass: true, mode: 'parser' };
}
