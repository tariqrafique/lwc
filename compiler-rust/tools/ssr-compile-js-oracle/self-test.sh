#!/usr/bin/env bash
# Self-test of run.mjs's exit paths. Each test builds a scratch git repo whose `main` commit is a copy of this
# directory, makes the edit under test in the scratch working tree (the real lists are never edited), and runs this
# directory's run.mjs against it with --root=<scratch>, as land.sh runs main's run.mjs against a branch worktree.
# Every negative is also run with the scratch tree's own run.mjs, with that rule disabled, and must go green
# (exit 0), so each negative is caught by the rule it names. root-guard shows that disabling a rule in the branch's
# run.mjs does not disable it in main's.
# Usage: tools/ssr-compile-js-oracle/self-test.sh [napi]   (default: the built cdylib run.mjs would load)
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; repo="$(cd "$here/../.." && pwd)"
napi="${1:-${CARGO_TARGET_DIR:-$repo/target}/release/liblwc_compiler_napi.so}"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
root="$tmp/r"; o="$root/tools/ssr-compile-js-oracle"
GREEN='^[0-9]+ cases, ([0-9]+) differ, \1 known'
checks=0 fails=0
check() { # check <name> <want-exit> <want-output-regex> <cmd...>
  local name=$1 want=$2 re=$3; shift 3; local out rc
  checks=$((checks + 1)); out="$("$@" 2>&1)"; rc=$?
  if [ "$rc" = "$want" ] && grep -qE -- "$re" <<<"$out"; then echo "ok   $name (exit $rc)"
  else echo "FAIL $name: exit $rc (want $want), output: $(tail -2 <<<"$out")"; fails=$((fails + 1)); fi
}
fresh() { # scratch repo: `main` = this directory as it is now; working tree = main until the test edits it
  rm -rf "$root"; mkdir -p "$root/tools"; cp -r "$here" "$o"
  git -C "$root" init -q -b main && git -C "$root" add -A \
    && git -C "$root" -c user.name=t -c user.email=t@t commit -qm main
}
mutate() { # mutate <rule>...: disable each named /*RULE:x*/ check in the scratch tree's run.mjs (never ours)
  local r; for r in "$@"; do
    case $r in
      raise|drop|env) perl -pi -e "s|/\\*RULE:$r\\*/ |/*RULE:$r*/ false && |" "$o/run.mjs" ;;
      extract-missing|extract-dup|absent-present|absent-unknown) perl -pi -e "s|/\\*RULE:$r\\*/ |/*RULE:$r*/ false && |" "$o/compare.mjs" ;;
      extract-nested)
        python3 - "$o/compare.mjs" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); s = p.read_text()
old = "/*RULE:extract-nested*/ slice(code, siArg)"
new = r"""(code.match(/__setStaticInternals\(\s*\w+,\s*["'][^"']*["'],\s*(\[[^\]]*\])/) || [])[1]"""
if old not in s: raise SystemExit("extract-nested marker not found")
p.write_text(s.replace(old, new, 1))
PY
        ;;
      token-types)
        python3 - "$o/compare.mjs" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); s = p.read_text()
old = """const tokens = (s) => { /*RULE:token-types*/
    return [...acorn.tokenizer(s, { ecmaVersion: 'latest', sourceType: 'module' })]
      .map((t) => (t.value === undefined ? t.type.label : `${t.type.label}:${JSON.stringify(t.value)}`)).join(' ');
  };"""
new = """const tokens = (s) => { /*RULE:token-types*/
    const mapped = [...acorn.tokenizer(s, { ecmaVersion: 'latest', sourceType: 'module' })]
      .map((t) => { const l = (t.type.label === 'template' || t.type.label === 'string' || t.type.label === 'name') ? 'lit' : t.type.label; return t.value === undefined ? l : `${l}:${JSON.stringify(t.value)}`; })
      .filter((x) => x !== '[' && x !== ']' && x !== '`');
    return mapped.join(' ').replace(/(\\blit:\\S+) : \\1/g, '$1');
  };"""
