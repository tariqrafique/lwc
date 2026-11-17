# Land-gate proposal: residual hardening of scripts/land.sh (diff against live main)

LWC Arch Board, 2026-11-01. **Proposal only.** The Arch Board does not edit `scripts/land.sh`, `scripts/land-classify.sh` or `trivial-paths.txt`. Harness Cutover owns these scripts and should land the patch below through land.sh with an `Arch-Review:` trailer.

Baseline: `/workspace/lwc-compiler-rust` main at `b11adfb`. `scripts/land.sh` is 64 lines and was last changed in `481950b`.

The two items the brief asked me to design have already landed while this review was running:

| brief item | landed in | live lines |
|---|---|---|
| (1) patch_id binding | `1e5558d` "land.sh: bind Arch-Review to git patch-id --stable of the landing diff; scripts/review-patch-id.sh" | land.sh:39 computes `pid`. :42 looks up columns by header name (id/verdict/patch_id) and requires all three; `tail -1` means the last row wins. :45 requires `APPROVE && rp non-empty && rp == pid`. |
| (3) `+`-prefixed globs | `481950b` "land.sh: trivial classifier (scripts/land-classify.sh) per Arch Board policy" | land-classify.sh:18-19 (`+glob` => numstat `added>0 && deleted=="0"`). land.sh:32-35 is the classify block, which is tested verbatim by test-land-classify.sh. |

This document therefore (a) explains how the live (1) closes the hole described in (2), and (b) proposes a residual-hardening diff. The diff has been validated in a scratch copy (`/tmp/lgp`): `bash -n` is clean on all three scripts, and the extended self-test passes with the proposed classifier. Four new cases fail with the live classifier.

## 1. Patch-id binding (live) and how it closes (2)

Before `1e5558d`, the gate accepted any landing whose range carried *some* `Arch-Review: <id>` trailer whose row said APPROVE. That allowed two attacks:

- **(2a) One id covers the whole range.** A bot could put a reviewed commit and an unreviewed commit in one landing. The single APPROVE row approved everything.
- **(2b) Id reuse.** Any old APPROVE id, from any branch and any date, could be pasted into a new commit's trailer.

The live code closes both because it binds the approval to the content of the entire landing, not to the id:

```bash
# land.sh:39 (live)
pid="$(git diff "$base" "$head" | git patch-id --stable | cut -d' ' -f1)"
# land.sh:45 (live)
[ "$v" = APPROVE ] && [ -n "$rp" ] && [ "$rp" = "$pid" ] && ok=1
```

- `base` is main *after* the rebase at land.sh:20-21, and `head` is the branch tip. So `pid` hashes exactly what the fast-forward at :60 will add to main.
- **(2a) is closed.** Adding any unreviewed hunk to the range changes `pid`, and no APPROVE row carries the new value.
- **(2b) is closed.** An old id passes only if its row's `patch_id` equals the new landing's diff. In other words, the "reuse" re-lands byte-identical content that was already approved. Against an advanced main, that is either an empty diff (refused at :22) or a diff that hashes differently.
- If main moves and the rebase shifts the hunks, `pid` changes and the review must be redone. This fails closed, and the die message at :46 says so.

Evidence that the live hash is the one reviewers record: the board's APPROVE rows match `git diff <c>~1 <c> | git patch-id --stable` for the landed single-commit ranges. AB-L0001 is `cf06347b…` for `1e5558d`, AB-L0006 is `b45cd47f…` for `481950b`, AB-L0007 is `d4196e00…` for `c6c0240`, and AB-L0008 is `6dba099c…` for `7dcfe44`.

Also checked: a scratch repo (`/tmp/pidt`) confirmed that binary-content changes and mode-only changes **do** change `patch-id --stable`. Neither is a bypass.

### Residual gaps in (1)

