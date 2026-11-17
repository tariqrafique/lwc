#!/usr/bin/env python3
"""Throwaway SD-2c premise-gate measurement. Not shipped.

For each of the 246 non-equal H-1 pairs in a 681aba0-class summary, decide
deterministically whether the conservative static rule in SD-2c §3 would
certify the pair (equal traces + no listed channel can see the differing
function text).

Channel 1 (F1): only LWC_VERSION_COMMENT_REGEX matches on function bodies that
look like the template function (the `function tmpl` / default export that ends
with the banner comment). Compared across sides.
Channel 2: not visible in the pair module (engine allowlist). Treated as not
firing unless a pair itself calls into an engine toString — it does not.
Channel 3: a function expression / arrow appearing in an attribute or
set-property position, or an `lwc:external` companion. Operand not provably
non-function → NotProven → OBSERVED.
Channel 4: `.toString(`, `String(`, template `${...}` in the compiled module
whose operand is not a literal (number/string/boolean/null). NotProven → OBSERVED.

A pair certifies only if traces are equal (static-observed-differs) AND none of
channels 1, 3, 4 fire (including NotProven).
"""
from __future__ import annotations

import json
import re
import sys
from collections import Counter
from pathlib import Path

BANNER_RE = re.compile(r"/\*LWC compiler v([\d.]+)\*/\s*}")
# Coercions in compiled output. Literals after String( are provably non-function.
STRING_CALL_RE = re.compile(r"\bString\s*\(([^)]*)\)")
TOSTRING_RE = re.compile(r"\.toString\s*\(")
# template interpolations
INTERP_RE = re.compile(r"\$\{([^}]*)\}")
LITERAL_OPERAND = re.compile(
    r"""^\s*(?:0[xXbBoO][\da-fA-F]+|\d+(?:\.\d+)?|'(?:\\.|[^'\\])*'|"(?:\\.|[^"\\])*"|`(?:\\.|[^`\\$])*`|true|false|null|undefined)\s*$"""
)

# Attribute-ish API names used by compiled templates.
ATTR_API_RE = re.compile(
    r"\b(?:api_attribute|api_attr|setAttribute|setProperty|sa|sp)\s*\("
)


def banner_matches(src: str) -> list[str]:
    return BANNER_RE.findall(src)


def channel4_hits(src: str) -> list[str]:
    hits = []
    if TOSTRING_RE.search(src):
        hits.append(".toString(")
    for m in STRING_CALL_RE.finditer(src):
        arg = m.group(1)
        if not LITERAL_OPERAND.match(arg):
            hits.append(f"String({arg[:40]})")
    for m in INTERP_RE.finditer(src):
        inner = m.group(1)
        if not LITERAL_OPERAND.match(inner):
            hits.append(f"${{{inner[:40]}}}")
    return hits


def channel3_hits(src: str) -> list[str]:
    hits = []
    if "lwc:external" in src or "lwcExternal" in src or "external" in src.lower() and "setProperty" in src:
        # keep this tight: compiled modules rarely contain the directive text
        if "lwc:external" in src:
            hits.append("lwc:external")
    # A function/arrow as a likely attribute or property value: `=>` immediately
    # inside an api_* call argument list, or `: () =>` / `: function` in an
    # object passed to the renderer.
    if re.search(r"(api_attribute|api_attr|setProperty|setAttribute)\s*\([^;]{0,200}(?:=>|function\s*\()", src):
        hits.append("function-valued-attr-api")
    return hits


def cluster_of(fd: dict) -> str:
    o = str(fd.get("oracle", ""))
    r = str(fd.get("rust", ""))
    pair = {o, r}
    # C2 arrow parens: a token `( ` vs an identifier, or `(` vs empty-ish
    if "(" in pair or ")" in pair:
        if any(t not in ("(", ")", "") and not re.fullmatch(r"\d+|0[xXbBoO][\da-fA-F]+", t) for t in pair):
            # foo vs (  is C2
            if "(" in pair:
                return "C2-arrow-parens"
    # C1 numeric spelling
    def is_num(t: str) -> bool:
        return bool(re.fullmatch(r"(?:0[xXbBoO][\da-fA-F]+|\d+(?:\.\d+)?)", t or ""))
    if is_num(o) and is_num(r):
        return "C1-numeric"
    if "undefined" in (o, r) or o in ("void",) or r in ("void",):
        return "C3-C5-undefined-or-other"
    return "other"


