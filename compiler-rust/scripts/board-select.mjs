#!/usr/bin/env node
// D-0282: path→rows selection and per-row reuse hashes.
// Usage:
//   board-select.mjs select <repo> [<file> ...]
//   board-select.mjs crates-for-row <repo> <row>
//   board-select.mjs hash <repo> <row> [--oracle-sha SHA] [--pretend-edit PATH] [--git-ref REF] [--legacy]
//   board-select.mjs hash-all <repo> [--oracle-sha SHA] [--git-ref REF] [--legacy]
//   board-select.mjs check-map <repo>
//   board-select.mjs parse <repo>          (dump parsed map, for tests)
// Env: BOARD_SELECT_MAP overrides the TSV path (default: <repo>/harness/board-select.tsv).
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

function die(msg, rc = 2) {
  console.error(`board-select: ${msg}`);
  process.exit(rc);
}

function repoRoot(arg) {
  const root = resolve(arg);
  if (!existsSync(join(root, "Cargo.toml"))) die(`not a rust workspace: ${root}`);
  return root;
}

function mapPath(repo) {
  return process.env.BOARD_SELECT_MAP || join(repo, "harness/board-select.tsv");
}

function parseMap(file) {
  const text = readFileSync(file, "utf8");
  const paths = [];
  const rows = new Map();
  for (const raw of text.split(/\n/)) {
    const line = raw.replace(/\r$/, "");
    if (!line || line.startsWith("#")) continue;
    const cols = line.split("\t");
    if (cols.length !== 3) die(`malformed map line (want 3 columns): ${line}`, 1);
    const [kind, key, value] = cols;
    if (!key || !value) die(`empty key/value: ${line}`, 1);
    if (kind === "path") {
      const rowsOrFull = value === "FULL" ? "FULL" : value.split(",").filter(Boolean);
      if (rowsOrFull !== "FULL" && rowsOrFull.length === 0) die(`empty row list: ${line}`, 1);
      paths.push({ prefix: key, rows: rowsOrFull });
    } else if (kind === "row") {
      if (rows.has(key)) die(`duplicate row mapping: ${key}`, 1);
      rows.set(key, value);
    } else {
      die(`unknown kind ${kind}: ${line}`, 1);
    }
  }
  return { paths, rows };
}

function matchPath(paths, file) {
  const hits = [];
  for (const p of paths) {
    if (file === p.prefix || file.startsWith(p.prefix)) hits.push(p);
  }
  return hits;
}

function selectRows(map, files) {
  const out = new Set();
  let full = false;
  const unmatched = [];
  for (const file of files) {
    if (!file) continue;
    const hits = matchPath(map.paths, file);
    if (hits.length === 0) {
      unmatched.push(file);
      full = true;
      continue;
    }
    for (const h of hits) {
      if (h.rows === "FULL") full = true;
      else for (const r of h.rows) out.add(r);
    }
  }
  if (full) return { scope: "FULL", rows: [], unmatched };
  return { scope: "scoped", rows: [...out].sort(), unmatched: [] };
}

function runCargoMetadata(repo) {
  const r = spawnSync(
    "cargo",
    [
      "metadata",
      "--manifest-path",
      join(repo, "Cargo.toml"),
      "--locked",
      "--offline",
      "--format-version",
      "1",
      "--no-deps",
    ],
    { encoding: "utf8", cwd: repo, env: { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR || "/tmp/d0282-board-select-target" } },
  );
  if (r.status !== 0) die(`cargo metadata failed: ${r.stderr || r.stdout}`, 2);
  return JSON.parse(r.stdout);
}

function workspacePackages(meta, repo) {
  const memberIds = new Set(meta.workspace_members);
  const byName = new Map();
  for (const p of meta.packages) {
    if (!memberIds.has(p.id)) continue;
    const manifestDir = dirname(p.manifest_path);
    const rel = relative(repo, manifestDir).split("\\").join("/");
    const deps = [];
    for (const d of p.dependencies || []) {
      const kind = d.kind == null || d.kind === "normal" ? "normal" : d.kind;
      if (kind !== "normal") continue;
      deps.push(d.name);
    }
    byName.set(p.name, { name: p.name, rel, deps });
  }
  return byName;
}

function closure(byName, root) {
  if (!byName.has(root)) die(`unknown workspace crate ${root}`, 1);
  const seen = new Set();
  const stack = [root];
  while (stack.length) {
    const n = stack.pop();
    if (seen.has(n)) continue;
    seen.add(n);
    const pkg = byName.get(n);
    if (!pkg) continue;
    for (const d of pkg.deps) {
      if (byName.has(d)) stack.push(d);
    }
  }
  return [...seen].sort();
}