| # | gap | why it matters | fix (in diff below) |
|---|---|---|---|
| R1 | land.sh:39 and scripts/review-patch-id.sh each spell the diff command separately | The two can drift. The bot's `~/.gitconfig` or repo config (`diff.external`, textconv, `diff.noprefix`, `diff.renames`) also feeds into the hash. | land.sh calls `review-patch-id.sh <base> <head>`, which pins the diff flags. |
| R2 | reviews.tsv and trivial-paths.txt are read from the notes **working tree** (`$AB/...`, land.sh:33,40,42) | Every bot can write `/workspace/notes`. A bot can append its own APPROVE row with the right `patch_id` (it can compute it with review-patch-id.sh), and nothing records that. | Read `HEAD:arch-board/<file>` from the notes git repo. Board approvals are already committed (e.g. notes `3bb9e66`, `cc27410`, `563990c`), so this matches current practice and makes every approval auditable. |
| R3 | The row's `reviewer` column is not checked | A row with any reviewer passes. | Require a `reviewer` column and require `$r=="LWC Arch Board"`. All 20 rows present at proposal time already carry exactly that value. |

R2+R3 do not stop a bot from committing a forged row to notes. They make the forgery a named commit in an append-only log, which the board's sweep can diff. Fully separating authority (a notes repo writable only by the board) is an ops decision outside these scripts.

## 2. `+` globs (live) and the residual bypass

The live `+glob` means "additions-only": numstat shows `added>0` and `deleted==0`. That is not "tests-only, mechanically checkable". I reproduced two bypasses with main's own `land-classify.sh` against the decided `/workspace/notes/arch-board/trivial-paths.txt` (scratch repo `/tmp/clt`):

1. **Disable a test file by adding one line.** I prepended `#![cfg(any())]` to `crates/lwc-x/tests/a.rs` (numstat `1 0`) and deleted `crates/lwc-x/NOTES.md`. The classifier output was **empty**, which means trivial: no Arch-Review and no review row. The same works with `#[ignore]` or an early `return;` in a test.
2. **`*` crosses `/`.** `[[ crates/lwc-template/src/parser/tests/x.rs == crates/*/tests/*.rs ]]` is true, so `+crates/*/tests/*.rs` reaches into `src/`. The same applies to `crates/lwc-template/tests/parse-errors/x/evil.rs`, the template diagnostics corpus. `*.md` likewise matches `harness/fixtures/js-eval/private-field-proxy/README.md`, a tracked fixture, which contradicts the policy file's own NEVER list for `harness/fixtures/**`.

Proposed fix (classifier code only; trivial-paths.txt stays as decided):

- **R4.** A `+glob` matches only **newly added** files (`--diff-filter=A`). Edits to existing files, even additions-only edits, are non-trivial.
- **R5.** A built-in NEVER list is checked *before* any glob, so no policy glob can make goldens, fixtures, src, the parse-error corpus, the harness, scripts, Cargo manifests or decisions.tsv trivial. The list repeats the NEVER comment from trivial-paths.txt as code.

With the proposed classifier, the `/tmp/clt` reproduction prints `crates/lwc-x/tests/a.rs`, so the landing is non-trivial. A new test file `crates/lwc-x/tests/new.rs` plus `docs/a.md` still classify as trivial.

## 3. Other gates (from the sweep: AB-0003, AB-0005)

