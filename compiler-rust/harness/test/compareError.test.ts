// Unit tests for src/compareError.ts (babel-frame policy, decisions.tsv `SEAM_GAP: babel-frame`).
import { describe, it, expect } from 'vitest';
import { classifyParserError, compareError, observeError } from '../src/compareError';

const FILE = '/fx/modules/x/foo/foo.js';
const babelGolden = `SyntaxError: ${FILE}: Unexpected token, expected "," (3:12)\n  1 | x\n> 3 | y\n    |             ^`;

describe('compareError', () => {
    it('parser error: different message text, same kind/file/line/col → pass', () => {
        const actual = { name: 'SyntaxError', message: `SyntaxError: ${FILE}: Expected \`,\` but found \`}\``, filename: FILE, line: 3, column: 12 };
        expect(compareError(babelGolden, actual)).toEqual({ pass: true, mode: 'parser' });
        // Text-only actual with Babel-style header/trailer also classifies.
        expect(compareError(babelGolden, `SyntaxError: ${FILE}: something else (3:12)`).pass).toBe(true);
    });
    it('parser error: different line → fail', () => {
        const r = compareError(babelGolden, { name: 'SyntaxError', message: 'x', filename: FILE, line: 4, column: 12 });
        expect(r).toMatchObject({ pass: false, mode: 'parser' });
        expect(r.reason).toMatch(/^line/);
    });
    it('parser error: different column or filename → fail', () => {
        expect(compareError(babelGolden, { name: 'SyntaxError', message: 'x', filename: FILE, line: 3, column: 13 }).pass).toBe(false);
        expect(compareError(babelGolden, { name: 'SyntaxError', message: 'x', filename: '/other.js', line: 3, column: 12 }).pass).toBe(false);
    });
    it('parser golden vs LWC-coded or non-SyntaxError actual → fail (kind)', () => {
        expect(compareError(babelGolden, { name: 'SyntaxError', message: `SyntaxError: ${FILE}: LWC1007: x (3:12)`, filename: FILE, line: 3, column: 12 }).pass).toBe(false);
        expect(compareError(babelGolden, { name: 'Error', message: 'boom', filename: FILE, line: 3, column: 12 }).pass).toBe(false);
    });
    it('LWC-coded error with different text → fail (exact mode), even when Babel-framed', () => {
        const coded = `SyntaxError: ${FILE}: LWC1112: @api get a and @api set a detected.\n> 9 | @api`;
        expect(classifyParserError(coded)).toBeNull();
        expect(compareError(coded, 'LWC1112: @api get a and @api set a detected.')).toMatchObject({ pass: false, mode: 'lwc-framed' });
        expect(compareError('LWC1061: a', { name: 'Error', message: 'LWC1061: b', code: 1061 })).toMatchObject({ pass: false, mode: 'exact' });
        expect(compareError(coded, coded)).toEqual({ pass: true, mode: 'lwc-framed' });
    });
    it('rust did not throw vs golden error → fail (both modes)', () => {
        expect(compareError(babelGolden, undefined)).toMatchObject({ pass: false, mode: 'parser' });
        expect(compareError('LWC1061: a', null)).toMatchObject({ pass: false, mode: 'exact' });
        expect(compareError(null, 'LWC1061: a').pass).toBe(false);
        expect(compareError(null, null).pass).toBe(true);
    });
    it('structured golden: BABEL_PARSER_SYNTAX_ERROR code or SyntaxError name without LWC code', () => {
        expect(classifyParserError({ name: 'Error', code: 'BABEL_PARSER_SYNTAX_ERROR', message: 'x', line: 1, column: 2 })).toEqual({ name: 'SyntaxError', filename: null, line: 1, column: 2 });
        expect(classifyParserError({ name: 'SyntaxError', code: 1007, message: 'x' })).toBeNull();
        // Fields absent from the golden are not compared (js/template behavior goldens record name + line only).
        expect(compareError({ name: 'SyntaxError', message: '', line: 2 }, { name: 'SyntaxError', message: 'y', line: 2, column: 9 }).pass).toBe(true);
    });
    it('exact includes mode (ssr.behavior error.txt substring)', () => {
        expect(compareError('boom', '[plugin x] a.js: boom', { exact: 'includes' }).pass).toBe(true);
        expect(compareError('boom', '[plugin x] a.js: boom', { exact: 'equal' }).pass).toBe(false);
    });
    it('observeError reads rollup-wrapped and rust-backend errors', () => {
        const rollup = Object.assign(new SyntaxError('m'), { code: 'PLUGIN_ERROR', pluginCode: 'BABEL_PARSER_SYNTAX_ERROR', id: FILE, loc: { file: FILE, line: 3, column: 12 } });
        expect(observeError(rollup)).toEqual({ name: 'SyntaxError', message: 'm', code: 'BABEL_PARSER_SYNTAX_ERROR', filename: FILE, line: 3, column: 12 });
        const rust = Object.assign(new Error('LWC1061: a'), { code: 1061, location: { line: 1, column: 5 }, line: 1, column: 5 });
        expect(observeError(rust)).toEqual({ name: 'Error', message: 'LWC1061: a', code: 1061, filename: null, line: 1, column: 5 });
    });
    it('lwc-framed: first line exact, code frame ignored; bare LWC message fails', () => {
        const g = 'SyntaxError: /v/x.js: LWC1112: bad wire (3:4)\n  1 | foo\n> 3 | bar';
        expect(compareError(g, 'SyntaxError: /v/x.js: LWC1112: bad wire (3:4)\n> 3 | other frame').pass).toBe(true);
        expect(compareError(g, 'LWC1112: bad wire').pass).toBe(false);
        expect(compareError(g, 'SyntaxError: /v/x.js: LWC1112: bad wire (3:5)').pass).toBe(false);
        expect(compareError(g, null).pass).toBe(false);
    });
});

describe('recovery-mode aggregate (errors[])', () => {
    const sub = (code: number, message: string, line: number, column: number, start: number, length: number) =>
        Object.assign(new Error(message), { code, location: { line, column, start, length } });
    const mk = (subs: any[]) => Object.assign(new AggregateError(subs, 'Multiple errors occurred during compilation.'), { errors: subs });
    const golden = observeError(mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: b', 5, 6, 7, 8)]));
    it('passes only on identical ordered members', () => {
        expect(compareError(golden, observeError(mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: b', 5, 6, 7, 8)])))).toEqual({ pass: true, mode: 'aggregate' });
    });
    it('fails on order, count, code, message, location, missing errors[], top-level message/code, no throw', () => {
        const bad = [
            mk([sub(1101, 'LWC1101: b', 5, 6, 7, 8), sub(1100, 'LWC1100: a', 1, 2, 3, 4)]),
            mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4)]),
            mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1102, 'LWC1101: b', 5, 6, 7, 8)]),
            mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: c', 5, 6, 7, 8)]),
            mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: b', 5, 6, 7, 9)]),
            Object.assign(new Error('Multiple errors occurred during compilation.'), {}),
            Object.assign(mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: b', 5, 6, 7, 8)]), { message: 'x' }),
            Object.assign(mk([sub(1100, 'LWC1100: a', 1, 2, 3, 4), sub(1101, 'LWC1101: b', 5, 6, 7, 8)]), { code: 1100 }),
        ];
        for (const b of bad) expect(compareError(golden, observeError(b)).pass).toBe(false);
        expect(compareError(golden, null).pass).toBe(false);
    });
});
