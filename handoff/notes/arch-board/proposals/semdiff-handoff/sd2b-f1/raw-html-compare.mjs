// Research-only: raw (no formatHTML) sync HTML for fixture bundles not printer-equal.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { pathToFileURL } from 'node:url';

const out = '/workspace/scratch/semdiff/sd2b-premise/f1-bundles';
const fixturesRoot = '/workspace/lwc/packages/@lwc/ssr-compiler/src/__tests__/fixtures';
const rewrite = JSON.parse(fs.readFileSync(path.join(out, 'bundles-rewrite.json'), 'utf8'));
const SSR_RUNTIME = '/workspace/lwc/packages/@lwc/ssr-runtime/dist/index.js';
const runtime = await import(pathToFileURL(SSR_RUNTIME).href);
try { runtime.setHooks({ sanitizeHtmlContent: (c) => String(c) }); } catch {}

const notPe = rewrite.cases.filter((c) => c.verdict.reason !== 'printer-equal');
const relOf = (id) => id.split('/').slice(1, -2).join('/');
const sha = (s) => crypto.createHash('sha256').update(s).digest('hex');
let nonce = 0;

async function render(bundlePath, props, styleDedupe) {
  const mod = await import(pathToFileURL(bundlePath).href + `?n=${nonce++}`);
  const html = await runtime.serverSideRenderComponent(
    'fixture-test', mod.default, props ?? {}, styleDedupe ?? true, 'sync');
  return typeof html === 'string' ? html : String(html);
}

const rows = [];
let html_differ = 0, html_equal = 0, throw_one = 0, throw_both = 0;
for (const c of notPe) {
  const rel = relOf(c.id);
  const config = JSON.parse(fs.readFileSync(path.join(fixturesRoot, rel, 'config.json'), 'utf8'));
  const jsPath = path.join(out, 'js-sync-dist', rel, 'compiled-ssr.mjs');
  const rsPath = path.join(out, 'rust-sync-dist', rel, 'compiled-ssr.mjs');
  let jsHtml = null, rsHtml = null, jsErr = null, rsErr = null;
  try { jsHtml = await render(jsPath, config.props, config.styleDedupe); }
  catch (e) { jsErr = String(e?.message ?? e); }
  try { rsHtml = await render(rsPath, config.props, config.styleDedupe); }
  catch (e) { rsErr = String(e?.message ?? e); }
  let kind;
  if (jsErr && rsErr) { kind = 'throw-both'; throw_both++; }
  else if (jsErr || rsErr) { kind = 'throw-one-side'; throw_one++; }
  else if (jsHtml === rsHtml) { kind = 'html-byte-equal'; html_equal++; }
  else { kind = 'html-byte-differ'; html_differ++; }
  rows.push({
    rel, reason: c.verdict.reason, kind, js_err: jsErr, rs_err: rsErr,
    js_sha: jsHtml != null ? sha(jsHtml) : null,
    rs_sha: rsHtml != null ? sha(rsHtml) : null,
    js_len: jsHtml?.length ?? null, rs_len: rsHtml?.length ?? null,
  });
}
const summary = {
  not_printer_equal: notPe.length,
  html_byte_differ: html_differ,
  html_byte_equal: html_equal,
  throw_one_side: throw_one,
  throw_both: throw_both,
  mode: 'sync',
  formatHTML: false,
  napi_sha256: 'c4cd4cb09eac1b1a1dd89a4e096ceb781980b86e2fe19f3c069afe69b1fccae2',
  oracle: 'b9e8007',
};
fs.writeFileSync(path.join(out, 'raw-html-summary.json'), JSON.stringify({ summary, rows }, null, 1) + '\n');
console.log(JSON.stringify(summary, null, 2));
