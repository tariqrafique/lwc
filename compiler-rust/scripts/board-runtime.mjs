#!/usr/bin/env node
// D-0282 official-board scope, reuse, and cadence state.
//   decide <main-repo> <commit> <store>
//   record <store> --cold 0|1 --src SRC --commit SHA
//   set-carry-disabled <store> --reason TEXT
//   clear-carry-disabled <store>
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const STATE_NAME = "BOARD_RUNTIME_STATE.json";
const SIX_HOURS_MS = 6 * 60 * 60 * 1000;
const LANDS_BEFORE_COLD = 5;

function die(msg, rc = 2) {
  console.error(`board-runtime: ${msg}`);
  process.exit(rc);
}

function statePath(store) {
  return join(store, STATE_NAME);
}

function loadState(store) {
  const p = statePath(store);
  if (!existsSync(p)) {
    return {
      last_cold_at: null,
      last_cold_src: null,
      last_cold_commit: null,
      lands_since_cold: 0,
      carry_disabled: false,
      carry_disabled_reason: "",
    };
  }
  return JSON.parse(readFileSync(p, "utf8"));
}

function saveState(store, st) {
  writeFileSync(statePath(store), JSON.stringify(st, null, 2) + "\n");
}

function parseOfficialBoardTxt(file) {
  const out = {};
  if (!existsSync(file)) return out;
  for (const line of readFileSync(file, "utf8").split(/\n/)) {
    const m = line.match(/^([A-Za-z0-9_]+)=(.*)$/);
    if (m) out[m[1]] = m[2];
  }
  return out;
}

function latestValid(store) {
  let best = null;
  let bestTs = "";
  if (!existsSync(store)) return null;
  for (const name of readdirSync(store)) {
    if (name === "invalid" || name === "breakglass" || name === STATE_NAME) continue;
    const meta = join(store, name, "OFFICIAL_BOARD.txt");
    const rec = parseOfficialBoardTxt(meta);
    if (rec.status !== "VALID") continue;
    const ts = rec.published || "";
    if (!best || ts > bestTs) {
      best = { src: name, commit: rec.commit || "", published: ts };
      bestTs = ts;
    }
  }
  return best;
}

function gitDiff(repo, a, b) {
  const r = spawnSync("git", ["-C", repo, "diff", "--name-only", a, b], { encoding: "utf8" });
  if (r.status !== 0) die(`git diff ${a} ${b} failed: ${r.stderr}`);
  return r.stdout.split(/\n/).filter(Boolean);
}

function runSelect(repo, files) {
  const r = spawnSync("node", [join(HERE, "board-select.mjs"), "select", repo, ...files], {
    encoding: "utf8",
  });
  if (r.status !== 0 && r.status !== 1) die(`board-select failed: ${r.stderr || r.stdout}`);
  const line = (r.stdout || "").trim();
  if (line === "FULL") return { scope: "FULL", rows: [] };
  return { scope: "scoped", rows: line ? line.split(/\n/).filter(Boolean) : [] };
}

function boardRows(repo) {
  const text = readFileSync(join(repo, "harness/board-rows.sh"), "utf8");
  const m = text.match(/^ROWS=\((.*)\)/m);
  if (!m) die("cannot parse ROWS from harness/board-rows.sh");
  return m[1].trim().split(/\s+/).filter(Boolean);
}

function rowHash(repo, row, ref, oracleSha) {
  const args = [join(HERE, "board-select.mjs"), "hash", repo, row, "--oracle-sha", oracleSha];
  if (ref) args.push("--git-ref", ref);
  const r = spawnSync("node", args, { encoding: "utf8" });
  if (r.status !== 0) {
    return null; // unknown/unhashable → fail closed (rerun)
  }
  return (r.stdout || "").trim();
}

function hashAll(repo, ref, oracleSha) {
  const args = [join(HERE, "board-select.mjs"), "hash-all", repo, "--oracle-sha", oracleSha];
  if (ref) args.push("--git-ref", ref);
  const r = spawnSync("node", args, { encoding: "utf8" });
  if (r.status !== 0) die(`board-select hash-all failed: ${r.stderr || r.stdout}`);
  const out = new Map();
  for (const line of (r.stdout || "").split(/\n/)) {
    if (!line) continue;
    const tab = line.indexOf("\t");
    if (tab < 0) continue;
    out.set(line.slice(0, tab), line.slice(tab + 1));
  }
  return out;
}