| # | live behaviour | proposed |
|---|---|---|
| R6 | There is no `cd "$wt"`. Relative `crates/$c/Cargo.toml` (:51), `scripts/test-land-classify.sh` (:53) and `harness/...` (:54-57) assume the caller ran land.sh from the worktree root. | `cd "$wt"` right after `wt=` is computed. |
| R7 | :49-52 runs `cargo test -p` only for crates with changed paths. A change in `lwc-shared` or `lwc-template` skips the tests of `lwc-compiler`, `lwc-ssr` and `lwc-compiler-napi`, which depend on them. | `cargo test --workspace`. |
| R8 | The option-inventory gate (`harness/scripts/option-gate.mjs`) only runs inside `harness/board.sh:88`, which nobody has to run before landing. | Run it in land.sh when compiler, napi, shared or options paths change. This needs `harness/board-rows.sh` holding `ROWS` (board.sh:16) and `DIAG` (board.sh:18), sourced by both scripts so there is one source of truth. |
| R9 | napi is built only *after* the merge (:62-63), and a failure only prints a warning. | Run `cargo build -p lwc-compiler-napi --release` before the merge so it gates the landing. The post-merge install stays non-fatal. |
| R10 | golden-gate.sh copies the harness *including* goldens (:12-13), and its recording step is `|| true` (:19). A failed recording compares stale goldens against themselves and passes (AB-0003). | Out of scope for this diff because it is in harness/scripts. Required: copy everything except `goldens/`, record fresh with `UPDATE_GOLDENS=1` on the **js** backend, drop `|| true`, and fail if any golden is missing from the fresh recording. |
| R11 | shim-gate (:54) and golden-gate (:57) run the **branch's** copies, so a branch can weaken a gate and use the weakened version in the same landing. shim-gate's allowlist also resolves via `import.meta.url` (shim-gate.mjs:7,11). | Run gate scripts from `$MAIN` (as land.sh:10 already does for itself and :33 for the classifier), pointing them at `$wt` for inputs. If a branch changes a gate script, its new version takes effect only after it lands. |

Not proposed: making the Rust-backend behaviour rows gate land.sh. board.sh:80 runs every row with `|| true` because the Rust backend has known failing rows. A pass/fail gate would block every landing. The right shape is a no-regression ratchet against a recorded per-row baseline from main. That needs a baseline file format that Harness Cutover should design (AB-0006).

## 4. The diff

Generated in `/tmp/lgp` from copies of main's scripts at `b11adfb`, using `git diff`. The `harness/board-rows.sh` file referenced by R8 is new and is not shown here: it is board.sh:16 and :18 moved verbatim.

