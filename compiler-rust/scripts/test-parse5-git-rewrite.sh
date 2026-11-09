#!/usr/bin/env bash
# Prove the prepared D-0238 git rewrite (scripts/box-parse5-git/) without applying it
# to the live box ~/.gitconfig or /workspace/.cargo/config.toml:
#   (i)  under the scratch GIT_CONFIG_GLOBAL, /workspace/parse5-rust's origin still resolves to GitHub
#        (fetch and push);
#   (ii) cargo --locked resolves parse5 e3520ba from the local clone (file:// rewrite, no GitHub fetch).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BOX="$ROOT/scripts/box-parse5-git"
T="$(mktemp -d /tmp/p5rewrite-test.XXXXXX)"; trap 'rm -rf "$T"' EXIT
P5=/workspace/parse5-rust
WANT='https://github.com/tariqrafique/parse5-rust'
fail=0
mkdir -p "$T/cargo-home"
ln -s /home/box/.cargo/registry "$T/cargo-home/registry"
cp "$BOX/cargo-config.toml" "$T/cargo-home/config.toml"
cp "$BOX/url-rewrite.gitconfig" "$T/url-rewrite.gitconfig"
cat > "$T/gitconfig" << G
[includeIf "gitdir:$T/cargo-home/git/"]
	path = $T/url-rewrite.gitconfig
G
export GIT_CONFIG_GLOBAL="$T/gitconfig"
export CARGO_HOME="$T/cargo-home"
export CARGO_NET_GIT_FETCH_WITH_CLI=true

push_url="$(git -C "$P5" remote get-url --push origin)"
ls_url="$(git -C "$P5" ls-remote --get-url origin)"
if [ "$push_url" = "$WANT" ] && [ "$ls_url" = "$WANT" ]; then
  echo "ok   (i) parse5-rust origin still $WANT (push=$push_url ls-remote=$ls_url)"
else
  echo "FAIL (i) origin rewritten: push=$push_url ls-remote=$ls_url want=$WANT"
  fail=1
fi

# Snapshot the repo outside /workspace so hierarchical cargo config cannot see the live [patch].
mkdir -p "$T/tree"
git -C "$ROOT" archive HEAD | tar -x -C "$T/tree"
# Working-tree locks win so this test covers an uncommitted D-0238 rewrite too.
cp "$ROOT/Cargo.lock" "$T/tree/Cargo.lock"
if [ -f "$ROOT/tools/template-diag-diff/driver/Cargo.lock" ]; then
  mkdir -p "$T/tree/tools/template-diag-diff/driver"
  cp "$ROOT/tools/template-diag-diff/driver/Cargo.lock" "$T/tree/tools/template-diag-diff/driver/Cargo.lock"
fi
if ! grep -q 'git+https://github.com/tariqrafique/parse5-rust?rev=' "$T/tree/Cargo.lock"; then
  echo "FAIL (ii) archived Cargo.lock has no parse5 git source (run after lock rewrite)"
  fail=1
else
  meta_err="$T/meta.err"
  if (cd "$T/tree" && cargo metadata --locked --format-version 1 >"$T/meta.json" 2>"$meta_err"); then
    node -e '
      const j=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));
      const p=j.packages.filter(p=>p.name==="parse5");
      if(p.length!==1){console.error("parse5 packages: "+p.length); process.exit(2)}
      const id=p[0].id, src=p[0].source||"";
      const want="e3520ba03c9af27eb8dd05d08960b094305910d8";
      if(!src.includes(want) && !id.includes(want)){
        console.error("parse5 id="+id+" source="+src+" missing "+want); process.exit(1)
      }
      console.log("ok   (ii) cargo resolved parse5 "+id+" source="+src);
    ' "$T/meta.json" || fail=1
  else
    echo "FAIL (ii) cargo metadata --locked failed: $(tail -5 "$meta_err" | tr "\n" " ")"
    fail=1
  fi
fi
exit $fail
