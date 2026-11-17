#!/usr/bin/env bash
# D-0286: shared registry readers (main UNION branch additions) and D-id cites.
# Citation format (minimal, in the incoming registry file):
#   # D-NNNN: remove <key> [-- <one-line reason>]
#   # D-NNNN: loosen <key> [-- <one-line reason>]
# NNNN is 4 digits. <key> is the entry id: a board row name, a baseline row
# name, or an allowlist id (`path` or `path dep`). The key after the action
# must match exactly (no glob). A cite without a matching D-[0-9]{4} is ignored.
# Gate code sources this file from its own tree (main when land.sh main_run's).

registry_cited() {
  local file="$1" action="$2" key="$3" line rest
  [ -n "$file" ] && [ -f "$file" ] && [ -n "$action" ] && [ -n "$key" ] || return 1
  case "$action" in remove|loosen) ;; *) return 1 ;; esac
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      "# D-"[0-9][0-9][0-9][0-9]": $action "*)
        rest="${line#*: $action }"
        rest="${rest%% -- *}"   # optional ' -- <reason>' suffix
        [ "$rest" = "$key" ] && return 0
        ;;
    esac
  done < "$file"
  return 1
}

extract_board_rows() {
  local f="$1"
  [ -f "$f" ] || return 1
  awk '
    $0 ~ /^ROWS=\(/ {
      line = $0
      while (line !~ /\)/) {
        if ((getline nxt) <= 0) break
        line = line " " nxt
      }
      sub(/^ROWS=\(/, "", line)
      sub(/\).*$/, "", line)
      n = split(line, a, /[[:space:]]+/)
      for (i = 1; i <= n; i++) if (a[i] != "") print a[i]
    }
  ' "$f"
}

extract_board_diag() {
  local f="$1"
  [ -f "$f" ] || return 1
  awk '
    $0 ~ /^DIAG=/ {
      line = $0
      sub(/^[^=]*=/, "", line)
      gsub(/^[[:space:]]*"/, "", line)
      sub(/".*$/, "", line)
      print line
    }
  ' "$f"
}

# One recording line per stdout row: MODE<TAB>TEST<TAB>ENV
# Empty stdout = not a vitest/golden row (template-diag*).
board_row_golden_records() {
  local row="$1" mode
  case "$row" in
    template-diag|template-diag-*) return 0 ;;
    js-api) printf '%s\t%s\t%s\n' sync test/js-options.behavior.test.ts "" ;;
    template-renderer) printf '%s\t%s\t%s\n' sync test/template.behavior.test.ts "" ;;
    csr-render) printf '%s\t%s\t%s\n' sync test/csr.render.test.ts "CORPUS=ssr" ;;
    csr-render-template) printf '%s\t%s\t%s\n' sync test/csr.render.test.ts "CORPUS=template" ;;
    ssr-render)
      printf '%s\t%s\t%s\n' sync test/ssr.render.test.ts ""
      printf '%s\t%s\t%s\n' sync test/ssr.render.test.ts "SSR_RUNTIME=src"
      ;;
    ssr-render-*)
      mode="${row#ssr-render-}"
      printf '%s\t%s\t%s\n' "$mode" test/ssr.render.test.ts ""
      printf '%s\t%s\t%s\n' "$mode" test/ssr.render.test.ts "SSR_RUNTIME=src"
      ;;
    ssr-*)
      mode="${row#ssr-}"
      [ -n "$mode" ] || mode=sync
      printf '%s\t%s\t%s\n' "$mode" test/ssr.behavior.test.ts ""
      ;;
    *) printf '%s\t%s\t%s\n' sync "test/${row}.behavior.test.ts" "" ;;
  esac
}