```diff
diff --git a/scripts/land-classify.sh b/scripts/land-classify.sh
index 9de97d9..f88332f 100755
--- a/scripts/land-classify.sh
+++ b/scripts/land-classify.sh
@@ -9,14 +9,21 @@ set -euo pipefail
 base="$1"; git rev-parse -q --verify "$1^{commit}" >/dev/null && git rev-parse -q --verify "$2^{commit}" >/dev/null || { echo "land-classify: bad revs $1 $2" >&2; exit 2; }
 head="$2"; policy="$3"
 globs=()
+# Built-in NEVER list (mirrors the NEVER comment in trivial-paths.txt): checked BEFORE any trivial glob, so a broad
+# glob such as '*.md' or '+crates/*/tests/*.rs' ('*' crosses '/') can never make these trivial.
+never=('crates/*/src/*' 'harness/goldens/*' 'harness/fixtures/*' 'crates/*/tests/goldens/*' 'crates/*/tests/parse-errors/*'
+       'harness/test/*' 'harness/src/*' 'scripts/*' 'Cargo.toml' 'Cargo.lock' '*/Cargo.toml' 'decisions.tsv')
 if [ -f "$policy" ]; then
   while IFS= read -r l || [ -n "$l" ]; do l="${l%%#*}"; l="$(printf '%s' "$l" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"; [ -n "$l" ] && globs+=("$l"); done < "$policy"
 fi
 git diff --no-renames --name-only "$base" "$head" | while IFS= read -r f; do
   [ -n "$f" ] || continue; m=
+  for n in "${never[@]}"; do [[ $f == $n ]] && { printf '%s\n' "$f"; continue 2; }; done
   for g in "${globs[@]+"${globs[@]}"}"; do
     if [[ $g == +* ]]; then
-      [[ $f == ${g#+} ]] && [ "$(git diff --no-renames --numstat "$base" "$head" -- "$f" | awk '$1 ~ /^[0-9]+$/ && $1 > 0 && $2 == "0" {print "add"}')" = add ] && { m=1; break; }
+      # '+glob' = NEW file only (status A). Additions-only edits to an existing file are not trivial: a single added
+      # '#![cfg(any())]' or '#[ignore]' disables tests with numstat "1 0".
+      [[ $f == ${g#+} ]] && [ "$(git diff --no-renames --diff-filter=A --numstat "$base" "$head" -- "$f" | awk '$1 ~ /^[0-9]+$/ && $1 > 0 && $2 == "0" {print "add"}')" = add ] && { m=1; break; }
     else [[ $f == $g ]] && { m=1; break; }; fi
   done
   [ -n "$m" ] || printf '%s\n' "$f"
diff --git a/scripts/land.sh b/scripts/land.sh
index b049711..dbccd1f 100755
--- a/scripts/land.sh
+++ b/scripts/land.sh
@@ -11,6 +11,7 @@ MAIN=/workspace/lwc-compiler-rust
 die() { echo "!! LAND REFUSED: $*" >&2; exit 1; }
 source "$HOME/.cargo/env" 2>/dev/null || true
 wt="$(git rev-parse --show-toplevel)"
+cd "$wt"   # every relative path below (crates/, harness/, scripts/) is worktree-relative
 br="$(git symbolic-ref --short -q HEAD || true)"
 [ "$wt" != "$MAIN" ] && [ "$br" != main ] && [ -n "$br" ] || die "run from your worktree on bot/<name>, not from $MAIN / main"
 [ -z "$(git status --porcelain)" ] || die "uncommitted changes in $wt (commit or stash first)"
@@ -29,29 +30,39 @@ echo "land: $br $(git rev-list --count "$base..$head") commit(s) onto main@${bas
 #                       scripts/review-patch-id.sh); the landing's recomputed patch-id must equal the row's. non-trivial landings need a commit in the range
 #                       carrying trailer 'Arch-Review: <id>' whose row has verdict APPROVE. Fails closed.
 AB=/workspace/notes/arch-board
+# Policy is read from the COMMITTED notes HEAD, never the working tree (any bot can write the file; a commit is auditable).
+committed() { git -C /workspace/notes show "HEAD:arch-board/$1" 2>/dev/null; }
+policy="$(mktemp)"; trap 'rm -f "$policy"' EXIT; committed trivial-paths.txt > "$policy" || : > "$policy"   # missing = nothing trivial
 # >>> classify (exercised verbatim by scripts/test-land-classify.sh)
-cls_out="$("$MAIN/scripts/land-classify.sh" "$base" "$head" "$AB/trivial-paths.txt")" || die "trivial classifier failed (rc=$?)"
+cls_out="$("$MAIN/scripts/land-classify.sh" "$base" "$head" "$policy")" || die "trivial classifier failed (rc=$?)"
 nontrivial=(); [ -z "$cls_out" ] || mapfile -t nontrivial <<< "$cls_out"
 # <<< classify
 if [ ${#nontrivial[@]} -gt 0 ]; then
   ids="$(git log --format='%(trailers:key=Arch-Review,valueonly)' "$base..$head" | sed '/^\s*$/d' | xargs -n1 2>/dev/null | sort -u)"
   [ -n "$ids" ] || die "non-trivial landing (${nontrivial[*]:0:5}...) needs an 'Arch-Review: <id>' trailer with an APPROVE row in $AB/reviews.tsv (ask LWC Arch Board)"
-  pid="$(git diff "$base" "$head" | git patch-id --stable | cut -d' ' -f1)"
-  [ -f "$AB/reviews.tsv" ] || die "no $AB/reviews.tsv; non-trivial landings cannot be approved yet"
+  pid="$("$MAIN/scripts/review-patch-id.sh" "$base" "$head")"   # ONE definition of the id, shared with reviewers
+  [ -n "$pid" ] || die "empty patch-id for $base..$head"
+  reviews="$(committed reviews.tsv)" || die "no committed arch-board/reviews.tsv in /workspace/notes; non-trivial landings cannot be approved yet"
   ok=; for id in $ids; do
-    row="$(awk -F'\t' -v id="$id" 'NR==1{for(i=1;i<=NF;i++){h=tolower($i); if(h=="id")c=i; if(h=="verdict")d=i; if(h=="patch_id")p=i}; next} c&&d&&p&&$c==id{print toupper($d) "\t" $p}' "$AB/reviews.tsv" | tail -1)"
+    row="$(awk -F'\t' -v id="$id" 'NR==1{for(i=1;i<=NF;i++){h=tolower($i); if(h=="id")c=i; if(h=="verdict")d=i; if(h=="patch_id")p=i; if(h=="reviewer")r=i}; next} c&&d&&p&&r&&$c==id&&$r=="LWC Arch Board"{print toupper($d) "\t" $p}' <<< "$reviews" | tail -1)"
     v="${row%%$'\t'*}"; rp="${row#*$'\t'}"
     echo "land: Arch-Review $id -> ${v:-<not found>} (reviewed patch-id ${rp:0:12}, landing ${pid:0:12})"
     [ "$v" = APPROVE ] && [ -n "$rp" ] && [ "$rp" = "$pid" ] && ok=1; done   # LAST row per id wins
   [ -n "$ok" ] || die "no APPROVE row in $AB/reviews.tsv whose patch_id matches this landing ($pid) for Arch-Review id(s): $ids. Diff changed after review (or rebase shifted it): get it re-reviewed. Compute with scripts/review-patch-id.sh"
 else echo "land: trivial landing (all paths match $AB/trivial-paths.txt), no Arch-Review needed"; fi
 cargo check --workspace --all-targets || die "cargo check failed"
-pkgs=()
-for c in $(echo "$changed" | sed -n 's#^crates/\([^/]*\)/.*#\1#p' | sort -u); do
-  [ -f "crates/$c/Cargo.toml" ] && pkgs+=(-p "$(sed -n 's/^name *= *"\(.*\)"/\1/p' "crates/$c/Cargo.toml" | head -1)"); done
-if [ ${#pkgs[@]} -gt 0 ]; then cargo test "${pkgs[@]}" || die "crate tests failed (${pkgs[*]})"; fi
+# Test the whole workspace: a change in lwc-shared/lwc-template must run its dependents' tests too.
+cargo test --workspace || die "workspace tests failed"
 if echo "$changed" | grep -qE '^scripts/(land|land-classify|test-land-classify)\.sh$'; then scripts/test-land-classify.sh || die "land-classify self-test failed"; fi
 CRATES_ROOT="$wt/crates" node harness/scripts/shim-gate.mjs || die "shim gate failed"
+if echo "$changed" | grep -qE '^(crates/lwc-(compiler|compiler-napi|shared)/|harness/src/backend\.ts|harness/scripts/option-)'; then
+  ( source harness/board-rows.sh   # NEW file: board.sh:16 ROWS + :18 DIAG moved here and sourced by board.sh too
+    OPTGATE_DIAG="$DIAG" node harness/scripts/option-gate.mjs "${ROWS[*]}" ) || die "option gate failed"
+fi
+if echo "$changed" | grep -q '^crates/\|^Cargo\.'; then
+  # Build the branch napi BEFORE merging: a crate change that breaks the napi build must not reach main.
+  cargo build -p lwc-compiler-napi --release || die "napi build failed (pre-merge)"
+fi
 if echo "$changed" | grep -qE '^harness/(goldens|test|src|fixtures)/'; then
   harness/scripts/ensure-deps.sh
   harness/scripts/golden-gate.sh "$wt/harness" || die "golden-gate: goldens != fresh JS-oracle recording"
@@ -60,5 +71,5 @@ fi
 git -C "$MAIN" merge --ff-only -q "$head" || die "fast-forward of main failed"
 echo "landed: main=${head:0:12} ($br)"
 if echo "$changed" | grep -q '^crates/\|^Cargo\.'; then
-  (cd "$MAIN" && scripts/install-napi.sh) || echo "!! landed, but live napi rebuild from main failed (main is still correct)" >&2
+  (cd "$MAIN" && scripts/install-napi.sh) || echo "!! landed, but live napi install from main failed (build already passed pre-merge)" >&2
 fi
diff --git a/scripts/review-patch-id.sh b/scripts/review-patch-id.sh
index 1f6b8dd..39f05a3 100755
--- a/scripts/review-patch-id.sh
+++ b/scripts/review-patch-id.sh
@@ -1,7 +1,9 @@
 #!/usr/bin/env bash
 # Patch-id that land.sh will bind an Arch-Review to: git patch-id --stable of the whole diff main...<ref>.
-# Usage (reviewer or bot, from any worktree): scripts/review-patch-id.sh [ref=HEAD]
+# Usage (reviewer or bot, from any worktree): scripts/review-patch-id.sh [ref=HEAD]   |   land.sh: review-patch-id.sh <base> <head>
+# Flags pin the diff so user/repo config (diff.renames, diff.noprefix, diff.external, textconv, color) cannot change the id.
 # Rebase onto main first; if main moves and the rebased diff changes, the id changes and the review must be redone.
 set -euo pipefail
-ref="${1:-HEAD}"; base="$(git merge-base main "$ref")"
-git diff "$base" "$ref" | git patch-id --stable | cut -d' ' -f1
+if [ $# -eq 2 ]; then base="$1"; ref="$2"; else ref="${1:-HEAD}"; base="$(git merge-base main "$ref")"; fi
+git -c diff.noprefix=false -c diff.mnemonicPrefix=false -c diff.relative=false diff --no-ext-diff --no-textconv --no-color --no-renames --full-index --binary "$base" "$ref" \
+  | git patch-id --stable | cut -d' ' -f1
```