function decide(repo, commit, store) {
  const st = loadState(store);
  const prev = latestValid(store);
  const now = Date.now();
  const lastCold = st.last_cold_at ? Date.parse(st.last_cold_at) : NaN;
  const envCold =
    process.env.BOARD_COLD === "1" ||
    process.env.OFFICIAL_COLD === "1" ||
    process.env.BOARD_MILESTONE === "1";
  const ageCold = Number.isFinite(lastCold) && now - lastCold >= SIX_HOURS_MS;
  const countCold = (st.lands_since_cold || 0) >= LANDS_BEFORE_COLD;
  const carryDisabled = !!st.carry_disabled;
  const noPrev = !prev || !prev.commit;
  const cold = !!(envCold || ageCold || countCold || carryDisabled || noPrev || !st.last_cold_at);
  const oracleSha = (
    process.env.BOARD_SELECT_ORACLE_SHA ||
    process.env.OFFICIAL_ORACLE_SHA ||
    ""
  ).trim();
  const allRows = boardRows(repo);

  let scope = "FULL";
  let run = allRows.slice();
  const carry = [];
  if (!cold && prev && prev.commit) {
    const files = gitDiff(repo, prev.commit, commit);
    const sel = runSelect(repo, files);
    if (sel.scope === "FULL") {
      scope = "FULL";
      run = allRows.slice();
    } else {
      const selected = new Set(sel.rows);
      const curHashes = hashAll(repo, commit, oracleSha || "oracle-unset");
      const oldHashes = hashAll(repo, prev.commit, oracleSha || "oracle-unset");
      run = [];
      for (const row of allRows) {
        if (selected.has(row)) {
          run.push(row);
          continue;
        }
        const cur = curHashes.get(row);
        const old = oldHashes.get(row);
        if (cur && old && cur === old) carry.push(row);
        else run.push(row); // fail closed
      }
      scope = run.length === allRows.length ? "FULL" : "scoped";
    }
  }

  const golden = cold || scope === "FULL" ? 1 : 0;
  const shq = (s) => "'" + String(s).replace(/'/g, "'\\''") + "'";
  console.log(`BOARD_SCOPE=${shq(scope)}`);
  console.log(`BOARD_COLD=${shq(cold ? 1 : 0)}`);
  console.log(`BOARD_GOLDEN_GATE=${shq(golden)}`);
  console.log(`BOARD_ROWS=${shq(run.join(" "))}`);
  console.log(`BOARD_CARRY_ROWS=${shq(carry.join(" "))}`);
  console.log(`BOARD_PREV_SRC=${shq(prev ? prev.src : "")}`);
  console.log(`BOARD_PREV_COMMIT=${shq(prev ? prev.commit : "")}`);
  console.log(`BOARD_CARRY_DISABLED=${shq(carryDisabled ? 1 : 0)}`);
  console.log(`BOARD_LANDS_SINCE_COLD=${shq(st.lands_since_cold || 0)}`);
}

function record(store, args) {
  let cold = "0", src = "", commit = "";
  while (args.length) {
    const a = args.shift();
    if (a === "--cold") cold = args.shift();
    else if (a === "--src") src = args.shift();
    else if (a === "--commit") commit = args.shift();
    else die(`unknown record flag ${a}`);
  }
  const st = loadState(store);
  if (cold === "1") {
    st.last_cold_at = new Date().toISOString();
    st.last_cold_src = src;
    st.last_cold_commit = commit;
    st.lands_since_cold = 0;
  } else {
    st.lands_since_cold = (st.lands_since_cold || 0) + 1;
  }
  saveState(store, st);
  console.log(`board-runtime: recorded cold=${cold} src=${src} lands_since_cold=${st.lands_since_cold}`);
}


function extractLine(text, row) {
  const re = new RegExp("^" + row.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "(?:\\s|$)");
  for (const line of text.split(/\n/)) {
    if (re.test(line)) return line;
  }
  return null;
}

function tailFromNapi(text) {
  const idx = text.indexOf("napi sha256:");
  return idx >= 0 ? text.slice(idx) : text;
}

function compose(thisLogPath, prevTxt, outPath, ranList, repo) {
  const thisLog = readFileSync(thisLogPath, "utf8");
  const prev = existsSync(prevTxt) ? readFileSync(prevTxt, "utf8") : "";
  const idx = thisLog.indexOf("napi sha256:");
  const prefix = idx >= 0 ? thisLog.slice(0, idx) : thisLog.endsWith("\n") ? thisLog : thisLog + "\n";
  const thisTail = idx >= 0 ? thisLog.slice(idx) : "";
  const prevTail = tailFromNapi(prev);
  const header = (thisTail.split(/\n/).find((l) => l.startsWith("napi sha256:")) || prevTail.split(/\n/).find((l) => l.startsWith("napi sha256:")) || "");
  const ran = new Set((ranList || "").split(/\s+/).filter(Boolean));
  const rows = boardRows(repo);
  const out = [prefix + header];
  for (const row of rows) {
    const line = ran.has(row) ? extractLine(thisTail, row) : extractLine(prevTail, row) || extractLine(thisTail, row);
    if (line) out.push(line);
    if (row === "ssr-render-async") {
      const klThis = thisTail.split(/\n/).find((l) => l.startsWith("known-limitation:"));
      const klPrev = prevTail.split(/\n/).find((l) => l.startsWith("known-limitation:"));
      const kl = ran.has(row) ? klThis : klPrev || klThis;
      if (kl) out.push(kl);
    }
  }
  const extras = ["semantic-diff-ssr", "semantic-diff", "shim-check:", "shim-registry:", "ssr-compile-js-scanners:", "option-gate:", "coverage-gate:", "golden-io-guard:"];
  const linesOf = (text) => text.split(/\n/);
  const matchExtra = (text, key) => {
    for (const line of linesOf(text)) {
      if (!line.startsWith(key)) continue;
      // `semantic-diff` is a prefix of `semantic-diff-ssr`; keep the H-1 line on its own key.
      if (key === "semantic-diff" && line.startsWith("semantic-diff-ssr")) continue;
      return line;
    }
    return undefined;
  };
  for (const key of extras) {
    const fromThis = matchExtra(thisTail, key);
    const fromPrev = matchExtra(prevTail, key);
    // gates always run on this board.sh invocation (even with zero suites)
    const line = fromThis || fromPrev;
    if (line) out.push(line);
  }
  for (const line of thisTail.split(/\n/)) {
    if (line.startsWith("style-parse-errors") || line.startsWith("style-corpus") || line.startsWith("napi-css-errors") || line.startsWith("csr-clusters[")) out.push(line);
  }
  const body = out.join("\n").replace(/\n+$/, "") + "\n";
  writeFileSync(outPath, body);
}

function checkAgreement(thisLogPath, prevTxt, repo, commit, prevCommit, oracleSha) {
  const thisTail = tailFromNapi(readFileSync(thisLogPath, "utf8"));
  const prevTail = tailFromNapi(readFileSync(prevTxt, "utf8"));
  const rows = boardRows(repo);
  const bad = [];
  const curHashes = hashAll(repo, commit, oracleSha || "oracle-unset");
  const oldHashes = hashAll(repo, prevCommit, oracleSha || "oracle-unset");
  for (const row of rows) {
    const cur = curHashes.get(row);
    const old = oldHashes.get(row);
    if (!cur || !old || cur !== old) continue; // would not have been carried
    const a = extractLine(thisTail, row);
    const b = extractLine(prevTail, row);
    if (a !== b) bad.push({ row, current: a, previous: b });
  }
  if (bad.length) {
    for (const b of bad) {
      console.error(`board-runtime: carry disagreement on ${b.row}`);
      console.error(`  current:  ${b.current}`);
      console.error(`  previous: ${b.previous}`);
    }
    process.exit(1);
  }
  console.log("board-runtime: carry agreement ok");
}

const args = process.argv.slice(2);
const cmd = args.shift();
if (cmd === "decide") {
  const repo = args.shift();
  const commit = args.shift();
  const store = args.shift();
  if (!repo || !commit || !store) die("usage: decide <main-repo> <commit> <store>");
  decide(repo, commit, store);
} else if (cmd === "record") {
  const store = args.shift() || die("usage: record <store> --cold 0|1 --src SRC --commit SHA");
  record(store, args);
} else if (cmd === "set-carry-disabled") {
  const store = args.shift() || die("missing store");
  let reason = "";
  if (args[0] === "--reason") reason = args[1] || "";
  const st = loadState(store);
  st.carry_disabled = true;
  st.carry_disabled_reason = reason;
  saveState(store, st);
  console.log("board-runtime: carry disabled");
} else if (cmd === "clear-carry-disabled") {
  const store = args.shift() || die("missing store");
  const st = loadState(store);
  st.carry_disabled = false;
  st.carry_disabled_reason = "";
  saveState(store, st);
  console.log("board-runtime: carry enabled");
} else if (cmd === "compose") {
  const thisLog = args.shift();
  const prevTxt = args.shift();
  const outPath = args.shift();
  const repo = args.shift() || die("usage: compose <this-log> <prev-board.txt> <out-log> <repo> [--ran ROWS]");
  let ran = "";
  if (args[0] === "--ran") ran = args[1] || "";
  compose(thisLog, prevTxt, outPath, ran, repo);
} else if (cmd === "check-carry-agreement") {
  const thisLog = args.shift();
  const prevTxt = args.shift();
  const repo = args.shift();
  const commit = args.shift();
  const prevCommit = args.shift();
  if (!thisLog || !prevTxt || !repo || !commit || !prevCommit) {
    die("usage: check-carry-agreement <this-log> <prev-board.txt> <repo> <commit> <prev-commit>");
  }
  checkAgreement(thisLog, prevTxt, repo, commit, prevCommit, process.env.BOARD_SELECT_ORACLE_SHA || "");
} else {
  die("usage: board-runtime.mjs decide|record|set-carry-disabled|clear-carry-disabled|compose|check-carry-agreement ...");
}
