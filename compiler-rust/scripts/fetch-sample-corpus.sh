#!/usr/bin/env bash
# Fetch the pretty-equal sample corpora and Prettier 3.9.9, then check
# harness/external-inputs.lock (content-only, same hash as official-board.sh).
#
#   sample-corpus/lwc-recipes      git https://github.com/trailheadapps/lwc-recipes.git
#                                  @ b5af6ecb2f8d5d75d161b2a702d899f6e4ffc878
#   sample-corpus/ebikes-lwc       git https://github.com/trailheadapps/ebikes-lwc.git
#                                  @ ab1a3235365eb3a779589f5b971503ee6d62fe55
#   sample-corpus/dreamhouse-lwc   git https://github.com/trailheadapps/dreamhouse-lwc.git
#                                  @ a01e7c0d7393ba5b26087e44d76af33cd88e275f
#   sample-corpus/b2b-commerce-open-source-components
#                                  git https://github.com/forcedotcom/b2b-commerce-open-source-components.git
#                                  @ 8769c20beedfa5557b0e427e4b7706e359296779 (Apache-2.0)
#   sample-corpus/LightningFlowComponents
#                                  git https://github.com/UnofficialSF/LightningFlowComponents.git
#                                  @ 5de6df2c7ce5d23d50e45887c48920b1bfa4f511 (BSD-3)
#   sample-corpus/NPSP             git https://github.com/SalesforceFoundation/NPSP.git
#                                  @ f5072b44781e7f31cb396c6919cfa7d0ead0ec6a (BSD-3)
#   sample-corpus/visualforce-to-lwc
#                                  git https://github.com/trailheadapps/visualforce-to-lwc.git
#                                  @ 81001b70214843d1c10546b1a4c26a30c0412d1f (CC0)
#   sample-corpus/revenue-cloud-foundations
#                                  git https://github.com/SalesforceLabs/revenue-cloud-foundations.git
#                                  @ c8506522ba45bb2c0015d41fa5eace51eed1ff91 (Apache-2.0)
#   sample-corpus/lwc-utils        git https://github.com/tsalb/lwc-utils.git
#                                  @ c652548d17c537d24f9c16e46717ae95c9e36321 (BSD-3)
#   prettier/3.9.9                 npm prettier@3.9.9
#                                  https://registry.npmjs.org/prettier/-/prettier-3.9.9.tgz
#
# Destinations default to <repo>/.oracle/{sample-corpus,prettier/3.9.9}.
# SAMPLE_CORPUS and PRETTIER_ROOT override them. An existing directory is
# hash-checked and not re-downloaded. A missing directory is fetched.
#
# Usage: scripts/fetch-sample-corpus.sh [--lock <file>]
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
lock="$root/harness/external-inputs.lock"
while [ $# -gt 0 ]; do
  case "$1" in
    --lock) lock="${2:-}"; shift 2 ;;
    *) echo "usage: fetch-sample-corpus.sh [--lock <file>]" >&2; exit 2 ;;
  esac
done
[ -f "$lock" ] || { echo "fetch-sample-corpus: missing $lock" >&2; exit 2; }

LWC_RECIPES_URL="https://github.com/trailheadapps/lwc-recipes.git"
LWC_RECIPES_REV="b5af6ecb2f8d5d75d161b2a702d899f6e4ffc878"
EBIKES_URL="https://github.com/trailheadapps/ebikes-lwc.git"
EBIKES_REV="ab1a3235365eb3a779589f5b971503ee6d62fe55"
DREAMHOUSE_URL="https://github.com/trailheadapps/dreamhouse-lwc.git"
DREAMHOUSE_REV="a01e7c0d7393ba5b26087e44d76af33cd88e275f"
B2B_URL="https://github.com/forcedotcom/b2b-commerce-open-source-components.git"
B2B_REV="8769c20beedfa5557b0e427e4b7706e359296779"
FLOW_URL="https://github.com/UnofficialSF/LightningFlowComponents.git"
FLOW_REV="5de6df2c7ce5d23d50e45887c48920b1bfa4f511"
NPSP_URL="https://github.com/SalesforceFoundation/NPSP.git"
NPSP_REV="f5072b44781e7f31cb396c6919cfa7d0ead0ec6a"
VF_URL="https://github.com/trailheadapps/visualforce-to-lwc.git"
VF_REV="81001b70214843d1c10546b1a4c26a30c0412d1f"
RCF_URL="https://github.com/SalesforceLabs/revenue-cloud-foundations.git"
RCF_REV="c8506522ba45bb2c0015d41fa5eace51eed1ff91"
UTILS_URL="https://github.com/tsalb/lwc-utils.git"
UTILS_REV="c652548d17c537d24f9c16e46717ae95c9e36321"
PRETTIER_TARBALL="https://registry.npmjs.org/prettier/-/prettier-3.9.9.tgz"
PRETTIER_VERSION="3.9.9"

samples="${SAMPLE_CORPUS:-$root/.oracle/sample-corpus}"
prettier="${PRETTIER_ROOT:-$root/.oracle/prettier/3.9.9}"
samples="$(realpath -m "$samples")"
prettier="$(realpath -m "$prettier")"

want_hash() { grep -v '^#' "$lock" | awk -F'\t' -v p="$1" '$1==p{print $2; found=1} END{exit !found}'; }

