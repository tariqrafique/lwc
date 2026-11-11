// Component roots for the upstream-cmp and js-transform-corpus rows.
// A root is one coverage source. integration-types is included: it has four
// playground bundles whose default export extends LightningElement.
import path from 'node:path';
import type { BundleRoot } from './lbcBundle';
import { oracleRoot, styleCorpusRoot } from './oracleRoot';

export function upstreamComponentRoots(): BundleRoot[] {
    const oracle = oracleRoot();
    const pkg = (name: string) => path.join(oracle, 'packages/@lwc', name);
    return [
        { corpus: 'integration-wtr', leaf: pkg('integration-wtr') },
        { corpus: 'integration-wdio', leaf: pkg('integration-wdio') },
        { corpus: 'integration-types', leaf: pkg('integration-types') },
        { corpus: 'rollup-plugin', leaf: path.join(pkg('rollup-plugin'), 'src/__tests__/fixtures') },
        { corpus: 'perf-benchmarks-components', leaf: pkg('perf-benchmarks-components') },
        { corpus: 'engine-server', leaf: pkg('engine-server') },
        { corpus: 'playground', leaf: path.join(oracle, 'playground') },
    ];
}

/** lbc + lbc-recipes plus the upstream component roots. */
export function jsTransformRoots(): BundleRoot[] {
    const corpus = styleCorpusRoot();
    return [
        { corpus: 'lbc', leaf: path.join(corpus, 'lbc') },
        { corpus: 'lbc-recipes', leaf: path.join(corpus, 'lbc-recipes') },
        ...upstreamComponentRoots(),
    ];
}
