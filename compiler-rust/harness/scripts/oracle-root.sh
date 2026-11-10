# shellcheck shell=bash
# Thin wrappers: the implementation lives in oracle-root.mjs (AB-M0002).
_ORACLE_ROOT_MJS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/oracle-root.mjs"
oracle_root() {
  node "$_ORACLE_ROOT_MJS" oracle_root
}
style_corpus_root() {
  node "$_ORACLE_ROOT_MJS" style_corpus_root
}
notes_decisions_path() {
  node "$_ORACLE_ROOT_MJS" notes_decisions_path
}
oracle_pin() {
  node "$_ORACLE_ROOT_MJS" oracle_pin "${1:-}"
}