# Match official-board inputhash1. $1 is the directory that contains the
# lock-relative path $2 (sample-corpus/<name> or prettier/3.9.9).
hash_rel() {
  local base="$1" rel="$2"
  (cd "$base" && find -H "$rel" -type f ! -path '*/.git/*' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -c1-12
}

fetch_git() {
  local url="$1" rev="$2" dest="$3" tmp
  tmp="$(mktemp -d)"
  echo "fetch-sample-corpus: fetching $url @$rev" >&2
  git init -q "$tmp"
  if ! git -C "$tmp" fetch -q --depth 1 "$url" "$rev"; then
    echo "fetch-sample-corpus: git fetch failed: $url" >&2
    rm -rf "$tmp"
    return 2
  fi
  git -C "$tmp" checkout -q --detach FETCH_HEAD
  [ "$(git -C "$tmp" rev-parse HEAD)" = "$rev" ] \
    || { echo "fetch-sample-corpus: HEAD != $rev for $url" >&2; rm -rf "$tmp"; return 2; }
  rm -rf "$tmp/.git"
  mkdir -p "$(dirname "$dest")"
  rm -rf "$dest"
  mv "$tmp" "$dest"
}

fetch_prettier() {
  local tmp
  tmp="$(mktemp -d)"
  echo "fetch-sample-corpus: downloading $PRETTIER_TARBALL" >&2
  curl -fsSL "$PRETTIER_TARBALL" -o "$tmp/prettier.tgz" \
    || { echo "fetch-sample-corpus: prettier download failed" >&2; rm -rf "$tmp"; return 2; }
  mkdir -p "$prettier"
  tar -xzf "$tmp/prettier.tgz" -C "$prettier" --strip-components=1 \
    || { echo "fetch-sample-corpus: prettier extract failed" >&2; rm -rf "$tmp" "$prettier"; return 2; }
  rm -rf "$tmp"
  python3 -c 'import json,sys; p=json.load(open(sys.argv[1]));
sys.exit(0 if p.get("name")=="prettier" and p.get("version")==sys.argv[2] else 1)' \
    "$prettier/package.json" "$PRETTIER_VERSION" \
    || { echo "fetch-sample-corpus: prettier package.json is not prettier@$PRETTIER_VERSION" >&2; return 2; }
}

[ -d "$samples/lwc-recipes" ] || fetch_git "$LWC_RECIPES_URL" "$LWC_RECIPES_REV" "$samples/lwc-recipes"
[ -d "$samples/ebikes-lwc" ] || fetch_git "$EBIKES_URL" "$EBIKES_REV" "$samples/ebikes-lwc"
[ -d "$samples/dreamhouse-lwc" ] || fetch_git "$DREAMHOUSE_URL" "$DREAMHOUSE_REV" "$samples/dreamhouse-lwc"
[ -d "$samples/b2b-commerce-open-source-components" ] || fetch_git "$B2B_URL" "$B2B_REV" "$samples/b2b-commerce-open-source-components"
[ -d "$samples/LightningFlowComponents" ] || fetch_git "$FLOW_URL" "$FLOW_REV" "$samples/LightningFlowComponents"
[ -d "$samples/NPSP" ] || fetch_git "$NPSP_URL" "$NPSP_REV" "$samples/NPSP"
[ -d "$samples/visualforce-to-lwc" ] || fetch_git "$VF_URL" "$VF_REV" "$samples/visualforce-to-lwc"
[ -d "$samples/revenue-cloud-foundations" ] || fetch_git "$RCF_URL" "$RCF_REV" "$samples/revenue-cloud-foundations"
[ -d "$samples/lwc-utils" ] || fetch_git "$UTILS_URL" "$UTILS_REV" "$samples/lwc-utils"
[ -f "$prettier/package.json" ] || fetch_prettier

# Lock paths are relative to one root that holds both sample-corpus/ and prettier/.
ext="$(dirname "$samples")"
[ "$(basename "$samples")" = sample-corpus ] \
  || { echo "fetch-sample-corpus: SAMPLE_CORPUS basename must be sample-corpus (got $samples)" >&2; exit 2; }
[ "$prettier" = "$ext/prettier/3.9.9" ] \
  || { echo "fetch-sample-corpus: PRETTIER_ROOT must be \$EXT/prettier/3.9.9 (got $prettier, ext $ext)" >&2; exit 2; }

ok=1
check() {
  local rel="$1" got want
  want="$(want_hash "$rel")" || { echo "fetch-sample-corpus: no lock row for $rel" >&2; exit 2; }
  got="$(hash_rel "$ext" "$rel")"
  if [ "$got" != "$want" ]; then
    echo "fetch-sample-corpus: hash mismatch $rel got=$got want=$want" >&2
    ok=0
  fi
}
check sample-corpus/lwc-recipes
check sample-corpus/ebikes-lwc
check sample-corpus/dreamhouse-lwc
check sample-corpus/b2b-commerce-open-source-components
check sample-corpus/LightningFlowComponents
check sample-corpus/NPSP
check sample-corpus/visualforce-to-lwc
check sample-corpus/revenue-cloud-foundations
check sample-corpus/lwc-utils
check prettier/3.9.9
[ "$ok" -eq 1 ] || exit 1
echo "fetch-sample-corpus: $ext matches external-inputs.lock"