### Self-test additions (scripts/test-land-classify.sh)

The `policy=` line is required because the classify block now reads `$policy` rather than `$AB/trivial-paths.txt`. Without it the block-extraction cases would fail on an unset variable, i.e. for the wrong reason.

```diff
diff --git a/scripts/test-land-classify.sh b/scripts/test-land-classify.sh
new file mode 100755
index 0000000..1debc05
--- /dev/null
+++ b/scripts/test-land-classify.sh
@@ -0,0 +1,38 @@
+#!/usr/bin/env bash
+# Self-test for scripts/land-classify.sh against Arch Board's policy shape. Exit 0 iff every case classifies right.
+set -euo pipefail
+C="$(cd "$(dirname "$0")" && pwd)/land-classify.sh"; t="$(mktemp -d)/repo"; P="$(dirname "$t")"; mkdir -p "$t"; trap "rm -rf \"$P\"" EXIT
+cd "$t"; git init -q -b main; git config user.email t@t; git config user.name t
+mkdir -p crates/x/src crates/x/tests docs; seq 1 50 > crates/x/src/a.rs; seq 1 5 > crates/x/tests/old.rs; echo d > docs/d.md
+printf '\x00\x01bin' > crates/x/tests/blob.rs.orig; git add -A; git commit -qm base; base=$(git rev-parse HEAD)
+printf 'docs/*\n*.md\n+crates/*/tests/*.rs\n' > "$P/policy"
+fail=0; check() { # name expect(trivial|nontrivial) [policy]
+  local out; out="$("$C" "$base" HEAD "${3:-$P/policy}")"
+  local got=trivial; [ -n "$out" ] && got=nontrivial
+  if [ "$got" = "$2" ]; then echo "ok   $1 -> $got ${out:+($(echo $out))}"; else echo "FAIL $1 -> $got, want $2 ${out:+($(echo $out))}"; fail=1; fi
+  git reset -q --hard "$base"; git clean -qfd; }
+echo x >> docs/d.md; git commit -qam c; check "docs edit" trivial
+seq 1 9 > crates/x/tests/new.rs; git add -A; git commit -qm c; check "+tests addition" trivial
+echo 6 >> crates/x/tests/old.rs; sed -i 1d crates/x/tests/old.rs; git commit -qam c; check "+tests edit with deletion" nontrivial
+git mv crates/x/src/a.rs crates/x/tests/a.rs; git commit -qm c; check "rename src->tests (+glob)" nontrivial
+git mv crates/x/src/a.rs docs/foo.md; git commit -qm c; check "rename src->docs (plain glob)" nontrivial
+printf '\x00\x02\x03' > crates/x/tests/b.rs; git add -A; git commit -qm c; check "binary under +glob" nontrivial
+echo x >> docs/d.md; git commit -qam c; check "missing policy file = nothing trivial" nontrivial "$P/nope"
+echo x >> crates/x/src/a.rs; git commit -qam c; check "src edit" nontrivial
+chmod +x crates/x/tests/old.rs; git commit -qam c; check "mode-only change under +glob" nontrivial
+printf '#![cfg(any())]\n' | cat - crates/x/tests/old.rs > o && mv o crates/x/tests/old.rs; git commit -qam c; check "+glob additions-only edit of EXISTING test (cfg-out)" nontrivial
+mkdir -p crates/x/src/tests; seq 1 3 > crates/x/src/tests/t.rs; git add -A; git commit -qm c; check "+glob '*' crossing into src/ (NEVER)" nontrivial
+mkdir -p harness/fixtures/f; echo r > harness/fixtures/f/README.md; git add -A; git commit -qm c; check "*.md under harness/fixtures (NEVER)" nontrivial
+mkdir -p crates/x/tests/goldens; echo g > crates/x/tests/goldens/g.md; git add -A; git commit -qm c; check "*.md under tests/goldens (NEVER)" nontrivial
+# classifier error (bad rev) must exit non-zero ...
+if "$C" deadbeef HEAD "$P/policy" >/dev/null 2>&1; then echo "FAIL classifier bad rev -> rc 0"; fail=1; else echo "ok   classifier bad rev -> rc!=0"; fi
+# ... and land.sh's actual classify block (extracted verbatim) must refuse when the classifier fails or crashes.
+L="$(cd "$(dirname "$C")" && pwd)/land.sh"; blk="$(sed -n '/^# >>> classify/,/^# <<< classify/p' "$L")"
+[ -n "$blk" ] || { echo "FAIL classify block not found in land.sh"; fail=1; }
+for mode in "exit 3" "echo crates/x/src/a.rs; exit 1" "kill -9 \$\$"; do
+  mkdir -p "$P/fm/scripts"; printf '#!/usr/bin/env bash\n%s\n' "$mode" > "$P/fm/scripts/land-classify.sh"; chmod +x "$P/fm/scripts/land-classify.sh"
+  if ( set -euo pipefail; die() { exit 97; }; MAIN="$P/fm"; AB="$P"; policy="$P/policy"; base=$base; head=HEAD; eval "$blk"; exit 0 ) >/dev/null 2>&1; then
+    echo "FAIL land classify block with failing classifier ($mode) -> did not refuse"; fail=1
+  else echo "ok   land refuses when classifier fails ($mode)"; fi
+done
+exit $fail
```

