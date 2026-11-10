#!/usr/bin/env bash
# Populate style-corpus/{lbc,lbc-recipes} from their public origins and verify
# harness/external-inputs.lock hashes (D-0280). Hashes exclude files under .git/.
#
#   lbc          npm lightning-base-components@1.28.19-alpha
#                https://registry.npmjs.org/lightning-base-components/-/lightning-base-components-1.28.19-alpha.tgz
#                (package.json license: MIT; LICENSE.txt is Salesforce Terms of Use —
#                we fetch the published npm tree, we do not vendor it.)
#   lbc-recipes  git https://github.com/salesforce/base-components-recipes.git
#                @ 9c5f95d4e0cd5434a2b8373405e4f6c7657c88e8
#                (LICENSE file: MIT; package.json license: CC0-1.0)
#
# STYLE_CORPUS_SOURCE, if set, copies those two leaves instead of fetching
# (still hash-verified, fail-closed). It is never required.
#
# Usage: scripts/fetch-style-corpus.sh [dest]
#   dest default: ${STYLE_CORPUS:-<repo>/.oracle/style-corpus}
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
lock="$root/harness/external-inputs.lock"
[ -f "$lock" ] || { echo "fetch-style-corpus: missing $lock" >&2; exit 2; }

LBC_NPM_TARBALL="https://registry.npmjs.org/lightning-base-components/-/lightning-base-components-1.28.19-alpha.tgz"
LBC_RECIPES_URL="https://github.com/salesforce/base-components-recipes.git"
LBC_RECIPES_REV="9c5f95d4e0cd5434a2b8373405e4f6c7657c88e8"

dest="${1:-${STYLE_CORPUS:-$root/.oracle/style-corpus}}"
dest="$(realpath -m "$dest")"
want_hash() { grep -v '^#' "$lock" | awk -F'\t' -v p="$1" '$1==p{print $2; found=1} END{exit !found}'; }

# Match official-board.sh inputhash1 (content-only: skip .git/).
# dest is the style-corpus directory, so we hash as style-corpus/<leaf>
# via a staging symlink when basename differs.
hash_leaf() {
  local leaf="$1" parent name staged
  parent="$(dirname "$dest")"
  name="$(basename "$dest")"
  if [ "$name" = style-corpus ]; then
    (cd "$parent" && find -H "style-corpus/$leaf" -type f ! -path '*/.git/*' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12
  else
    staged="$(mktemp -d)"
    mkdir -p "$staged/style-corpus"
    ln -s "$dest/$leaf" "$staged/style-corpus/$leaf"
    (cd "$staged" && find -H "style-corpus/$leaf" -type f ! -path '*/.git/*' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12
    rm -rf "$staged"
  fi
}

src="${STYLE_CORPUS_SOURCE:-}"

fetch_lbc() {
  local tmp
  tmp="$(mktemp -d)"
  echo "fetch-style-corpus: downloading $LBC_NPM_TARBALL" >&2
  curl -fsSL "$LBC_NPM_TARBALL" -o "$tmp/lbc.tgz" \
    || { echo "fetch-style-corpus: failed to download lightning-base-components tarball" >&2; rm -rf "$tmp"; return 2; }
  mkdir -p "$dest/lbc"
  tar -xzf "$tmp/lbc.tgz" -C "$dest/lbc" \
    || { echo "fetch-style-corpus: failed to extract lbc tarball" >&2; rm -rf "$tmp" "$dest/lbc"; return 2; }
  [ -f "$dest/lbc/package/package.json" ] \
    || { echo "fetch-style-corpus: lbc tarball missing package/package.json" >&2; rm -rf "$tmp" "$dest/lbc"; return 2; }
  rm -rf "$tmp"
}

fetch_recipes() {
  local tmp
  tmp="$(mktemp -d)"
  echo "fetch-style-corpus: fetching $LBC_RECIPES_URL @$LBC_RECIPES_REV" >&2
  git init -q "$tmp"
  if ! git -C "$tmp" fetch -q --depth 1 "$LBC_RECIPES_URL" "$LBC_RECIPES_REV"; then
    echo "fetch-style-corpus: git fetch of lbc-recipes failed" >&2
    rm -rf "$tmp"
    return 2
  fi
  git -C "$tmp" checkout -q --detach FETCH_HEAD
  [ "$(git -C "$tmp" rev-parse HEAD)" = "$LBC_RECIPES_REV" ] \
    || { echo "fetch-style-corpus: lbc-recipes HEAD != $LBC_RECIPES_REV" >&2; rm -rf "$tmp"; return 2; }
  rm -rf "$tmp/.git"
  mkdir -p "$dest"
  mv "$tmp" "$dest/lbc-recipes"
}

ensure_leaf() {
  local leaf="$1"
  if [ -d "$dest/$leaf" ]; then return 0; fi
  if [ -n "$src" ]; then
    [ -d "$src/$leaf" ] || { echo "fetch-style-corpus: $src/$leaf missing" >&2; return 2; }
    mkdir -p "$dest"
    cp -a "$src/$leaf" "$dest/$leaf"
    return 0
  fi
  mkdir -p "$dest"
  case "$leaf" in
    lbc) fetch_lbc ;;
    lbc-recipes) fetch_recipes ;;
    *) echo "fetch-style-corpus: unknown leaf $leaf" >&2; return 2 ;;
  esac
}

ensure_leaf lbc || exit $?
ensure_leaf lbc-recipes || exit $?
ok=1
for leaf in lbc lbc-recipes; do
  want="$(want_hash "style-corpus/$leaf")" || { echo "fetch-style-corpus: no lock row for style-corpus/$leaf" >&2; exit 2; }
  got="$(hash_leaf "$leaf")"
  if [ "$got" != "$want" ]; then
    echo "fetch-style-corpus: hash mismatch style-corpus/$leaf got=$got want=$want" >&2
    ok=0
  fi
done
[ "$ok" -eq 1 ] || exit 1
echo "fetch-style-corpus: $dest matches external-inputs.lock"