if old not in s: raise SystemExit("token-types body not found")
p.write_text(s.replace(old, new, 1))
PY
        ;;
      branch-list) perl -pi -e 's|/\*RULE:branch-list\*/ setupFail\(.*?\);|/*RULE:branch-list*/ listText = execFileSync("git", ["-C", BRANCH, "show", "main:./known-diffs.txt"], { encoding: "utf8" });|' "$o/run.mjs" ;;
      arg-*) perl -pi -e "s|/\\*RULE:$r\\*/ setupFail\\(.*\\); \\}|/*RULE:$r*/ continue; }|" "$o/run.mjs" ;;
      main-ref) perl -pi -e 's|/\*RULE:main-ref\*/ setupFail\(.*?\);|/*RULE:main-ref*/ mainKnown = new Set(known.keys()); mainCases = new Set();|' "$o/run.mjs" ;;
    esac
    if grep -qE "RULE:$r\\*/ (setupFail|process|raised|dropped)" "$o/run.mjs"; then
      echo "FAIL mutate $r did not apply"; checks=$((checks + 1)); fails=$((fails + 1)); fi
  done; return 0
}
run() { node "$here/run.mjs" --root="$root" --napi="$napi" "$@"; }   # ours, judging the scratch tree
own() { node "$o/run.mjs" --napi="$napi" "$@"; }                     # the scratch tree's (maybe mutant) copy
# Edits. A copy of a listed case always differs, so it is a genuine new diff.
raise() { echo 'cases.zzRaise = cases.ctorDefaults;' >>"$o/cases-members.mjs"; echo 'members/zzRaise  new entry' >>"$o/known-diffs.txt"; }
swap() { echo 'delete cases.multiline; cases.zzNew = C("  @wire(adapter, {\n  // c\n  foo: 1 }) p;");' >>"$o/cases-wire.mjs"
         sed -i '/^wire\/multiline /d' "$o/known-diffs.txt"; echo 'wire/zzNew  swapped in' >>"$o/known-diffs.txt"; }
drop() { echo 'delete cases.split;' >>"$o/cases-props.mjs"; }   # props/split is not a known diff
# negative <name> <want-exit> <regex> <edit> <rule>...: red under ours; green under the scratch copy with the rule(s) off
negative() {
  local name=$1 want=$2 re=$3 edit=$4; shift 4
  fresh; eval "$edit"; check "$name" "$want" "$re" run
  fresh; eval "$edit"; mutate "$@"; check "$name [mutant: $* off]" 0 "$GREEN" own
}

fresh; check clean 0 "$GREEN \\(main=main root=$root\\)" run
fresh; check clean-default-root 0 "$GREEN \\(main=main root=$root\\)" own
fresh; echo 'cases.zzAdded = { src: H + "export default class C extends LightningElement {}\n", expectAbsent: ["reg"] };' >>"$o/cases-props.mjs"
       check add-matching-case 0 '^142 cases' run
negative raise 1 'not a known diff on main .*: members/zzRaise$' raise raise
fresh; raise; mutate raise; check "root-guard: branch run.mjs with raise off, judged by ours" 1 'not a known diff on main .*: members/zzRaise$' run
negative swap 1 'not a known diff on main .*: wire/zzNew$' swap raise drop
fresh; swap; check swap-also-drop 1 'missing from the branch .*: wire/multiline$' run
fresh; swap; mutate raise; check "swap [mutant: raise off] still red via drop" 1 'missing from the branch .*: wire/multiline$' own
fresh; swap; mutate drop; check "swap [mutant: drop off] still red via raise" 1 'not a known diff on main .*: wire/zzNew$' own
negative drop-case 1 'missing from the branch .*: props/split$' drop drop
negative list-missing 2 'cannot read .*known-diffs.txt' 'rm -f $o/known-diffs.txt' branch-list
negative list-unreadable 2 'cannot read .*known-diffs.txt' 'rm -f $o/known-diffs.txt && mkdir $o/known-diffs.txt' branch-list
fresh; check main-unreadable 2 'cannot read the oracle on nope' run --main=nope
fresh; mutate main-ref; check "main-unreadable [mutant: main-ref off]" 0 "$GREEN" own --main=nope
fresh; check env-override 2 'SSR_ORACLE_NAPI is retired' env SSR_ORACLE_NAPI="$napi" node "$here/run.mjs" --root="$root"
fresh; check env-override+flag 2 'SSR_ORACLE_NAPI is retired' env SSR_ORACLE_NAPI="$napi" node "$here/run.mjs" --root="$root" --napi="$napi"
fresh; check env-override-empty 2 'SSR_ORACLE_NAPI is retired' env SSR_ORACLE_NAPI= node "$here/run.mjs" --root="$root" --napi="$napi"
fresh; mutate env; check "env-override+flag [mutant: env off]" 0 "$GREEN" env SSR_ORACLE_NAPI="$napi" node "$o/run.mjs" --napi="$napi"
# Argument guards: exit 2 under ours; under a copy whose guard is off (`continue`: the argument is dropped, so the
# default applies, i.e. the old fallback), the same command line is green.
negarg() { # negarg <name> <regex> <rule> <args...>
  local name=$1 re=$2 rule=$3; shift 3
  fresh; check "$name" 2 "$re" run "$@"
  fresh; mutate "$rule"; check "$name [mutant: $rule off]" 0 "$GREEN" own "$@"
}
negarg arg-unknown 'unknown argument "--roots=x"' arg-unknown --roots=x
negarg arg-positional 'unknown argument "foo"' arg-unknown foo
negarg arg-duplicate '--main given twice' arg-dup --main=main --main=main
negarg arg-empty-main '--main needs a value' arg-empty-main --main=
negarg arg-print-value '--print takes no value' arg-print-value --print=1
fresh; check arg-empty-root 2 '--root needs a value' node "$here/run.mjs" --root= --napi="$napi"
fresh; mutate arg-empty-root; check "arg-empty-root [mutant: arg-empty-root off]" 0 "$GREEN \\(main=main root=$root\\)" own --root=
fresh; check arg-empty-napi 2 '--napi needs a value' node "$here/run.mjs" --root="$root" --napi=
fresh; mutate arg-empty-napi; check "arg-empty-napi [mutant: arg-empty-napi off]" 0 "$GREEN" \
       env CARGO_TARGET_DIR="$(cd "$(dirname "$napi")/.." && pwd)" node "$o/run.mjs" --napi=