Results in scratch (`bash scripts/test-land-classify.sh`):

- With the proposed classifier, all 17 cases are `ok` (rc=0).
- With the **live** `land-classify.sh` and the extended test, these 4 cases fail:
  - `+glob additions-only edit of EXISTING test (cfg-out) -> trivial`
  - `+glob '*' crossing into src/ (NEVER) -> trivial`
  - `*.md under harness/fixtures (NEVER) -> trivial`
  - `*.md under tests/goldens (NEVER) -> trivial`

### Compatibility

- **Pinned flags.** The proposed `review-patch-id.sh` flags (`--no-renames --full-index --binary --no-ext-diff --no-textconv --no-color`) give the **same** ids as the live command on `1e5558d`, `481950b`, `c6c0240`, `7dcfe44` and `b11adfb`, which I checked one by one. Ids differ only for diffs containing renames or binary hunks. Any in-flight APPROVE row for such a diff must be re-hashed after this lands.
- **Committed notes.** Reading from `HEAD` means the board must commit a row before the bot runs land.sh. That is already the board's practice.

## 5. Order of landing

1. R4+R5 classifier and test additions. This closes the proven bypass, and the patch is small and self-tested.
2. R1-R3 (one patch-id helper, committed notes, reviewer check) and R6 (`cd "$wt"`).
3. R7 and R9 (workspace tests, pre-merge napi build).
4. R8 (`board-rows.sh` and the option gate), R10 (golden-gate fix), R11 (gates run from main).