# Union board-rows from main_file and branch_file. Prints unique rows in
# main-then-branch order. Fails (prints to stderr, returns 1) on an uncited
# removal or DIAG loosening.
union_board_rows() {
  local main_file="$1" branch_file="$2"
  local r main_rows branch_rows fail=0
  local -A in_branch=() in_main=() main_diag=() branch_diag=() seen=()
  main_rows="$(extract_board_rows "$main_file")" || return 2
  branch_rows="$(extract_board_rows "$branch_file")" || return 2
  while IFS= read -r r; do
    [ -n "$r" ] || continue
    in_branch["$r"]=1
  done <<< "$branch_rows"
  while IFS= read -r r; do
    [ -n "$r" ] || continue
    in_main["$r"]=1
    if [ -z "${in_branch[$r]:-}" ]; then
      if registry_cited "$branch_file" remove "$r"; then
        continue
      fi
      echo "FAIL board-rows removal without D-id cite (# D-NNNN: remove $r): $r" >&2
      fail=1
      continue
    fi
    if [ -z "${seen[$r]:-}" ]; then
      printf '%s\n' "$r"
      seen["$r"]=1
    fi
  done <<< "$main_rows"
  while IFS= read -r r; do
    [ -n "$r" ] || continue
    if [ -z "${seen[$r]:-}" ]; then
      printf '%s\n' "$r"
      seen["$r"]=1
    fi
  done <<< "$branch_rows"

  local d
  for d in $(extract_board_diag "$main_file"); do
    [ -n "$d" ] || continue
    main_diag["$d"]=1
  done
  for d in $(extract_board_diag "$branch_file"); do
    [ -n "$d" ] || continue
    branch_diag["$d"]=1
    if [ -n "${in_main[$d]:-}" ] && [ -z "${main_diag[$d]:-}" ]; then
      if ! registry_cited "$branch_file" loosen "$d"; then
        echo "FAIL board-rows DIAG loosening without D-id cite (# D-NNNN: loosen $d): $d" >&2
        fail=1
      fi
    fi
  done
  return "$fail"
}

baseline_row_names() {
  awk -F'\t' 'NF && $1 !~ /^#/ && $1 != "board-baseline/v2" { print $1 }' "$1" | sort -u
}

# D-0286(2) for keyed allowlists: union_keyed_registry <main> <incoming> <nkey>
# Prints MAIN's entry lines, then the incoming file's added entries. Comment and
# blank lines are skipped. The key is the first <nkey> tab fields joined by one
# space (e.g. `path` or `path line rule`). A MAIN key missing from the incoming
# file needs `# D-NNNN: remove <key>` in the incoming file, else FAIL on stderr
# and return 1. A missing incoming file counts as empty. A missing MAIN file is
# return 2.
union_keyed_registry() {
  local main_file="$1" incoming_file="$2" nkey="$3" line key fail=0 inc
  local -A in_inc=() seen=()
  [ -f "$main_file" ] || { echo "registry-union: no MAIN registry at $main_file" >&2; return 2; }
  [[ "$nkey" =~ ^[1-9]$ ]] || { echo "registry-union: bad key width '$nkey'" >&2; return 2; }
  inc="$incoming_file"; [ -f "$inc" ] || inc=/dev/null
  _registry_key() { awk -F'\t' -v n="$nkey" '{ k=$1; for (i = 2; i <= n; i++) k = k " " $i; print k }' <<< "$1"; }
  while IFS= read -r line || [ -n "$line" ]; do
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    in_inc["$(_registry_key "$line")"]=1
  done < "$inc"
  while IFS= read -r line || [ -n "$line" ]; do
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    key="$(_registry_key "$line")"
    if [ -z "${in_inc[$key]:-}" ]; then
      if registry_cited "$incoming_file" remove "$key"; then continue; fi
      echo "FAIL $(basename "$main_file") removal without D-id cite (# D-NNNN: remove $key): $key" >&2
      fail=1
      continue
    fi
    [ -n "${seen[$key]:-}" ] && continue
    seen["$key"]=1; printf '%s\n' "$line"
  done < "$main_file"
  while IFS= read -r line || [ -n "$line" ]; do
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    key="$(_registry_key "$line")"
    [ -n "${seen[$key]:-}" ] && continue
    seen["$key"]=1; printf '%s\n' "$line"
  done < "$inc"
  return "$fail"
}
