// Row `pretty-equal` (D-0292). The census runs only when the napi path is set,
// which board.sh does. Golden-gate records JS goldens and has no napi yet;
// this row has no goldens, so that recording returns without comparing.
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { runPrettyEqual } from '../src/prettyEqual';

function goldenRecord(): boolean {
    return !process.env.LWC_COMPILER_NAPI_PATH && process.cwd().includes('lwc-golden-gate');
}

describe('pretty-equal', () => {
    it('census', async () => {
        if (goldenRecord()) {
            expect(process.env.LWC_COMPILER_NAPI_PATH).toBeUndefined();
            return;
        }
        const report = await runPrettyEqual();
        const dest = process.env.PRETTY_EQUAL_SUMMARY;
        if (dest) {
            fs.mkdirSync(path.dirname(dest), { recursive: true });
            fs.writeFileSync(dest, report.text);
        }
        // The board cats this file. Also leave it on the vitest log.
        process.stdout.write(report.text);
        expect(report.failures, report.text).toEqual([]);
    });
});
