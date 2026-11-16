#!/usr/bin/env node
// Item 15(a): test crates/lwc-compiler-napi/index.js platform resolution on Linux by simulating
// process.platform / process.arch / glibc in a child node, plus one real load of a .node.
// usage: node scripts/test-napi-loader-resolution.mjs [--real-node PATH]   (exit 0 = all pass)
// It never edits index.js: each case copies it into a scratch package dir.
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pkgDir = path.join(root, 'crates/lwc-compiler-napi');
const loaderSrc = fs.readFileSync(path.join(pkgDir, 'index.js'), 'utf8');
const pkgJson = JSON.parse(fs.readFileSync(path.join(pkgDir, 'package.json'), 'utf8'));
const realIdx = process.argv.indexOf('--real-node');
const realNode = realIdx > 0 ? path.resolve(process.argv[realIdx + 1]) : null;

let failures = 0;
const ok = (cond, msg) => {
    console.log(`${cond ? 'ok  ' : 'FAIL'} ${msg}`);
    if (!cond) failures++;
};

// --- single source: napi-triples.tsv == index.js SUPPORTED == optionalDependencies == napi.triples
const rows = fs
    .readFileSync(path.join(root, 'scripts/napi-triples.tsv'), 'utf8')
    .split('\n')
    .filter((l) => l && !l.startsWith('#'))
    .map((l) => l.split('\t'));
const shipped = rows.filter((r) => r[7] === 'shipped');
const tsvTriples = shipped.map((r) => r[0]).sort();
const supported = JSON.parse(
    (loaderSrc.match(/const SUPPORTED = (\[[^\]]*\])/) ?? [, '[]'])[1].replace(/'/g, '"'),
).sort();
const optDeps = Object.keys(pkgJson.optionalDependencies ?? {})
    .map((k) => k.replace(/^@lwc\/rust-compiler-/, ''))
    .sort();
const napiTriples = [...(pkgJson.napi?.triples?.additional ?? [])].sort();
ok(JSON.stringify(tsvTriples) === JSON.stringify(supported), `tsv shipped rows == index.js SUPPORTED (${supported.join(',')})`);
ok(JSON.stringify(tsvTriples) === JSON.stringify(optDeps), 'tsv shipped rows == package.json optionalDependencies');
ok(tsvTriples.includes('win32-arm64-msvc'), 'win32-arm64-msvc is shipped (D-0290)');
ok(
    JSON.stringify(shipped.map((r) => r[1]).sort()) === JSON.stringify(napiTriples),
    'tsv shipped rust targets == package.json napi.triples.additional',
);
for (const v of Object.values(pkgJson.optionalDependencies ?? {})) {
    ok(v === pkgJson.version, `optionalDependency pinned to exact version ${v}`);
}

// --- simulated platforms
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'napi-loader-'));
const preload = path.join(scratch, 'sim.mjs');
fs.writeFileSync(
    preload,
    `const s = JSON.parse(process.env.SIM);
Object.defineProperty(process, 'platform', { value: s.platform, configurable: true });
Object.defineProperty(process, 'arch', { value: s.arch, configurable: true });
const header = s.glibc ? { glibcVersionRuntime: s.glibc } : {};
Object.defineProperty(process, 'report', { value: { getReport: () => ({ header }) }, configurable: true });
`,
);
const fakeBinding = (t) =>
    `const r = () => ({ code: 'fake:${t}' });\nmodule.exports = { transform: r, transformSync: r };\n`;

function makeCase(name, { pkgs = [], sibling = null }) {
    const dir = path.join(scratch, name);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'index.js'), loaderSrc);
    // The rest of the shipped main-package files (wrap.js since AB-0008 r3), straight from package.json `files`.
    for (const f of pkgJson.files ?? []) if (f !== 'index.js') fs.copyFileSync(path.join(pkgDir, f), path.join(dir, f));
    fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ type: 'module' }));
    for (const t of pkgs) {
        const p = path.join(dir, 'node_modules', `@lwc/rust-compiler-${t}`);
        fs.mkdirSync(p, { recursive: true });
        fs.writeFileSync(path.join(p, 'package.json'), JSON.stringify({ name: `@lwc/rust-compiler-${t}`, main: 'fake.cjs' }));
        fs.writeFileSync(path.join(p, 'fake.cjs'), fakeBinding(t));
    }
    if (sibling?.file) fs.copyFileSync(sibling.file, path.join(dir, `lwc-compiler.${sibling.triple}.node`));
    if (sibling?.garbage) fs.writeFileSync(path.join(dir, `lwc-compiler.${sibling.triple}.node`), 'not a binary');
    return dir;
}
const runner = `
const m = await import(process.argv[1]);
const fn = m.transformSync ?? m.transform;
try { const r = fn(process.argv[2], process.argv[3], { name: 'foo', namespace: 'x' }); console.log(JSON.stringify({ code: String(r.code).slice(0, 40) })); }
catch (e) { console.log(JSON.stringify({ compileError: String(e.message) })); }
`;
function run(dir, sim, src = '<template><p>{x}</p></template>', file = 'foo.html') {
    const r = spawnSync(
        process.execPath,
        ['--import', preload, '--input-type=module', '-e', runner, path.join(dir, 'index.js'), src, file],
        { env: { ...process.env, SIM: JSON.stringify(sim) }, encoding: 'utf8' },
    );
    return { status: r.status, out: r.stdout.trim(), err: r.stderr };
}
const sims = {
    'linux-x64-gnu': { platform: 'linux', arch: 'x64', glibc: '2.28' },
    'linux-arm64-gnu': { platform: 'linux', arch: 'arm64', glibc: '2.28' },
    'darwin-x64': { platform: 'darwin', arch: 'x64' },
    'darwin-arm64': { platform: 'darwin', arch: 'arm64' },
    'win32-x64-msvc': { platform: 'win32', arch: 'x64' },
    'win32-arm64-msvc': { platform: 'win32', arch: 'arm64' },
};
ok(JSON.stringify(Object.keys(sims).sort()) === JSON.stringify(tsvTriples), 'simulation covers every shipped triple');

