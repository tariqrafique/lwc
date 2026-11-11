// Resolution for the SSR exec runner.
// `lwc` and `@lwc/ssr-runtime` go to SD_SSR_RUNTIME (the absolute oracle entry the emitter pinned).
// `vitest` is an external of some fixture bundles; it resolves from the oracle checkout.
// TypeScript parents (the three barrel-lwc-exports fixtures) get a `.ts` suffix.
import path from 'node:path';
import { pathToFileURL } from 'node:url';

let vitestUrl;

function vitestHref() {
    if (vitestUrl) return vitestUrl;
    const root = process.env.LWC_ORACLE;
    if (!root) throw new Error('semdiff-ssr: LWC_ORACLE is unset');
    // createRequire resolves the CJS export. Fixture bundles are ESM (`import { expect } from 'vitest'`).
    const parent = pathToFileURL(path.join(root, 'package.json')).href;
    vitestUrl = import.meta.resolve('vitest', parent);
    return vitestUrl;
}

export async function resolve(spec, ctx, next) {
    const runtime = process.env.SD_SSR_RUNTIME;
    if ((spec === 'lwc' || spec === '@lwc/ssr-runtime') && runtime) {
        return { url: pathToFileURL(runtime).href, shortCircuit: true };
    }
    if (spec === 'vitest') return { url: vitestHref(), shortCircuit: true };
    const parent = ctx.parentURL ?? '';
    if (spec.startsWith('.') && /\.tsx?$/.test(parent) && !/\.[cm]?[jt]sx?$/.test(spec)) {
        return next(`${spec}.ts`, ctx);
    }
    return next(spec, ctx);
}
