import { defineConfig } from 'vitest/config';

// Pinned board config (AB-0006 r4): board.sh always passes `--config` this file from THIS
// script's tree (MAIN when land.sh runs MAIN's board.sh). No Vite plugins. setupFiles and
// globalSetup are only the closed lists below — a branch vitest.config.ts cannot inject either.
const SETUP_FILES: string[] = [];
const GLOBAL_SETUP: string[] = [];

export default defineConfig({
  // cwd is the tree-under-test's harness/ (board.sh cds there). Do not pin this
  // file's directory — that would run MAIN's tests when land uses MAIN's --config.
  root: process.cwd(),
  // Do not load a branch harness/.env (or any .env) into the vitest child.
  envDir: '/nonexistent-vitest-board-env-dir',
  plugins: [],
  test: {
    include: ['test/**/*.test.ts'],
    setupFiles: SETUP_FILES,
    globalSetup: GLOBAL_SETUP,
  },
});