// 1. every shipped triple resolves its own platform package, and only that one
const allPkgs = Object.keys(sims);
for (const [t, sim] of Object.entries(sims)) {
    const r = run(makeCase(`pkg-${t}`, { pkgs: allPkgs }), sim);
    ok(r.status === 0 && r.out === JSON.stringify({ code: `fake:${t}` }), `${t}: loads @lwc/rust-compiler-${t} (${r.out || r.err.split('\n')[0]})`);
}
// 2. missing platform package -> the documented error, naming the triple and listing SUPPORTED
for (const [t, sim] of Object.entries(sims)) {
    const others = allPkgs.filter((x) => x !== t);
    const r = run(makeCase(`miss-${t}`, { pkgs: others }), sim);
    ok(
        r.status !== 0 &&
            r.err.includes(`@lwc/rust-compiler: no native binary for ${t}.`) &&
            r.err.includes(`@lwc/rust-compiler-${t}: MODULE_NOT_FOUND`) &&
            r.err.includes('Reinstall without --omit=optional'),
        `${t}: missing package fails with the no-native-binary message`,
    );
}
// 3. unsupported hosts fail closed (no fallback to another triple's package)
const unsupported = {
    'linux-x64-musl': { platform: 'linux', arch: 'x64', glibc: null },
    'linux-arm64-musl': { platform: 'linux', arch: 'arm64', glibc: null },
    'win32-ia32-msvc': { platform: 'win32', arch: 'ia32' },
    'freebsd-x64': { platform: 'freebsd', arch: 'x64' },
    'linux-ppc64-gnu': { platform: 'linux', arch: 'ppc64', glibc: '2.28' },
};
for (const [t, sim] of Object.entries(unsupported)) {
    const r = run(makeCase(`unsup-${t}`, { pkgs: allPkgs }), sim);
    ok(r.status !== 0 && r.err.includes(`no native binary for ${t}.`), `${t}: unsupported host fails closed`);
}
// 4. a broken sibling .node falls through to the package, and both causes are reported when both fail
{
    const r = run(makeCase('garbage-sibling', { pkgs: ['darwin-arm64'], sibling: { triple: 'darwin-arm64', garbage: true } }), sims['darwin-arm64']);
    ok(r.status === 0 && r.out === JSON.stringify({ code: 'fake:darwin-arm64' }), 'darwin-arm64: unloadable sibling .node falls through to the package');
    const r2 = run(makeCase('garbage-only', { sibling: { triple: 'win32-x64-msvc', garbage: true } }), sims['win32-x64-msvc']);
    ok(
        r2.status !== 0 && r2.err.includes('lwc-compiler.win32-x64-msvc.node: ERR_DLOPEN_FAILED') && r2.err.includes('@lwc/rust-compiler-win32-x64-msvc: MODULE_NOT_FOUND'),
        'win32-x64-msvc: both tried locations appear in the error',
    );
}
// 5. real binary: sibling wins over the package, under a simulated foreign platform name and natively
if (realNode) {
    const sim = { platform: 'darwin', arch: 'arm64' };
    const r = run(makeCase('real-sibling', { pkgs: ['darwin-arm64'], sibling: { triple: 'darwin-arm64', file: realNode } }), sim);
    ok(r.status === 0 && !r.out.includes('fake:') && /"code":"/.test(r.out), `sibling .node is preferred over the package (real compile: ${r.out})`);
    const host = { platform: 'linux', arch: 'x64', glibc: '2.41' };
    const r2 = run(makeCase('real-host', { sibling: { triple: 'linux-x64-gnu', file: realNode } }), host, 'export default class {}', 'foo.js');
    ok(r2.status === 0 && /"code":"/.test(r2.out), `real linux-x64-gnu .node compiles a .js through the loader (${r2.out})`);
}
fs.rmSync(scratch, { recursive: true, force: true });
console.log(failures ? `${failures} FAILED` : 'all passed');
process.exit(failures ? 1 : 0);
