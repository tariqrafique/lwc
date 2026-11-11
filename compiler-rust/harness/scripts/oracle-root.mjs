// Single resolver for LWC_ORACLE / STYLE_CORPUS / notes decisions (AB-M0002).
// The .ts module re-exports these functions; the .sh wrappers invoke this file.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const BOX_DEFAULT = '/workspace/lwc';
const BOX_NOTES = '/workspace/notes/lwc-rust-migrate/decisions.tsv';
export const ORACLE_PATH_TOKEN = '<ORACLE_ROOT>';

export function oracleRoot() {
  const raw = process.env.LWC_ORACLE;
  if (raw !== undefined && raw !== '') {
    const p = path.resolve(raw);
    if (!fs.existsSync(path.join(p, 'package.json'))) {
      throw new Error(`LWC_ORACLE=${p} is not an lwc checkout (missing package.json)`);
    }
    return p;
  }
  if (fs.existsSync(path.join(BOX_DEFAULT, 'package.json'))) return BOX_DEFAULT;
  throw new Error('LWC_ORACLE is unset and /workspace/lwc is absent; run scripts/fetch-oracle.sh');
}

export function styleCorpusRoot() {
  const raw = process.env.STYLE_CORPUS;
  if (raw !== undefined && raw !== '') {
    const p = path.resolve(raw);
    if (!fs.existsSync(p)) throw new Error(`STYLE_CORPUS=${p} does not exist`);
    return p;
  }
  if (fs.existsSync('/workspace/style-corpus')) return '/workspace/style-corpus';
  throw new Error('STYLE_CORPUS is unset and /workspace/style-corpus is absent; run scripts/fetch-style-corpus.sh');
}

/** Rewrite the real oracle prefix to <ORACLE_ROOT>, a token real output cannot contain. */
export function canonOraclePath(s) {
  const root = oracleRoot();
  return s.split(root).join(ORACLE_PATH_TOKEN);
}

export function notesDecisionsPath() {
  if (process.env.OPTGATE_DECISIONS) return path.resolve(process.env.OPTGATE_DECISIONS);
  if (process.env.SEMDIFF_REGISTRY) return path.resolve(process.env.SEMDIFF_REGISTRY);
  if (fs.existsSync(BOX_NOTES)) return BOX_NOTES;
  const snap = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../decisions.snapshot.tsv');
  if (fs.existsSync(snap)) return snap;
  throw new Error('OPTGATE_DECISIONS/SEMDIFF_REGISTRY unset, /workspace/notes/.../decisions.tsv absent, and harness/decisions.snapshot.tsv missing');
}

export function oraclePin(pinFile) {
  if (!pinFile) throw new Error('oracle_pin: need path to harness/oracle.pin');
  const lines = fs.readFileSync(pinFile, 'utf8').split(/\r?\n/);
  const pin = lines.find((l) => !/^\s*(#|$)/.test(l));
  if (!pin || !/^[0-9a-f]{40}$/.test(pin)) {
    throw new Error(`oracle.pin is not a 40-hex sha: '${pin ?? ''}'`);
  }
  return pin;
}

const here = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === here) {
  const cmd = process.argv[2];
  const rest = process.argv.slice(3);
  try {
    let out;
    switch (cmd) {
      case 'oracle_root':
        out = oracleRoot();
        break;
      case 'style_corpus_root':
        out = styleCorpusRoot();
        break;
      case 'notes_decisions_path':
        out = notesDecisionsPath();
        break;
      case 'oracle_pin':
        out = oraclePin(rest[0]);
        break;
      case 'canon_oracle_path':
        out = canonOraclePath(rest.join(' '));
        break;
      default:
        console.error('usage: oracle-root.mjs oracle_root|style_corpus_root|notes_decisions_path|oracle_pin <file>|canon_oracle_path <text>');
        process.exit(2);
    }
    process.stdout.write(`${out}\n`);
  } catch (e) {
    console.error(`!! ${e.message}`);
    process.exit(2);
  }
}
