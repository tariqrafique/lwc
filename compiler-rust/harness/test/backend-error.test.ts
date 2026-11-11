// D-0239 / D-0241 / D-0267: Rust backend shapes napi errors from `location` only.
import { describe, it, expect } from 'vitest';
import { errorFromNapi, type NapiCompileError } from '../src/backend';
import { observeError } from '../src/compareError';

describe('errorFromNapi (D-0239/D-0241)', () => {
    it('uses location when present', () => {
        const ne: NapiCompileError = {
            message: 'LWC1061: a',
            code: 1061,
            filename: 'a.js',
            location: { line: 2, column: 6, start: 10, length: 3 },
        };
        const err: any = errorFromNapi(ne);
        expect(err).toBeInstanceOf(Error);
        expect(err.message).toBe('LWC1061: a');
        expect(err.code).toBe(1061);
        expect(err.filename).toBe('a.js');
        expect(err.location).toEqual({ line: 2, column: 6, start: 10, length: 3 });
        expect(err.line).toBeUndefined();
        expect(err.column).toBeUndefined();
        expect(observeError(err)).toMatchObject({ line: 2, column: 6, code: 1061, filename: 'a.js' });
    });

    it('does not invent location from own line/column when location is absent', () => {
        // Runtime may still carry transitional own keys until JS Style deletes them; type omits them.
        const ne = {
            message: 'LWC1021: bad',
            code: 1021,
            line: 7,
            column: 4,
        } as NapiCompileError;
        const err: any = errorFromNapi(ne);
        expect(err.location).toBeUndefined();
        expect(err.line).toBeUndefined();
        expect(err.column).toBeUndefined();
        expect(observeError(err)).toMatchObject({ line: null, column: null, code: 1021 });
    });

    it('does not invent location from loc either', () => {
        const ne = {
            message: 'x',
            loc: { line: 3, column: 1 },
            line: 3,
            column: 1,
        } as NapiCompileError;
        const err: any = errorFromNapi(ne);
        expect(err.location).toBeUndefined();
        expect(err.loc).toBeUndefined();
        expect(observeError(err).line).toBeNull();
    });

    it('treats null/undefined location as absent even when line/column are set', () => {
        for (const location of [null, undefined] as const) {
            const err: any = errorFromNapi({ message: 'm', location, line: 1, column: 2 } as NapiCompileError);
            expect(err.location).toBeUndefined();
            expect(observeError(err).line).toBeNull();
        }
    });

    it('aggregate members keep location; AggregateError name/errors come from the constructor', () => {
        const ne = {
            message: 'Multiple errors occurred during compilation.',
            name: 'ShouldNotAppear',
            errors: [
                { message: 'LWC1100: a', code: 1100, location: { line: 1, column: 2, start: 3, length: 4 } },
                { message: 'LWC1101: b', code: 1101, line: 9, column: 9 }, // no location → none
            ],
        } as NapiCompileError;
        const err: any = errorFromNapi(ne);
        expect(err).toBeInstanceOf(AggregateError);
        expect(err.name).toBe('AggregateError');
        expect(err.errors).toHaveLength(2);
        expect(err.errors[0].location).toEqual({ line: 1, column: 2, start: 3, length: 4 });
        expect(err.errors[1].location).toBeUndefined();
        const obs = observeError(err);
        expect(obs.aggregate?.errors[0].location).toEqual({ line: 1, column: 2, start: 3, length: 4 });
        expect(obs.aggregate?.errors[1].location).toBeNull();
    });
});