def main() -> int:
    summary_path = Path(sys.argv[1])
    pairs_dir = Path(sys.argv[2])
    out_path = Path(sys.argv[3])
    d = json.loads(summary_path.read_text())
    manifest = json.loads((pairs_dir / "manifest.json").read_text())
    byid = {c["id"]: c for c in manifest["cases"]}

    totals = d["totals"]
    by_reason = d["by_reason"]
    non = [c for c in d["cases"] if c["bucket"] != "equivalent"]
    assert len(non) == 246, len(non)

    rows = []
    counters = Counter()
    cluster_cert = Counter()
    cluster_total = Counter()
    reason_cert = Counter()

    for c in non:
        v = c["verdict"]
        reason = v["reason"]
        fd = v.get("first_diff") or (v.get("rewrite") or {}).get("first_diff") or {}
        cl = cluster_of(fd) if fd else "no-first-diff"
        # refine C3-C5 from reason
        if reason == "exec-throw-one-side":
            cl = "C3-tagged-template-throw"
        elif reason == "exec-differs":
            rw = (v.get("rewrite") or {}).get("reason")
            if rw == "token-spelling-differs":
                cl = "C2-arrow-parens-exec-differs"  # the lwc:external one
            else:
                # 70 printed-differs exec-differs: C4/C5 object/template undefined
                cl = "C4-C5-exec-differs"
        rec = byid[c["id"]]
        oracle = (pairs_dir / rec["oracle"]).read_text()
        rust = (pairs_dir / rec["rust"]).read_text()
        traces_equal = reason == "static-observed-differs"
        b1 = banner_matches(oracle) != banner_matches(rust)
        c3 = channel3_hits(oracle) + [f"rust:{h}" for h in channel3_hits(rust)]
        c4 = channel4_hits(oracle) + [f"rust:{h}" for h in channel4_hits(rust)]
        fires = []
        if b1:
            fires.append("banner")
        if c3:
            fires.append("dom-attr")
        if c4:
            fires.append("output-coercion")
        # equal traces required
        would = traces_equal and not fires
        # hoist-only: printed-differs + equal traces
        rw = (v.get("rewrite") or {}).get("reason") or v.get("exec", {}).get("detail", {}).get("rewrite_reason")
        hoist_only = traces_equal and rw == "printed-differs"
        row = {
            "id": c["id"],
            "reason": reason,
            "rewrite_reason": rw,
            "cluster": cl,
            "traces_equal": traces_equal,
            "hoist_only": hoist_only,
            "first_diff": fd,
            "banner_oracle": banner_matches(oracle),
            "banner_rust": banner_matches(rust),
            "channel3": c3,
            "channel4": c4,
            "fires": fires,
            "would_certify": would,
        }
        rows.append(row)
        cluster_total[cl] += 1
        if traces_equal:
            counters["equal_traces"] += 1
            cluster_cert[cl + (":certify" if would else ":no")] += 1
            if would:
                counters["would_certify"] += 1
            else:
                counters["equal_traces_observed"] += 1
        else:
            counters["unequal_traces"] += 1
        if hoist_only:
            counters["hoist_only"] += 1
        reason_cert[reason] += 1

    # How many printer-equal exist (never need SD-2c)
    printer_equal = by_reason.get("printer-equal", totals.get("equivalent", 0))

    report = {
        "summary": str(summary_path),
        "pairs_dir": str(pairs_dir),
        "totals": totals,
        "by_reason": by_reason,
        "printer_equal_never_need_sd2c": printer_equal,
        "non_equal": 246,
        "equal_traces_candidates": counters["equal_traces"],
        "would_certify": counters["would_certify"],
        "equal_traces_but_channel_observed": counters["equal_traces_observed"],
        "unequal_traces_not_candidates": counters["unequal_traces"],
        "hoist_only_equal_trace": counters["hoist_only"],
        "cluster_totals_among_246": dict(cluster_total),
        "cluster_equal_trace_decisions": dict(cluster_cert),
        "reason_counts": dict(reason_cert),
        "cases": rows,
    }
    out_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in report if k != "cases"}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
