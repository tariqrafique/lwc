import { defineConfig } from 'vitest/config';
const oracle = process.env.LWC_ORACLE;
if (!oracle) throw new Error('LWC_ORACLE is unset; export it via harness/scripts/oracle-root.mjs');

export default defineConfig({
  resolve: {
    // Static imports use the `/workspace/lwc` Vite alias, remapped to LWC_ORACLE (D-0280(b)).
    alias: [{ find: '/workspace/lwc', replacement: oracle }],
  }, test: { include: ['test/**/*.test.ts'] } });
