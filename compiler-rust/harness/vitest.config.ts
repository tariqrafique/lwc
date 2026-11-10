import { defineConfig } from 'vitest/config';
const oracle = process.env.LWC_ORACLE || '/workspace/lwc';

export default defineConfig({
  resolve: {
    // Static imports use the stable `/workspace/lwc` alias; remap to LWC_ORACLE when set (D-0280).
    alias: [{ find: '/workspace/lwc', replacement: oracle }],
  }, test: { include: ['test/**/*.test.ts'] } });