function gitLsAll(repo, ref) {
  const args = ref
    ? ["-C", repo, "ls-tree", "-r", "--name-only", "-z", ref]
    : ["-C", repo, "ls-files", "-z"];
  const r = spawnSync("git", args, { encoding: "buffer" });
  if (r.status !== 0) die(`git ls-tree/ls-files (ref=${ref || "worktree"}) failed`, 2);
  return r.stdout.toString("utf8").split("\0").filter(Boolean);
}

function gitLsFiles(repo, dir, ref, cache) {
  if (cache) return cache.list(dir);
  const args = ref
    ? ["-C", repo, "ls-tree", "-r", "--name-only", "-z", ref, "--", dir]
    : ["-C", repo, "ls-files", "-z", "--", dir];
  const r = spawnSync("git", args, { encoding: "buffer" });
  if (r.status !== 0) die(`git ls-files ${dir} (ref=${ref || "worktree"}) failed`, 2);
  return r.stdout.toString("utf8").split("\0").filter(Boolean);
}

function fileBytes(repo, file, ref) {
  if (ref) {
    // Default maxBuffer is 1MiB; harness/board-baseline.tsv is larger and used to
    // ENOBUFS, which made --git-ref silently drop the file and disagree with
    // worktree content hashes. Read the full blob.
    const r = spawnSync("git", ["-C", repo, "show", `${ref}:${file}`], {
      encoding: "buffer",
      maxBuffer: 512 * 1024 * 1024,
    });
    if (r.status !== 0) return null;
    return r.stdout;
  }
  const abs = join(repo, file);
  if (!existsSync(abs)) return null;
  return readFileSync(abs);
}

