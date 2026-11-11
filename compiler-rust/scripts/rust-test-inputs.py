#!/usr/bin/env python3
"""Derive repo paths that Rust tests and build.rs files read.

Scans crates/**/{src,tests}/**/*.rs, tools/**/{src,tests}/**/*.rs, and every
build.rs for include_str!/include_bytes!, .join("…") / path string literals,
Command::new("node"|…) argument paths, and CARGO_MANIFEST_DIR joins.
JS/TS files discovered that way have their relative imports followed.
"""
from __future__ import annotations

import argparse
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

JOIN_RE = re.compile(r"""\.join\(\s*(?:["']([^"']+)["'])\s*\)""")
INCLUDE_RE = re.compile(
    r"""include_(?:str|bytes)!\(\s*(?:["']([^"']+)["'])\s*\)"""
)
REPO_STR_RE = re.compile(
    r"""["']((?:\.\./)*(?:crates|tools|harness|scripts)/[^"']+)["']"""
)
REL_STR_RE = re.compile(r"""["']((?:\.\./)+[^"']+)["']""")
NODE_CMD_RE = re.compile(
    r"""Command::new\(\s*["'](?:node|npx|npm)["']\s*\)"""
)
JS_PATH_RE = re.compile(
    r"""["']([^"']+\.(?:mjs|js|cjs|ts|tsx))["']"""
)
JS_IMPORT_RE = re.compile(
    r"""(?:import|export)\s+(?:[^'"\n]+from\s+)?["'](\.[^"']+)["']|import\(\s*["'](\.[^"']+)["']\s*\)|require\(\s*["'](\.[^"']+)["']\s*\)"""
)
JS_REL_RE = re.compile(r"""["'](\./[^"']+)["']""")
BROAD_TOP = {"crates", "tools", "harness", "scripts", "src", "tests"}
SKIP_DIR_NAMES = {".git", "target", "node_modules", ".oracle", ".vitest"}


def rust_scan_files(root: Path) -> list[Path]:
    out: list[Path] = []
    for base in (root / "crates", root / "tools"):
        if not base.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIR_NAMES]
            rel = Path(dirpath).relative_to(root).as_posix()
            parts = rel.split("/")
            # crates/<pkg>/{src,tests}/... or tools/<pkg>/{src,tests}/...
            if len(parts) >= 3 and parts[2] in {"src", "tests"}:
                for name in filenames:
                    if name.endswith(".rs"):
                        out.append(Path(dirpath) / name)
            for name in filenames:
                if name == "build.rs":
                    p = Path(dirpath) / name
                    if p not in out:
                        out.append(p)
    return sorted(set(out))


def crate_root_and_pkg(root: Path, rust_file: Path) -> tuple[Path, str] | None:
    cur = rust_file.parent
    while cur != root and cur != cur.parent:
        manifest = cur / "Cargo.toml"
        if manifest.is_file():
            text = manifest.read_text(encoding="utf-8", errors="replace")
            m = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text)
            if not m:
                return None
            return cur, m.group(1)
        cur = cur.parent
    return None


def repo_rel(root: Path, path: Path) -> str | None:
    try:
        rel = path.resolve().relative_to(root.resolve())
    except ValueError:
        return None
    posix = rel.as_posix()
    if posix.startswith(".."):
        return None
    return posix


def add_path(root: Path, found: dict[str, set[str]], pkg: str, path: Path, follow_js: list[tuple[str, str]]) -> None:
    rel = repo_rel(root, path)
    if rel is None:
        return
    if path.is_dir():
        key = rel.rstrip("/") + "/"
        if key.rstrip("/").split("/")[0] in BROAD_TOP and key.count("/") == 1:
            return
        found[key].add(pkg)
        return
    if not path.is_file():
        return
    found[rel].add(pkg)
    if path.suffix in {".mjs", ".js", ".cjs", ".ts", ".tsx"}:
        follow_js.append((rel, pkg))


def try_resolve(root: Path, crate: Path, rust_file: Path, literal: str) -> Path | None:
    candidates = [
        (rust_file.parent / literal),
        (crate / literal),
        (root / literal),
    ]
    # strip leading ./
    lit = literal[2:] if literal.startswith("./") else literal
    if lit != literal:
        candidates.extend([rust_file.parent / lit, crate / lit, root / lit])
    for c in candidates:
        if c.exists():
            return c
    return None


def extract_from_rust(root: Path, rust_file: Path, crate: Path, pkg: str, found: dict[str, set[str]], follow_js: list[tuple[str, str]]) -> None:
    text = rust_file.read_text(encoding="utf-8", errors="replace")
    literals: set[str] = set()
    literals.update(INCLUDE_RE.findall(text))
    literals.update(JOIN_RE.findall(text))
    literals.update(REPO_STR_RE.findall(text))
    literals.update(REL_STR_RE.findall(text))
    if NODE_CMD_RE.search(text):
        literals.update(JS_PATH_RE.findall(text))
    for lit in literals:
        if not lit or lit.startswith("http") or "\n" in lit:
            continue
        resolved = try_resolve(root, crate, rust_file, lit)
        if resolved is None:
            continue
        add_path(root, found, pkg, resolved, follow_js)


def js_import_specs(text: str) -> list[str]:
    out = []
    for m in JS_IMPORT_RE.finditer(text):
        spec = m.group(1) or m.group(2) or m.group(3)
        if spec:
            out.append(spec)
    return out