# Props arrays compare byte-exact (D-0256): --print shows them as emitted. The mutant re-adds the old quote and
# whitespace stripping to the scratch copy's props extract, and its --print shows the stripped form.
fresh; check props-exact 0 '^  up:   props=\["fieldApi", "g", "m"\] reg=$' run --print
fresh; perl -pi -e 's/return `props=\$\{props\}/return `props=\${String(props).replace(\/["\x27]\/g,"\x27").replace(\/\\s+\/g,"")}/' "$o/compare.mjs"
       check "props-exact [mutant: stripping re-added]" 0 "^  up:   props=\\['fieldApi','g','m'\\] reg=\$" own --print
# Nested-array extract (AST slice vs the old /[^\]]*/ regex) and a missing match on both sides (exit 2; the
# mutant restores undefined==undefined). Token types stay distinct; the mutant collapses them (D-0257, F9).
fresh; check extract-nested 0 '^DIFFER props=\["a", \["x"\], "LEFT"\]' node "$here/prove-extract.mjs"
fresh; mutate extract-nested; check "extract-nested [mutant: regex]" 0 '^SAME props=\["a", \["x"\]' node "$o/prove-extract.mjs"
missing_both() { echo 'cases.zzBare = "export default {};\n";' >>"$o/cases-props.mjs"; }
negative extract-missing-both 2 'EXTRACT props/zzBare' missing_both extract-missing
unknown_absent() { echo 'cases.zzUnk = { src: cases.nestedClass, expectAbsent: ["nope"] };' >>"$o/cases-props.mjs"; }
negative absent-unknown 2 'unknown expectAbsent name "nope"' unknown_absent absent-unknown
present_both() { echo 'cases.zzBoth = { src: cases.nestedClass, expectAbsent: ["reg"] };' >>"$o/cases-props.mjs"; }
negative absent-present-both 2 'stale or false' present_both absent-present
fresh; check absent-present-one 2 'stale or false' node "$here/prove-extract.mjs" present-one
fresh; mutate absent-present; check "absent-present-one [mutant: absent-present off]" 0 '^ok$' node "$o/prove-extract.mjs" present-one
fresh; check extract-dup 2 'duplicate __setStaticInternals call' node "$here/prove-extract.mjs" dup
fresh; mutate extract-dup; check "extract-dup [mutant: extract-dup off]" 0 '^MATCHED props=\["a"\]' node "$o/prove-extract.mjs" dup
missing_reg() { echo 'cases.zzNoReg = H + "export default class C extends LightningElement {}\n";' >>"$o/cases-props.mjs"; }
negative extract-missing-reg 2 'EXTRACT props/zzNoReg' missing_reg extract-missing
fresh; check token-types 0 '5 pairs differ, 0 pairs match' node "$here/prove-tokens.mjs"
fresh; mutate token-types; check "token-types [mutant: types collapsed]" 0 '0 pairs differ, 5 pairs match' node "$o/prove-tokens.mjs" --expect-collapsed
fresh; check napi-missing-in-root 2 "napi not found: $root/target/release" node "$here/run.mjs" --root="$root"
fresh; check napi-unloadable 2 'cannot load napi' node "$here/run.mjs" --root="$root" --napi="$o/README.md"
fresh; check upstream-missing 2 'upstream build not found' env LWC_ROOT="$tmp/none" node "$here/run.mjs" --root="$root" --napi="$napi"
fresh; rm "$o"/cases-*.mjs; check cases-missing 2 'cannot read the cases under' run
fresh; sed -i '/^wire\/multiline /d' "$o/known-diffs.txt"; check new-diff 1 'NEW DIFF wire/multiline' run
fresh; echo 'cases.zzAdded = { src: H + "export default class C extends LightningElement {}\n", expectAbsent: ["reg"] };' >>"$o/cases-props.mjs"
       echo 'props/zzAdded  x' >>"$o/known-diffs.txt"; check stale-entry 1 'STALE props/zzAdded' run
fresh; grep -m1 '^wire/' "$o/known-diffs.txt" >>"$o/known-diffs.txt"; check duplicate-entry 1 'lists a case twice' run
echo "$checks checks: $((checks - fails)) ok, $fails failed"; [ $fails -eq 0 ]