function sha256Bytes(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

function sha256File(path) {
  return sha256Bytes(readFileSync(path));
}

function parseCatFileBatch(buf, files) {
  const out = new Map();
  let off = 0;
  for (const file of files) {
    const nl = buf.indexOf(0x0a, off);
    if (nl < 0) die(`git cat-file --batch: truncated header at ${file}`);
    const header = buf.slice(off, nl).toString("utf8");
    off = nl + 1;
    if (/\smissing$/.test(header)) {
      out.set(file, null);
      continue;
    }
    const parts = header.split(" ");
    const size = parseInt(parts[parts.length - 1], 10);
    if (!Number.isFinite(size) || size < 0) die(`git cat-file --batch: bad header '${header}'`);
    if (off + size > buf.length) die(`git cat-file --batch: truncated body at ${file}`);
    const content = buf.slice(off, off + size);
    off += size;
    if (off < buf.length && buf[off] === 0x0a) off += 1;
    out.set(file, sha256Bytes(content));
  }
  return out;
}

class FileHashCache {
  constructor(repo, ref, legacy) {
    this.repo = repo;
    this.ref = ref || null;
    this.legacy = !!legacy;
    this.digests = new Map();
    this.listing = null;
  }
  list(dir) {
    if (!this.listing) this.listing = gitLsAll(this.repo, this.ref);
    const prefix = dir.endsWith("/") ? dir : `${dir}/`;
    return this.listing.filter((f) => f === dir || f.startsWith(prefix));
  }
  has(file) {
    if (!this.listing) this.listing = gitLsAll(this.repo, this.ref);
    return this.listing.includes(file);
  }
  ensure(files) {
    const need = files.filter((f) => !this.digests.has(f));
    if (!need.length) return;
    if (!this.ref) {
      for (const f of need) {
        const abs = join(this.repo, f);
        this.digests.set(f, existsSync(abs) ? sha256File(abs) : null);
      }
      return;
    }
    if (this.legacy) {
      for (const f of need) {
        const buf = fileBytes(this.repo, f, this.ref);
        this.digests.set(f, buf ? sha256Bytes(buf) : null);
      }
      return;
    }
    const input = need.map((f) => `${this.ref}:${f}`).join("\n") + "\n";
    const r = spawnSync("git", ["-C", this.repo, "cat-file", "--batch"], {
      input,
      maxBuffer: 512 * 1024 * 1024,
    });
    if (r.status !== 0) die(`git cat-file --batch failed: ${r.stderr || ""}`);
    for (const [f, digest] of parseCatFileBatch(r.stdout, need)) this.digests.set(f, digest);
  }
  digest(file) {
    this.ensure([file]);
    return this.digests.get(file);
  }
}

function rowHash(repo, map, metaPkgs, row, opts) {
  const root = map.rows.get(row);
  if (!root) die(`no row→crate mapping for ${row}`, 1);
  const ref = opts.gitRef || null;
  const cache = opts.cache || new FileHashCache(repo, ref, opts.legacy);
  const crates = closure(metaPkgs, root);
  const files = new Set();
  for (const c of crates) {
    const pkg = metaPkgs.get(c);
    if (!pkg) continue;
    for (const f of cache.list(pkg.rel)) files.add(f);
  }
  for (const extra of ["Cargo.lock", "rust-toolchain.toml"]) {
    if (cache.has(extra)) files.add(extra);
  }
  const napi = metaPkgs.get("lwc-compiler-napi");
  if (napi) for (const f of cache.list(napi.rel)) files.add(f);
  for (const f of cache.list("harness")) files.add(f);
  const pretend = opts.pretendEdit || null;
  cache.ensure([...files]);
  const lines = [];
  for (const f of [...files].sort()) {
    let digest = cache.digest(f);
    if (!digest) continue;
    if (pretend && pretend === f) {
      let buf = fileBytes(repo, f, ref);
      if (!buf) continue;
      buf = Buffer.concat([buf, Buffer.from("\n# board-select pretend-edit\n")]);
      digest = sha256Bytes(buf);
    }
    lines.push(`${f}\t${digest}`);
  }
  const oracle = opts.oracleSha || process.env.BOARD_SELECT_ORACLE_SHA || "oracle-unset";
  lines.push(`oracle\t${oracle}`);
  lines.push(`crates\t${crates.join(",")}`);
  return { hash: sha256Bytes(Buffer.from(lines.join("\n") + "\n")), crates, files: [...files].sort() };
}

function checkMap(repo, map, metaPkgs) {
  const errors = [];
  for (const [name, pkg] of metaPkgs) {
    const want = pkg.rel.endsWith("/") ? pkg.rel : pkg.rel + "/";
    const hit = map.paths.some((p) => p.prefix === pkg.rel || p.prefix === want || p.prefix === `${pkg.rel}/`);
    if (!hit) errors.push(`crate ${name} (${pkg.rel}) is not covered by a path prefix`);
  }
  return errors;
}

function usage() {
  die("usage: board-select.mjs select|crates-for-row|hash|check-map|parse <repo> ...", 2);
}

const args = process.argv.slice(2);
const cmd = args.shift();
if (!cmd) usage();
const repo = repoRoot(args.shift() || die("missing <repo>"));
const map = parseMap(mapPath(repo));

if (cmd === "parse") {
  console.log(JSON.stringify({ paths: map.paths, rows: Object.fromEntries(map.rows) }, null, 2));
  process.exit(0);
}

if (cmd === "select") {
  const files = args;
  const res = selectRows(map, files);
  if (res.scope === "FULL") {
    console.log("FULL");
    if (res.unmatched.length) console.error(`board-select: unmapped path(s): ${res.unmatched.join(" ")}`);
  } else {
    console.log(res.rows.join("\n"));
  }
  process.exit(0);
}

if (cmd === "check-map") {
  const meta = runCargoMetadata(repo);
  const pkgs = workspacePackages(meta, repo);
  const errors = checkMap(repo, map, pkgs);
  if (errors.length) {
    for (const e of errors) console.error(`board-select: ${e}`);
    process.exit(1);
  }
  console.log(`ok   map covers ${pkgs.size} workspace crates`);
  process.exit(0);
}

if (cmd === "crates-for-row") {
  const row = args.shift() || die("missing <row>");
  const meta = runCargoMetadata(repo);
  const pkgs = workspacePackages(meta, repo);
  const root = map.rows.get(row);
  if (!root) die(`no row→crate mapping for ${row}`, 1);
  console.log(closure(pkgs, root).join("\n"));
  process.exit(0);
}

if (cmd === "hash" || cmd === "hash-all") {
  let row = "";
  if (cmd === "hash") row = args.shift() || die("missing <row>");
  let oracleSha = process.env.BOARD_SELECT_ORACLE_SHA || "";
  let pretendEdit = "";
  let gitRef = "";
  let legacy = false;
  while (args.length) {
    const a = args.shift();
    if (a === "--oracle-sha") oracleSha = args.shift() || die("missing --oracle-sha value");
    else if (a === "--pretend-edit") pretendEdit = args.shift() || die("missing --pretend-edit value");
    else if (a === "--git-ref") gitRef = args.shift() || die("missing --git-ref value");
    else if (a === "--legacy") legacy = true;
    else die(`unknown hash flag ${a}`);
  }
  const meta = runCargoMetadata(repo);
  const pkgs = workspacePackages(meta, repo);
  const cache = new FileHashCache(repo, gitRef, legacy);
  if (cmd === "hash") {
    const res = rowHash(repo, map, pkgs, row, { oracleSha, pretendEdit, gitRef, legacy, cache });
    console.log(res.hash);
  } else {
    for (const r of [...map.rows.keys()]) {
      const res = rowHash(repo, map, pkgs, r, { oracleSha, pretendEdit, gitRef, legacy, cache });
      console.log(`${r}\t${res.hash}`);
    }
  }
  process.exit(0);
}

usage();