def resolve_js_import(from_file: Path, spec: str) -> Path | None:
    base = (from_file.parent / spec).resolve()
    if base.is_file():
        return base
    for ext in ("", ".ts", ".tsx", ".mjs", ".js", ".cjs"):
        p = Path(str(base) + ext) if ext else base
        if p.is_file():
            return p
    for name in ("index.ts", "index.mjs", "index.js"):
        p = base / name
        if p.is_file():
            return p
    return None


def follow_js_imports(root: Path, found: dict[str, set[str]], seeds: list[tuple[str, str]]) -> None:
    queue = list(seeds)
    seen: set[tuple[str, str]] = set()
    while queue:
        rel, pkg = queue.pop()
        key = (rel, pkg)
        if key in seen:
            continue
        seen.add(key)
        path = root / rel
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        specs = js_import_specs(text) + JS_REL_RE.findall(text)
        for spec in specs:
            if not spec.startswith("."):
                continue
            nxt = resolve_js_import(path, spec)
            if nxt is None:
                continue
            nrel = repo_rel(root, nxt)
            if nrel is None:
                continue
            if nrel not in found or pkg not in found[nrel]:
                found[nrel].add(pkg)
                queue.append((nrel, pkg))


def scan(root: Path) -> dict[str, set[str]]:
    found: dict[str, set[str]] = defaultdict(set)
    follow_js: list[tuple[str, str]] = []
    for rust_file in rust_scan_files(root):
        info = crate_root_and_pkg(root, rust_file)
        if info is None:
            continue
        crate, pkg = info
        extract_from_rust(root, rust_file, crate, pkg, found, follow_js)
        text = rust_file.read_text(encoding="utf-8", errors="replace")
        if "CARGO_MANIFEST_DIR" in text or "cargo_manifest_dir" in text:
            crel = repo_rel(root, crate)
            if crel and crel not in BROAD_TOP:
                found[crel.rstrip("/") + "/"].add(pkg)
    follow_js_imports(root, found, follow_js)
    return found


def rows_from_found(found: dict[str, set[str]]) -> list[tuple[str, str]]:
    rows = []
    for path, pkgs in found.items():
        for pkg in sorted(pkgs):
            rows.append((path, pkg))
    rows.sort()
    return rows


def load_tsv(path: Path) -> list[tuple[str, str]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 2:
            raise SystemExit(f"malformed rust-test-inputs row: {line!r}")
        rows.append((parts[0], parts[1]))
    return rows


HEADER = """\
# path	package
# Generated by scripts/rust-test-inputs.py. Directory paths end with /.
# --check fails if a scan-found (path, package) row is missing from this file.
"""


def write_tsv(path: Path, rows: list[tuple[str, str]]) -> None:
    lines = [HEADER]
    for p, pkg in rows:
        lines.append(f"{p}\t{pkg}\n")
    path.write_text("".join(lines), encoding="utf-8")


def rust_or_cargo_changed(changed: list[str]) -> bool:
    for p in changed:
        if p.endswith(".rs"):
            return True
        if p in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}:
            return True
        if p.endswith("/Cargo.toml"):
            return True
    return False


def packages_for_changed(changed: list[str], rows: list[tuple[str, str]]) -> list[str]:
    pkgs: set[str] = set()
    for p in changed:
        for path, pkg in rows:
            if path.endswith("/"):
                if p.startswith(path):
                    pkgs.add(pkg)
            elif p == path:
                pkgs.add(pkg)
    return sorted(pkgs)


def decide(changed: list[str], rows: list[tuple[str, str]]) -> str:
    if rust_or_cargo_changed(changed):
        return "workspace"
    pkgs = packages_for_changed(changed, rows)
    if pkgs:
        return "packages " + " ".join(f"-p {p}" for p in pkgs)
    return "skip"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("root")
    ap.add_argument("--scan", action="store_true")
    ap.add_argument("--write", metavar="TSV")
    ap.add_argument("--check", metavar="TSV")
    ap.add_argument("--decide", metavar="TSV")
    args = ap.parse_args()
    root = Path(args.root).resolve()

    if args.decide:
        rows = load_tsv(Path(args.decide))
        changed = [ln.strip() for ln in sys.stdin if ln.strip()]
        print(decide(changed, rows))
        return 0

    found = scan(root)
    rows = rows_from_found(found)
    if args.scan:
        for p, pkg in rows:
            print(f"{p}\t{pkg}")
        return 0
    if args.write:
        write_tsv(Path(args.write), rows)
        print(f"wrote {len(rows)} rows to {args.write}", file=sys.stderr)
        return 0
    if args.check:
        committed = load_tsv(Path(args.check))
        committed_set = set(committed)
        scanned_set = set(rows)
        missing = sorted(scanned_set - committed_set)
        if missing:
            print("FAIL rust-test-inputs --check: scan found inputs missing from the checked-in list:", file=sys.stderr)
            for p, pkg in missing:
                print(f"  {p}\t{pkg}", file=sys.stderr)
            return 1
        print(f"ok   rust-test-inputs --check ({len(scanned_set)} scanned rows, {len(committed_set)} listed)")
        return 0
    ap.error("need --scan, --write, --check, or --decide")
    return 2


if __name__ == "__main__":
    sys.exit(main())
