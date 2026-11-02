// Shared plumbing for the option-coverage rows (js-options, template-options, ssr_dynamic_imports, *-api).
// Goldens are the JS oracle only: UPDATE_GOLDENS=1 is refused on any non-JS backend.
import fs from 'node:fs';
import path from 'node:path';
import { expect } from 'vitest';
import { observeAggregate, type AggregateObservation } from './compareError';

export const UPDATE = !!process.env.UPDATE_GOLDENS;
export const BACKEND_ID = process.env.LWC_COMPILER_BACKEND ?? 'js';
if (UPDATE && BACKEND_ID !== 'js') throw new Error('Refusing UPDATE_GOLDENS on a non-JS backend: goldens are the JS oracle');

export const lwcCodes = (s: string) => [...s.matchAll(/LWC(\d{4})/g)].map((m) => Number(m[1]));

/** Error observation: name, sorted unique LWC codes (top-level code, aggregate members, codes in messages), line. */
export function errorObservation(e: any): { ok: false; name: string; codes: number[]; line: number | null; aggregate?: AggregateObservation } {
    const msgs = [e?.message ?? '', ...(e?.errors ?? []).map((x: any) => x?.message ?? '')].join('\n');
    const codes = [...new Set<number>([
        ...(e?.code != null ? [Number(e.code)] : []),
        ...(e?.errors ?? []).map((x: any) => x?.code).filter((c: any) => c != null).map(Number),
        ...lwcCodes(msgs),
    ])].sort((a, b) => a - b);
    const aggregate = observeAggregate(e);  // recovery mode: ordered members with code/message/location, compared exactly
    return { ok: false, name: e?.name ?? typeof e, codes, line: e?.location?.line ?? null, ...(aggregate ? { aggregate } : {}) };
}

/** Error multiset (one entry per aggregate member) for error-recovery cases. */
export function errorMultiset(e: any): number[] {
    const members: any[] = Array.isArray(e?.errors) && e.errors.length ? e.errors : [e];
    return members.flatMap((x) => (x?.code != null ? [Number(x.code)] : lwcCodes(x?.message ?? '').slice(0, 1))).sort((a, b) => a - b);
}

export const goldenName = (key: string) => key.replace(/[\\/]/g, '__') + '.json';

/** One golden file per case. */
export function checkGolden(dir: string, key: string, observed: unknown): void {
    const g = path.join(dir, goldenName(key));
    if (UPDATE || !fs.existsSync(g)) {
        if (!UPDATE) throw new Error(`missing golden ${g} (record with UPDATE_GOLDENS=1 on the JS backend)`);
        fs.mkdirSync(dir, { recursive: true });
        fs.writeFileSync(g, JSON.stringify(observed, null, 2) + '\n');
    } else {
        expect(observed).toEqual(JSON.parse(fs.readFileSync(g, 'utf8')));
    }
}

/**
 * One golden file per fixture holding every matrix cell (`{ [cell]: observed }`); tests of one file run sequentially, so
 * UPDATE writes merge cell by cell. Keeps the boundary matrices at one file per fixture instead of one per cell.
 */
export function checkGoldenCell(dir: string, key: string, cell: string, observed: unknown): void {
    const g = path.join(dir, goldenName(key));
    if (UPDATE) {
        fs.mkdirSync(dir, { recursive: true });
        const cur = fs.existsSync(g) ? JSON.parse(fs.readFileSync(g, 'utf8')) : {};
        cur[cell] = observed;
        const sorted = Object.fromEntries(Object.entries(cur).sort(([a], [b]) => a.localeCompare(b)));
        fs.writeFileSync(g, JSON.stringify(sorted, null, 1) + '\n');
        return;
    }
    if (!fs.existsSync(g)) throw new Error(`missing golden ${g} (record with UPDATE_GOLDENS=1 on the JS backend)`);
    const want = JSON.parse(fs.readFileSync(g, 'utf8'));
    if (!(cell in want)) throw new Error(`golden ${g} has no cell ${cell}`);
    expect(observed).toEqual(want[cell]);
}

/** Remove stale per-fixture cell goldens before an UPDATE run so dropped cells do not linger. */
export function resetGoldenDir(dir: string): void {
    if (UPDATE) fs.rmSync(dir, { recursive: true, force: true });
}

export function findFiles(dir: string, name: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const p = path.join(dir, e.name);
        return e.isDirectory() ? findFiles(p, name) : e.name === name ? [p] : [];
    }).sort();
}

/** apiVersion boundary cell label + option value (`omitted` leaves the key out). */
export type ApiCell = number | 'omitted';
export const apiLabel = (v: ApiCell) => (v === 'omitted' ? 'omitted' : `v${v}`);
export const apiOpt = (v: ApiCell) => (v === 'omitted' ? {} : { apiVersion: v });
