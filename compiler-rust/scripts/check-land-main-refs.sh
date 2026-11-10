#!/usr/bin/env bash
# D-0274(5)/D-0275/AB-L0079: land.sh may mention MAIN only inside the single
# main-chokepoint region, whose body sha256 is pinned in MAIN's table.
# Helper-arg rows: executed or sourced paths are MAIN_SCRIPT_ON_BRANCH_INPUT;
# MAIN_DATA_AS_BASE is only the table, harness/ratchets, and OWNERS.
# BOX_CONFIG_DEPENDENT rows must be logged by log_box_configs (required deps
# may not be absent). Bare unlocked-build tokens in scripts/*.sh and
# harness/scripts/*.sh must go through cargo_locked.
#
# Usage:
#   --self-test <root> --main-table FILE --incoming-table FILE
#   --check <land.sh> <incoming-tree> --main-table FILE --incoming-table FILE
# Missing table is rc 2. Unlisted/stale/forbidden token is rc 1.
set -euo pipefail

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALID_MAIN='MAIN_SCRIPT_ON_BRANCH_INPUT|MAIN_DATA_AS_BASE'
VALID_BOX='BOX_CONFIG_DEPENDENT'
VALID_REGION='REGION_SHA256'
VALID_NEXT='NEXT_REGION_SHA256'
VALID_DEP='~/.gitconfig|/workspace/.cargo/config.toml|CARGO_HOME|/workspace/.cargo/parse5-url-rewrite.gitconfig'
VALID_ABSENT='required|absent_ok'
WRAPPER_REL='scripts/cargo-locked.sh'
CHECKER_REL='scripts/check-land-main-refs.sh'

region_marker_report() {
  local file="$1"
  awk '
    $0=="# >>> main-chokepoint" { o++; if (o==1) oline=NR }
    $0=="# <<< main-chokepoint" { c++; if (c==1) cline=NR }
    END {
      if (o!=1 || c!=1 || oline==0 || cline==0 || oline>=cline) {
        print "want exactly one opener then one closer; openers=" o " closers=" c \
              " first_open=" oline " first_close=" cline
      }
    }
  ' "$file"
}

region_body() {
  awk '
    $0=="# >>> main-chokepoint" { o++; next }
    $0=="# <<< main-chokepoint" { c++; next }
    o==1 && c==0 { print }
  ' "$1"
}

region_sha() {
  region_body "$1" | sha256sum | awk '{print $1}'
}

region_leak_lines() {
  # An assignment that copies MAIN into another name (X="$MAIN", printf -v X ...).
  # MAIN=... or export MAIN=... is the only allowed write of the MAIN name.
  awk '
    $0=="# >>> main-chokepoint" { o++; next }
    $0=="# <<< main-chokepoint" { c++; next }
    o==1 && c==0 {
      { line=$0; gsub(/(^|[^A-Za-z0-9_])MAIN=/, " ", line); if (line ~ /(^|[^A-Za-z_])[A-Za-z_][A-Za-z0-9_]*="\$MAIN"/) print NR ": " $0 }
      if ($0 ~ /printf[[:space:]]+-v/ && $0 ~ /\$MAIN/) print NR ": " $0
    }
  ' "$1"
}

forbidden_main_lines() {
  local file="$1"
  awk '
    $0=="# >>> main-chokepoint" { in_block=1 }
    {
      if (!in_block && match($0, /\$\{MAIN_TREE\}|\$MAIN_TREE|\$\{MAIN\}|\$MAIN/)) {
        print NR ": " $0
      }
    }
    $0=="# <<< main-chokepoint" { in_block=0 }
  ' "$file"
}

indirect_main_lines() {
  local file="$1" n=0 line
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    case "$line" in
      *'${! '*) echo "$n: $line" ;;
      *'${!	'*) echo "$n: $line" ;;
      *'${!'*) echo "$n: $line" ;;
    esac
  done < "$file"
}

extract_helpers() {
  local file="$1" line rest t
  local re='main_(run|data)[[:space:]]+([A-Za-z0-9_./-]+)'
  while IFS= read -r line || [ -n "$line" ]; do
    t="${line#"${line%%[![:space:]]*}"}"
    case "$t" in ''|'#'*) continue ;; esac
    rest="$line"
    while [[ "$rest" =~ $re ]]; do
      printf '%s\t%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
      rest="${rest#*"${BASH_REMATCH[0]}"}"
    done
  done < "$file" | sort -u
}

line_box_deps() {
  local body="$1"
  if [[ "$body" =~ (^|[^A-Za-z0-9_])cargo_locked([^A-Za-z0-9_]|$) ]] \
     || { [[ ! "$body" =~ (^|[^A-Za-z0-9_])cargo(_locked|-locked) ]] \
          && [[ "$body" =~ (^|[^A-Za-z0-9_.])cargo([^A-Za-z0-9_-]|$) ]]; }; then
    printf '%s\n' '~/.gitconfig' '/workspace/.cargo/config.toml' 'CARGO_HOME' '/workspace/.cargo/parse5-url-rewrite.gitconfig'
  fi
  if [[ "$body" == *'~/.gitconfig'* ]] || [[ "$body" =~ \$HOME/.gitconfig ]] || [[ "$body" =~ \$\{HOME\}/.gitconfig ]]; then
    printf '%s\n' '~/.gitconfig'
  fi
  if [[ "$body" == *'/workspace/.cargo/config.toml'* ]] || [[ "$body" == *'git-fetch-with-cli'* ]]; then
    printf '%s\n' '/workspace/.cargo/config.toml'
  fi
  if [[ "$body" =~ (^|[^A-Za-z0-9_])CARGO_HOME([^A-Za-z0-9_]|$) ]]; then
    printf '%s\n' 'CARGO_HOME'
  fi
  if [[ "$body" == *'/workspace/.cargo/parse5-url-rewrite.gitconfig'* ]] || [[ "$body" == *'insteadOf'* ]]; then
    printf '%s\n' '/workspace/.cargo/parse5-url-rewrite.gitconfig'
  fi
}

extract_box_deps_for() {
  local file="$1"
  [ -f "$file" ] || { echo "check-land-main-refs: cannot scan missing file $file" >&2; return 2; }
  local line trimmed
  while IFS= read -r line || [ -n "$line" ]; do
    trimmed="${line#"${line%%[![:space:]]*}"}"
    case "$trimmed" in
      ''|'#'*) continue ;;
    esac
    line_box_deps "$trimmed"
  done < "$file" | sort -u
}

# Exact trimmed lines in this file that mention unlocked-build tokens as
# detector text (not substring). A planted unlocked invocation is not in this set.
# Pins are <line-number>:<sha256 of the trimmed line with no trailing newline>.
# A copy of the same text on another line is not exempt.
checker_pattern_hash() {
  printf '%s' "$1" | sha256sum | awk '{print $1}'
}

checker_pattern_line() {
  local lineno="$1" trimmed="$2" h
  h="$(checker_pattern_hash "$trimmed")"
  case "${lineno}:$h" in
    107:4f77354fccb9e915e5e471db68895c3bfeb2707f3e82948896cb56e297ccfb90) return 0 ;;
    108:fe15827a8eddb63d8e0cf3ea0884d08be3ceea8e9157735ba5b2ba0484748f5c) return 0 ;;
    165:20c9dcb6900672c4ee8d5378023c11ec303611effea4c0fd096b4b6c984689f7) return 0 ;;
    179:571741e090d15d1bb4eb057c77e529c67290bcb3a7dbc3bd6fba859cc325a24d) return 0 ;;
    193:ba2487fb82150f052ad96187d720f4b65ed622d3812894187251d77e11d32870) return 0 ;;
    196:275cbbf897792c8aed826a04d73ed1782497da80adf1d9bdd6a019db398165f3) return 0 ;;
    229:11e000b74cd0855afb7084752a66ca34d330259be540a73b091c2ce7a4fc494b) return 0 ;;
    241:3ab8eb07c657c005d1be2d9b2792b0b685e6d55e9e20c4b0a51242e46885e02e) return 0 ;;
    243:4e0af068b291f6f1b76988de0697f5462c64352c92cdfe6daf1c994500b2d87d) return 0 ;;
  esac
  return 1
}

unlocked_line_exempt() {
  local rel="$1" trimmed="$2" lineno="${3:-}"
  if [ "$rel" = "$WRAPPER_REL" ] && [ "$trimmed" = 'command cargo "$@"' ]; then
    return 0
  fi
  if [ "$rel" = "$CHECKER_REL" ] && checker_pattern_line "$lineno" "$trimmed"; then
    return 0
  fi
  return 1
}

# True if this line defines cargo_* or _cargo_exec, or aliases cargo / cargo_*.
# Forms: name(), name (), function name, function name(), function name (),
# with or without a same-line {; plus alias cargo= and alias cargo_*=.
unlocked_fn_def_or_alias() {
  local trimmed="$1"
  if [[ "$trimmed" =~ (^|[[:space:];|&])alias[[:space:]]+(cargo[_A-Za-z0-9]*)= ]]; then
    return 0
  fi
  if [[ "$trimmed" =~ (^|[[:space:];|&])function[[:space:]]+(cargo_[A-Za-z0-9_]+|_cargo_exec)([[:space:]]|\(|\{|;|$) ]]; then
    return 0
  fi
  if [[ "$trimmed" =~ (^|[[:space:];|&])(cargo_[A-Za-z0-9_]+|_cargo_exec)[[:space:]]*\(\) ]]; then
    return 0
  fi
  return 1
}

allowed_generate_lockfile_line() {
  local rel="$1" trimmed="$2"
  if [ "$rel" = "$WRAPPER_REL" ] && [[ "$trimmed" =~ cargo_generate_lockfile[[:space:]]*\(\) ]]; then
    return 0
  fi
  if [ "$rel" = "scripts/test-parse5-pin.sh" ] && ! [[ "$trimmed" =~ cargo_generate_lockfile[[:space:]]*\(\) ]]; then
    return 0
  fi
  return 1
}

# Bare unlocked-build tokens, plus cargo_locked() defined outside the wrapper.
forbidden_cargo_lines() {
  local file="$1" rel="${2:-}"
  [ -f "$file" ] || return 0
  local line trimmed n=0
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    trimmed="${line#"${line%%[![:space:]]*}"}"
    case "$trimmed" in
      ''|'#'*) continue ;;
    esac
    if unlocked_line_exempt "$rel" "$trimmed" "$n"; then
      continue
    fi
    if unlocked_fn_def_or_alias "$trimmed"; then
      if [ "$rel" != "$WRAPPER_REL" ]; then
        echo "$n: $trimmed"
      fi
      continue
    fi
    if [[ "$trimmed" =~ (^|[^A-Za-z0-9_])cargo_[A-Za-z0-9_]+ ]]; then
      local tmp ident badstar=0
      tmp="$trimmed"
      while [[ "$tmp" =~ (^|[^A-Za-z0-9_])(cargo_[A-Za-z0-9_]+) ]]; do
        ident="${BASH_REMATCH[2]}"
        case "$ident" in
          cargo_locked) ;;
          cargo_generate_lockfile)
            allowed_generate_lockfile_line "$rel" "$trimmed" || badstar=1
            ;;
          *) badstar=1 ;;
        esac
        tmp="${tmp#*"$ident"}"
      done
      if [ "$badstar" -ne 0 ]; then
        echo "$n: $trimmed"
        continue
      fi
    fi
    if [[ "$trimmed" =~ (^|[^A-Za-z0-9_.])cargo([^A-Za-z0-9_-]|$) ]]; then
      echo "$n: $trimmed"
    elif [[ "$trimmed" =~ (^|[^A-Za-z0-9_])\$CARGO([^A-Za-z0-9_]|$) ]]; then
      echo "$n: $trimmed"
    elif [ "$rel" = "scripts/napi-pack.sh" ] \
        && [[ "$trimmed" =~ ^npm[[:space:]]+run[[:space:]]+build[[:space:]]+--[[:space:]]+--[[:space:]]*cargo-flags=--locked ]]; then
      continue
    elif [[ "$trimmed" =~ (^|[;|&][[:space:]]*)napi[[:space:]]+build([[:space:]]|$) ]] \
        || [[ "$trimmed" =~ (^|[;|&][[:space:]]*)npx[[:space:]]+napi([[:space:]]|$) ]] \
        || [[ "$trimmed" =~ (^|[;|&][[:space:]]*)npm[[:space:]]+run[[:space:]]+build([[:space:]]|$) ]] \
        || [[ "$trimmed" =~ (^|[;|&][[:space:]]*)npm[[:space:]]+test([[:space:]]|$) ]]; then
      echo "$n: $trimmed"
    fi
  done < "$file"
}

resolve_file() {
  local rel="$1" overlay="$2" root="$3"
  if [ -n "$overlay" ] && [ -f "$overlay/$rel" ]; then
    printf '%s\n' "$overlay/$rel"
  elif [ -f "$root/$rel" ]; then
    printf '%s\n' "$root/$rel"
  fi
}

collect_sh_rels() {
  local root="$1" helper rel
  helper="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/tracked-files.sh"
  {
    if [ -x "$helper" ]; then
      while IFS= read -r -d '' rel; do
        case "$rel" in
          *.sh) printf '%s\n' "$rel" ;;
        esac
      done < <("$helper" "$root" scripts harness/scripts)
    else
      echo "check-land-main-refs: missing $helper" >&2
      return 2
    fi
    printf '%s\n' "harness/board.sh" "harness/board-rows.sh"
  } | sort -u
}

load_allowlist() {
  local file="$1"
  [ -f "$file" ] || { echo "check-land-main-refs: no allowlist at $file" >&2; return 2; }
  awk -F'\t' '
    /^#/ || NF==0 { next }
    NF!=4 { print "malformed:" NR ":" $0 > "/dev/stderr"; bad=1; next }
    { print $1 "\t" $2 "\t" $3 "\t" $4 }
    END { if (bad) exit 2 }
  ' "$file"
}

helpers_to_listed() {
  awk -F'\t' '
    $1=="run" { print $2 "\tMAIN_SCRIPT_ON_BRANCH_INPUT" }
    $1=="data" && $2 ~ /\.sh$/ { print $2 "\tMAIN_SCRIPT_ON_BRANCH_INPUT" }
    $1=="data" && $2 !~ /\.sh$/ { print $2 "\tMAIN_DATA_AS_BASE" }
  ' | sort -u
}

# Two real call sites: log_box_configs before the first cargo_locked call,
# and another log_box_configs after that and before main_run harness/board.sh.
assert_box_log_call_sites() {
  local file="$1"
  local n=0 line t
  local locked_call_line=0 board_line=0 fn_depth=0
  local -a logs=()
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    t="${line#"${line%%[![:space:]]*}"}"
    case "$t" in ''|'#'*) continue ;; esac
    # Track function bodies so a column-0 call inside `f() { ... }` does not count.
    if [[ "$t" =~ ^(function[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*[[:space:]]*(\(\)[[:space:]]*)?\{ ]]; then
      fn_depth=$((fn_depth + 1))
      if [[ "$t" == *'}'* ]]; then
        fn_depth=$((fn_depth - 1))
      fi
    elif [ "$fn_depth" -gt 0 ] && [ "$t" = '}' ]; then
      fn_depth=$((fn_depth - 1))
    fi
    if [[ "$t" =~ ^log_box_configs[[:space:]]*\(\) ]]; then
      continue
    fi
    # Outside any function: indented / `if false` / unused-function calls do not count.
    if [ "$fn_depth" -eq 0 ] && [[ "$line" =~ ^log_box_configs([[:space:]]|$) ]]; then
      logs+=("$n")
    fi
    if [ "$locked_call_line" -eq 0 ] && [[ "$t" =~ (^|[[:space:];|&])cargo_locked([[:space:]]|$) ]] \
       && ! [[ "$t" =~ cargo_locked[[:space:]]*\(\) ]]; then
      locked_call_line=$n
    fi
    if [[ "$t" =~ main_run[[:space:]]+harness/board\.sh ]]; then
      board_line=$n
    fi
  done < "$file"
  if [ "$locked_call_line" -eq 0 ]; then
    echo "FAIL land.sh has no cargo_locked call site"
    return 1
  fi
  if [ "$board_line" -eq 0 ]; then
    echo "FAIL land.sh has no main_run harness/board.sh call site"
    return 1
  fi
  local before_cargo=0 before_board=0 logn
  for logn in "${logs[@]+"${logs[@]}"}"; do
    if [ "$logn" -lt "$locked_call_line" ]; then before_cargo=1; fi
    if [ "$logn" -gt "$locked_call_line" ] && [ "$logn" -lt "$board_line" ]; then before_board=1; fi
  done
  if [ "$before_cargo" -ne 1 ] || [ "$before_board" -ne 1 ]; then
    echo "FAIL land.sh must call log_box_configs before the first cargo_locked ($locked_call_line) and again before board.sh ($board_line); calls: ${logs[*]:-none}"
    return 1
  fi
  return 0
}


# AB-M0001: stale = (incoming ∩ MAIN) − found; miss = found − MAIN.
# Callers pass both tables explicitly (--main-table / --incoming-table).

# Plants copy $allow as the synthetic MAIN table. A tree that pre-lists unused
# incoming-only rows (table-only land) would then show those rows on both sides
# and fail as stale. Drop unused MAIN_* / BOX_CONFIG_DEPENDENT rows so the
# plant baseline matches found for this tree. REGION / NEXT rows are kept.
write_self_test_plant_allow() {
  local src="$1" root="$2" dest="$3"
  local land="$root/scripts/land.sh"
  local found_classes box_found="" rel f deps d
  found_classes="$(extract_helpers "$land" | helpers_to_listed)"
  while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    case "$rel" in
      scripts/check-land-main-refs.sh|scripts/test-*.sh) continue ;;
    esac
    if [ "$rel" = "scripts/land.sh" ]; then
      f="$land"
    else
      f="$root/$rel"
    fi
    [ -f "$f" ] || continue
    deps="$(extract_box_deps_for "$f")" || return 2
    [ -n "$deps" ] || continue
    while IFS= read -r d; do
      [ -n "$d" ] || continue
      box_found+="$rel"$'	'"$d"$'
'
    done <<< "$deps"
  done < <(collect_sh_rels "$root")
  box_found="$(printf '%s' "$box_found" | sort -u)"
  awk -F'	' -v okm="$VALID_MAIN" -v okb="$VALID_BOX" \
      -v foundm="$found_classes" -v foundb="$box_found" '
    BEGIN {
      n=split(foundm, a, "\n"); for (i=1;i<=n;i++) if (a[i] != "") mm[a[i]]=1
      n=split(foundb, a, "\n"); for (i=1;i<=n;i++) if (a[i] != "") bb[a[i]]=1
    }
    /^#/ || NF==0 { print; next }
    $2 ~ "^("okm")$" { if (($1 "\t" $2) in mm) print; next }
    $2 == okb { if (($1 "\t" $3) in bb) print; next }
    { print }
  ' "$src" > "$dest"
}

check() {
  local land="$1" main_table="$2" root="$3" incoming_table="$4" overlay="${5:-}"
  local fail=0
  [ -f "$land" ] || { echo "check-land-main-refs: no land.sh at $land" >&2; return 2; }
  local rows
  [ -n "${main_table:-}" ] && [ -f "$main_table" ] || { echo "check-land-main-refs: no MAIN allowlist at ${main_table:-<empty>}" >&2; return 2; }
  rows="$(load_allowlist "$main_table")" || return 2

  local markers
  markers="$(region_marker_report "$land")"
  if [ -n "$markers" ]; then
    echo "FAIL main-chokepoint region: $markers"
    fail=1
  fi

  local forbidden
  forbidden="$(forbidden_main_lines "$land")"
  if [ -n "$forbidden" ]; then
    echo "FAIL forbidden MAIN token(s) outside the main-chokepoint helpers:"
    printf '%s\n' "$forbidden" | sed 's/^/  /'
    fail=1
  fi

  local indirect
  indirect="$(indirect_main_lines "$land")"
  if [ -n "$indirect" ]; then
    echo "FAIL indirect expansion \${! in land.sh:"
    printf '%s\n' "$indirect" | sed 's/^/  /'
    fail=1
  fi

  local leaks
  leaks="$(region_leak_lines "$land")"
  if [ -n "$leaks" ]; then
    echo "FAIL main-chokepoint region leaks MAIN into another variable:"
    printf '%s\n' "$leaks" | sed 's/^/  /'
    fail=1
  fi

  local helpers found_classes
  helpers="$(extract_helpers "$land")"
  found_classes="$(printf '%s\n' "$helpers" | helpers_to_listed)"

  local path class dep policy pinned="" nextpin=""
  local nregion=0 nnext=0
  while IFS=$'\t' read -r path class dep policy; do
    [ -n "${path:-}" ] || continue
    if [[ "$class" =~ ^($VALID_MAIN)$ ]]; then
      [ "$dep" = "-" ] || { echo "FAIL main-ref $path ($class): depends_on must be '-', got '$dep'"; fail=1; }
      [ "$policy" = "-" ] || { echo "FAIL main-ref $path ($class): absent_ok must be '-', got '$policy'"; fail=1; }
    elif [ "$class" = "$VALID_BOX" ]; then
      if ! [[ "$dep" =~ ^($VALID_DEP)$ ]]; then
        echo "FAIL box-config $path: unknown depends_on '$dep'"
        fail=1
      fi
      if ! [[ "$policy" =~ ^($VALID_ABSENT)$ ]]; then
        echo "FAIL box-config $path $dep: absent_ok must be required or absent_ok, got '$policy'"
        fail=1
      fi
    elif [ "$class" = "$VALID_REGION" ]; then
      nregion=$((nregion + 1))
      if ! [[ "$dep" =~ ^[0-9a-f]{64}$ ]]; then
        echo "FAIL REGION_SHA256 depends_on must be a 64-hex digest, got '$dep'"
        fail=1
      fi
      [ "$policy" = "-" ] || { echo "FAIL REGION_SHA256 absent_ok must be '-', got '$policy'"; fail=1; }
      pinned="$dep"
    elif [ "$class" = "$VALID_NEXT" ]; then
      nnext=$((nnext + 1))
      if ! [[ "$dep" =~ ^[0-9a-f]{64}$ ]]; then
        echo "FAIL NEXT_REGION_SHA256 depends_on must be a 64-hex digest, got '$dep'"
        fail=1
      fi
      [ "$policy" = "-" ] || { echo "FAIL NEXT_REGION_SHA256 absent_ok must be '-', got '$policy'"; fail=1; }
      nextpin="$dep"
    else
      echo "FAIL allowlist $path: unknown class '$class'"
      fail=1
    fi
  done <<< "$rows"
  if [ "$nregion" -ne 1 ]; then
    echo "FAIL allowlist must have exactly one REGION_SHA256 row (have $nregion)"
    fail=1
  elif [ "$nnext" -gt 1 ]; then
    echo "FAIL allowlist must have at most one NEXT_REGION_SHA256 row (have $nnext)"
    fail=1
  elif [ -n "$pinned" ]; then
    local got
    got="$(region_sha "$land")"
    if [ "$got" != "$pinned" ] && { [ -z "$nextpin" ] || [ "$got" != "$nextpin" ]; }; then
      echo "FAIL REGION_SHA256 mismatch: table=$pinned next=${nextpin:-none} land=$got"
      fail=1
    fi
  fi

  # Incoming tree's table: NEXT equal to this land.sh body is an unpromoted step 2.
  # Production --check still accepts a body that matches MAIN's NEXT (that is step 2
  # against MAIN). The branch table must not still carry that NEXT.
  # Missing incoming table is rc 2. Unlisted helpers stay vs MAIN's table.
  # Stale MAIN_* (path+class) and BOX_CONFIG_DEPENDENT (path+dep) rows are
  # (incoming ∩ MAIN) minus found uses: a MAIN-only drop and an incoming-only
  # pending addition are accepted; a row in both that is unused fails.
  local inext igot
  if [ -z "${incoming_table:-}" ] || [ ! -f "$incoming_table" ]; then
    echo "check-land-main-refs: no incoming allowlist at ${incoming_table:-<empty>}" >&2
    return 2
  fi
  inext="$(awk -F'\t' '$2=="NEXT_REGION_SHA256"{print $3; exit}' "$incoming_table")"
  if [ -n "$inext" ]; then
    igot="$(region_sha "$land")"
    if [ "$igot" = "$inext" ]; then
      echo "FAIL NEXT_REGION_SHA256 equals this tree's region body (unpromoted step 2); promote it into REGION_SHA256 and delete the NEXT row"
      fail=1
    fi
  fi

  local dups
  dups="$(awk -F'\t' -v ok="$VALID_MAIN" '$2 ~ "^("ok")$" {print $1}' <<< "$rows" | sort | uniq -d)"
  [ -z "$dups" ] || { echo "FAIL allowlist duplicate MAIN_* path(s): $dups"; fail=1; }

  local listed listed_in listed_both miss extra
  listed="$(awk -F'\t' -v ok="$VALID_MAIN" '$2 ~ "^("ok")$" {print $1 "\t" $2}' <<< "$rows" | sort -u)"
  listed_in="$(awk -F'\t' -v ok="$VALID_MAIN" '$2 ~ "^("ok")$" {print $1 "\t" $2}' "$incoming_table" | sort -u)"
  listed_both="$(comm -12 <(printf '%s\n' "$listed") <(printf '%s\n' "$listed_in"))"
  miss="$(comm -13 <(printf '%s\n' "$listed") <(printf '%s\n' "$found_classes"))"
  extra="$(comm -23 <(printf '%s\n' "$listed_both") <(printf '%s\n' "$found_classes"))"
  [ -z "$miss" ] || { echo "FAIL unlisted helper arg(s):"; printf '%s\n' "$miss" | sed 's/^/  /'; fail=1; }
  [ -z "$extra" ] || { echo "FAIL stale allowlist MAIN_* row(s):"; printf '%s\n' "$extra" | sed 's/^/  /'; fail=1; }

  if ! assert_box_log_call_sites "$land"; then
    fail=1
  fi

  local box_found="" rel f deps d token_hits
  while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    if [ "$rel" = "scripts/land.sh" ]; then
      f="$land"
    else
      f="$(resolve_file "$rel" "$overlay" "$root")"
    fi
    [ -n "${f:-}" ] && [ -f "$f" ] || continue
    token_hits="$(forbidden_cargo_lines "$f" "$rel")"
    if [ -n "$token_hits" ]; then
      echo "FAIL bare unlocked-build token(s) in $rel (must use the locked wrapper):"
      printf '%s\n' "$token_hits" | sed 's/^/  /'
      fail=1
    fi
    case "$rel" in
      scripts/check-land-main-refs.sh|scripts/test-*.sh) continue ;;
    esac
    deps="$(extract_box_deps_for "$f")" || return 2
    [ -n "$deps" ] || continue
    while IFS= read -r d; do
      [ -n "$d" ] || continue
      box_found+="$rel"$'\t'"$d"$'\n'
    done <<< "$deps"
  done < <(collect_sh_rels "$root")

  box_found="$(printf '%s' "$box_found" | sort -u)"
  local box_listed box_listed_in box_listed_both
  box_listed="$(awk -F'\t' -v ok="$VALID_BOX" '$2==ok {print $1 "\t" $3}' <<< "$rows" | sort -u)"
  box_listed_in="$(awk -F'\t' -v ok="$VALID_BOX" '$2==ok {print $1 "\t" $3}' "$incoming_table" | sort -u)"
  box_listed_both="$(comm -12 <(printf '%s\n' "$box_listed") <(printf '%s\n' "$box_listed_in"))"

  miss="$(comm -13 <(printf '%s\n' "$box_listed") <(printf '%s\n' "$box_found"))"
  extra="$(comm -23 <(printf '%s\n' "$box_listed_both") <(printf '%s\n' "$box_found"))"
  [ -z "$miss" ] || { echo "FAIL unlisted box-config dependency:"; printf '%s\n' "$miss" | sed 's/^/  /'; fail=1; }
  [ -z "$extra" ] || { echo "FAIL stale allowlist BOX_CONFIG_DEPENDENT row(s):"; printf '%s\n' "$extra" | sed 's/^/  /'; fail=1; }

  if [ "$fail" -eq 0 ]; then
    local nfound nbox
    nfound="$(printf '%s\n' "$found_classes" | grep -c . || true)"
    nbox="$(printf '%s\n' "$box_found" | grep -c . || true)"
    echo "check-land-main-refs: ok ($nfound helper args, $nbox box-config deps)"
  fi
  return "$fail"
}

self_test() {
  local root="$1" main_table="$2" incoming_table="$3"
  local land="$root/scripts/land.sh" tree_allow="$root/scripts/land-main-refs.tsv"
  local allow="$tree_allow"
  local T fail=0
  T="$(mktemp -d /tmp/main-refs.XXXXXX)"
  trap 'rm -rf "$T"' RETURN

  [ -n "${main_table:-}" ] && [ -f "$main_table" ] || { echo "usage: --self-test <root> --main-table FILE --incoming-table FILE" >&2; return 2; }
  [ -n "${incoming_table:-}" ] && [ -f "$incoming_table" ] || { echo "usage: --self-test <root> --main-table FILE --incoming-table FILE" >&2; return 2; }

  if check "$land" "$main_table" "$root" "$incoming_table"; then
    echo "ok   live land.sh matches allowlist"
  else
    echo "FAIL live land.sh does not match allowlist"; fail=1
  fi

  write_self_test_plant_allow "$tree_allow" "$root" "$T/plant.allow" || return 2
  allow="$T/plant.allow"

  incoming_table_for() {
    local overlay="${1:-}" r="${2:-$root}"
    if [ -n "$overlay" ] && [ -f "$overlay/scripts/land-main-refs.tsv" ]; then
      printf '%s\n' "$overlay/scripts/land-main-refs.tsv"
    else
      printf '%s\n' "$r/scripts/land-main-refs.tsv"
    fi
  }
  chk() {
    local _land="$1" _mt="$2" _r="$3" _ov="${4:-}"
    check "$_land" "$_mt" "$_r" "$(incoming_table_for "$_ov" "$_r")" "$_ov"
  }

  local plant name
  declare -A plants=(
    ['${MAIN}/']='"${MAIN}/scripts/planted-c.sh"'
    ['${MAIN_TREE}/']='"${MAIN_TREE}/scripts/planted-d.sh"'
    ['"$MAIN"/p']='"$MAIN"/p'
    ['$M/']='M="$MAIN"; "$M/scripts/planted-f.sh"'
    ['$(dirname "$MAIN")/']='$(dirname "$MAIN")/scripts/planted-g.sh'
    ['"$MAIN/$s"']='"$MAIN/$s"'
    ['cd "$MAIN" && ./x']='cd "$MAIN" && ./scripts/planted-j.sh'
  )
  for name in '${MAIN}/' '${MAIN_TREE}/' '"$MAIN"/p' '$M/' '$(dirname "$MAIN")/' '"$MAIN/$s"' 'cd "$MAIN" && ./x'; do
    plant="${plants[$name]}"
    cp "$land" "$T/land.sh"
    printf '\n%s\n' "$plant" >> "$T/land.sh"
    if chk "$T/land.sh" "$allow" "$root" >"$T/out.plant" 2>&1; then
      echo "FAIL planted MAIN spelling $name was accepted"; sed 's/^/       /' "$T/out.plant"; fail=1
    elif grep -q 'forbidden MAIN token' "$T/out.plant"; then
      echo "ok   planted MAIN spelling $name fails"
    else
      echo "FAIL planted MAIN spelling $name: want 'forbidden MAIN token'"; sed 's/^/       /' "$T/out.plant"; fail=1
    fi
  done

  cp "$land" "$T/land.sh"
  printf '\n# >>> main-chokepoint\n"$MAIN/scripts/planted-second.sh"\n# <<< main-chokepoint\n' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.second" 2>&1; then
    echo "FAIL second opener was accepted"; sed 's/^/       /' "$T/out.second"; fail=1
  elif grep -q 'main-chokepoint region' "$T/out.second"; then
    echo "ok   planted second opener fails"
  else
    echo "FAIL second opener: want region marker error"; sed 's/^/       /' "$T/out.second"; fail=1
  fi

  cp "$land" "$T/land.sh"
  awk '$0=="# <<< main-chokepoint"{next} {print}' "$land" > "$T/land.sh"
  printf '\n"$MAIN/scripts/planted-expanded.sh"\n# <<< main-chokepoint\n' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.moved" 2>&1; then
    echo "FAIL closer moved to EOF was accepted"; sed 's/^/       /' "$T/out.moved"; fail=1
  elif grep -q 'REGION_SHA256 mismatch' "$T/out.moved"; then
    echo "ok   planted closer moved to EOF fails"
  else
    echo "FAIL moved closer: want REGION_SHA256 mismatch"; sed 's/^/       /' "$T/out.moved"; fail=1
  fi

  awk '$0=="# <<< main-chokepoint"{print "X=\"$MAIN\""} {print}' "$land" > "$T/land.sh"
  printf '\n"$X/scripts/planted-alias.sh"\n' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.alias" 2>&1; then
    echo "FAIL region alias X=\"\$MAIN\" was accepted"; sed 's/^/       /' "$T/out.alias"; fail=1
  elif grep -qE 'REGION_SHA256 mismatch|leaks MAIN' "$T/out.alias"; then
    echo "ok   planted region alias X=\"\$MAIN\" fails"
  else
    echo "FAIL region alias: want REGION_SHA256 mismatch or leak"; sed 's/^/       /' "$T/out.alias"; fail=1
  fi

  cp "$land" "$T/land.sh"
  printf '\nv=MAIN; echo "${!v}/scripts/planted-indirect.sh"\n' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.ind" 2>&1; then
    echo "FAIL indirect \${!v} was accepted"; sed 's/^/       /' "$T/out.ind"; fail=1
  elif grep -q 'indirect expansion' "$T/out.ind"; then
    echo "ok   planted indirect \${!v} fails"
  else
    echo "FAIL indirect: want indirect expansion"; sed 's/^/       /' "$T/out.ind"; fail=1
  fi

  awk '$0=="# <<< main-chokepoint"{print "# planted region-body edit"} {print}' "$land" > "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.edit" 2>&1; then
    echo "FAIL edited region body with unchanged table was accepted"; sed 's/^/       /' "$T/out.edit"; fail=1
  elif grep -q 'REGION_SHA256 mismatch' "$T/out.edit"; then
    echo "ok   planted edited region body with unchanged table fails"
  else
    echo "FAIL edited region: want REGION_SHA256 mismatch"; sed 's/^/       /' "$T/out.edit"; fail=1
  fi

  cp "$land" "$T/land.sh"
  printf '\n%s() { command %s \"$@\"; }\n' cargo_locked c'argo' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.redef" 2>&1; then
    echo "FAIL cargo_locked one-line redefine was accepted"; sed 's/^/       /' "$T/out.redef"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.redef"; then
    echo "ok   planted cargo_locked one-line redefine fails"
  else
    echo "FAIL redefine: want bare unlocked-build token"; sed 's/^/       /' "$T/out.redef"; fail=1
  fi

  mkdir -p "$T/overlay/scripts"
  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\n%s %s\n' n'pm' te'st' >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.npm" 2>&1; then
    echo "FAIL planted n"'pm'" te"'st'" was accepted"; sed 's/^/       /' "$T/out.npm"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.npm" && grep -q 'lint-shell-pipes.sh' "$T/out.npm"; then
    echo "ok   planted n"'pm'" te"'st'" fails"
  else
    echo "FAIL npm test: want bare unlocked-build token"; sed 's/^/       /' "$T/out.npm"; fail=1
  fi

  mkdir -p "$T/overlay/scripts"
  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\necho "napi-pack: Cargo.lock changed during %s %s (not --locked-clean)"\n' na'pi' bu'ild' >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.napiprose" 2>&1; then
    echo "ok   error string mentioning "napi" "build" is not an unlocked invocation"
  else
    echo "FAIL prose "napi" "build" should be accepted"; sed 's/^/       /' "$T/out.napiprose"; fail=1
  fi

  mkdir -p "$T/overlay/scripts"
  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\n%s %s\n' 'c'argo bu'ild' >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.cargobuild" 2>&1; then
    echo "FAIL planted "'c'argo" "bu'ild'" invocation was accepted"; sed 's/^/       /' "$T/out.cargobuild"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.cargobuild" && grep -q 'lint-shell-pipes.sh' "$T/out.cargobuild"; then
    echo "ok   planted "'c'argo" "bu'ild'" invocation fails"
  else
    echo "FAIL "'c'argo" "bu'ild'": want bare unlocked-build token"; sed 's/^/       /' "$T/out.cargobuild"; fail=1
  fi

  cp "$land" "$T/land.sh"
  printf '\nmain_run scripts/not-listed.sh || true\n' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.unlisted" 2>&1; then
    echo "FAIL added unlisted helper arg was accepted"; sed 's/^/       /' "$T/out.unlisted"; fail=1
  elif grep -q 'unlisted helper arg' "$T/out.unlisted" && grep -q 'scripts/not-listed.sh' "$T/out.unlisted"; then
    echo "ok   added unlisted helper arg fails"
  else
    echo "FAIL unlisted helper: want scripts/not-listed.sh"; sed 's/^/       /' "$T/out.unlisted"; fail=1
  fi

  mkdir -p "$T/in.stale/scripts"
  cp "$allow" "$T/in.stale/scripts/land-main-refs.tsv"
  printf 'scripts/gone.sh\tMAIN_SCRIPT_ON_BRANCH_INPUT\t-\t-\n' >> "$T/in.stale/scripts/land-main-refs.tsv"
  if chk "$land" "$allow" "$root" "$T/in.stale" >"$T/out.stale" 2>&1; then
    echo "ok   incoming-only unused MAIN_* row is accepted"
  else
    echo "FAIL incoming-only unused MAIN_* row should be accepted"; sed 's/^/       /' "$T/out.stale"; fail=1
  fi

  mkdir -p "$T/in.bothstale/scripts"
  cp "$allow" "$T/allow.bothstale"
  printf 'scripts/gone.sh\tMAIN_SCRIPT_ON_BRANCH_INPUT\t-\t-\n' >> "$T/allow.bothstale"
  cp "$T/allow.bothstale" "$T/in.bothstale/scripts/land-main-refs.tsv"
  if chk "$land" "$T/allow.bothstale" "$root" "$T/in.bothstale" >"$T/out.bothstale" 2>&1; then
    echo "FAIL unused MAIN_* row in both tables was accepted"; sed 's/^/       /' "$T/out.bothstale"; fail=1
  elif grep -q 'stale allowlist MAIN_' "$T/out.bothstale" && grep -q 'scripts/gone.sh' "$T/out.bothstale"; then
    echo "ok   unused MAIN_* row in both tables fails"
  else
    echo "FAIL unused MAIN_* in both: want scripts/gone.sh"; sed 's/^/       /' "$T/out.bothstale"; fail=1
  fi

  cp "$allow" "$T/allow.mainextra"
  printf 'scripts/gone.sh\tMAIN_SCRIPT_ON_BRANCH_INPUT\t-\t-\n' >> "$T/allow.mainextra"
  if chk "$land" "$T/allow.mainextra" "$root" >"$T/out.mainextra" 2>&1; then
    echo "ok   MAIN table extra row dropped from incoming is accepted"
  else
    echo "FAIL MAIN extra / incoming dropped should be accepted"; sed 's/^/       /' "$T/out.mainextra"; fail=1
  fi

  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\n: "${CARGO_HOME:=/tmp}"\n' >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.box" 2>&1; then
    echo "FAIL unlisted box-config dependency was accepted"; sed 's/^/       /' "$T/out.box"; fail=1
  elif grep -q 'unlisted box-config' "$T/out.box" && grep -q 'lint-shell-pipes.sh' "$T/out.box" && grep -q 'CARGO_HOME' "$T/out.box"; then
    echo "ok   added unlisted box-config dependency fails"
  else
    echo "FAIL unlisted box-config: want lint-shell-pipes.sh CARGO_HOME"; sed 's/^/       /' "$T/out.box"; fail=1
  fi

  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\ncat ~/.gitconfig >/dev/null\n' >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.gitconfig" 2>&1; then
    echo "FAIL ~/.gitconfig mention without a wrapper call was accepted"; sed 's/^/       /' "$T/out.gitconfig"; fail=1
  elif grep -q 'unlisted box-config' "$T/out.gitconfig" && grep -q 'lint-shell-pipes.sh' "$T/out.gitconfig" && grep -q '~/.gitconfig' "$T/out.gitconfig"; then
    echo "ok   ~/.gitconfig mention without a wrapper call is detected"
  else
    echo "FAIL ~/.gitconfig plant: want lint-shell-pipes.sh ~/.gitconfig"; sed 's/^/       /' "$T/out.gitconfig"; fail=1
  fi

  mkdir -p "$T/in.boxstale/scripts"
  cp "$allow" "$T/in.boxstale/scripts/land-main-refs.tsv"
  printf 'scripts/lint-shell-pipes.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/in.boxstale/scripts/land-main-refs.tsv"
  if chk "$land" "$allow" "$root" "$T/in.boxstale" >"$T/out.boxstale" 2>&1; then
    echo "ok   incoming-only unused BOX_CONFIG_DEPENDENT row is accepted"
  else
    echo "FAIL incoming-only unused BOX_CONFIG_DEPENDENT row should be accepted"; sed 's/^/       /' "$T/out.boxstale"; fail=1
  fi

  mkdir -p "$T/in.boxboth/scripts"
  cp "$allow" "$T/allow.boxboth"
  printf 'scripts/lint-shell-pipes.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/allow.boxboth"
  cp "$T/allow.boxboth" "$T/in.boxboth/scripts/land-main-refs.tsv"
  if chk "$land" "$T/allow.boxboth" "$root" "$T/in.boxboth" >"$T/out.boxboth" 2>&1; then
    echo "FAIL unused BOX_CONFIG_DEPENDENT row in both tables was accepted"; sed 's/^/       /' "$T/out.boxboth"; fail=1
  elif grep -q 'stale allowlist BOX_CONFIG' "$T/out.boxboth" && grep -q 'lint-shell-pipes.sh' "$T/out.boxboth"; then
    echo "ok   unused BOX_CONFIG_DEPENDENT row in both tables fails"
  else
    echo "FAIL unused BOX in both: want lint-shell-pipes.sh"; sed 's/^/       /' "$T/out.boxboth"; fail=1
  fi

  cp "$allow" "$T/allow.boxmainextra"
  printf 'scripts/lint-shell-pipes.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/allow.boxmainextra"
  if chk "$land" "$T/allow.boxmainextra" "$root" >"$T/out.boxmainextra" 2>&1; then
    echo "ok   MAIN table extra BOX_CONFIG_DEPENDENT row dropped from incoming is accepted"
  else
    echo "FAIL MAIN extra BOX / incoming dropped should be accepted"; sed 's/^/       /' "$T/out.boxmainextra"; fail=1
  fi

  mkdir -p "$T/in.boxabsent/scripts"
  cp "$allow" "$T/in.boxabsent/scripts/land-main-refs.tsv"
  printf 'scripts/not-yet-on-main.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/in.boxabsent/scripts/land-main-refs.tsv"
  if chk "$land" "$allow" "$root" "$T/in.boxabsent" >"$T/out.boxabsent" 2>&1; then
    echo "ok   incoming-only BOX_CONFIG_DEPENDENT row for an absent file is accepted"
  else
    echo "FAIL incoming-only absent-file BOX_CONFIG row should be accepted"; sed 's/^/       /' "$T/out.boxabsent"; fail=1
  fi

  mkdir -p "$T/in.boxbothabsent/scripts"
  cp "$allow" "$T/allow.boxbothabsent"
  printf 'scripts/not-yet-on-main.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/allow.boxbothabsent"
  cp "$T/allow.boxbothabsent" "$T/in.boxbothabsent/scripts/land-main-refs.tsv"
  if chk "$land" "$T/allow.boxbothabsent" "$root" "$T/in.boxbothabsent" >"$T/out.boxbothabsent" 2>&1; then
    echo "FAIL unused BOX_CONFIG_DEPENDENT row for an absent file in both tables was accepted"; sed 's/^/       /' "$T/out.boxbothabsent"; fail=1
  elif grep -q 'stale allowlist BOX_CONFIG' "$T/out.boxbothabsent" && grep -q 'not-yet-on-main.sh' "$T/out.boxbothabsent"; then
    echo "ok   unused BOX_CONFIG_DEPENDENT row for an absent file in both tables fails"
  else
    echo "FAIL unused absent-file BOX in both: want not-yet-on-main.sh"; sed 's/^/       /' "$T/out.boxbothabsent"; fail=1
  fi

  # Same file as $allow and incoming (how --self-test's live check calls check).
  cp "$incoming_table" "$T/incoming.copy.tsv"
  hard="$T/incoming.hard.tsv"
  if ! ln "$incoming_table" "$hard" 2>/dev/null; then
    hard="$(mktemp "$(dirname "$incoming_table")/.incoming-hard.XXXXXX")"
    ln "$incoming_table" "$hard"
    trap 'rm -rf "$T"; rm -f "'"$hard"'"' RETURN
  fi
  check "$land" "$main_table" "$root" "$incoming_table" >"$T/out.origrc" 2>&1; orig_rc=$?
  check "$land" "$main_table" "$root" "$T/incoming.copy.tsv" >"$T/out.copyrc" 2>&1; copy_rc=$?
  check "$land" "$main_table" "$root" "$hard" >"$T/out.hardrc" 2>&1; hard_rc=$?
  if [ "$copy_rc" -eq "$orig_rc" ] && [ "$hard_rc" -eq "$orig_rc" ]; then
    echo "ok   copied and hardlinked incoming tables match original rc ($orig_rc)"
  else
    echo "FAIL copy/hardlink rc copy=$copy_rc hard=$hard_rc orig=$orig_rc"; fail=1
    sed 's/^/       /' "$T/out.copyrc"; sed 's/^/       /' "$T/out.hardrc"
  fi

  mkdir -p "$T/dirtymain/scripts"
  git init -q "$T/dirtymain"
  git -C "$T/dirtymain" config user.email plant@test
  git -C "$T/dirtymain" config user.name plant
  cp "$allow" "$T/dirtymain/scripts/land-main-refs.tsv"
  git -C "$T/dirtymain" add scripts/land-main-refs.tsv
  git -C "$T/dirtymain" commit -q -m plant-main-table
  printf 'scripts/not-yet-on-main.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/dirtymain/scripts/land-main-refs.tsv"
  git -C "$T/dirtymain" show HEAD:scripts/land-main-refs.tsv > "$T/from-git.tsv"
  mkdir -p "$T/in.dirtywt/scripts"
  cp "$allow" "$T/in.dirtywt/scripts/land-main-refs.tsv"
  printf 'scripts/not-yet-on-main.sh\tBOX_CONFIG_DEPENDENT\tCARGO_HOME\trequired\n' >> "$T/in.dirtywt/scripts/land-main-refs.tsv"
  if check "$land" "$T/from-git.tsv" "$root" "$T/in.dirtywt/scripts/land-main-refs.tsv" "$T/in.dirtywt" >"$T/out.dirtywt" 2>&1; then
    echo "ok   dirty MAIN working-tree table is ignored (git show HEAD used)"
  else
    echo "FAIL dirty MAIN working tree should be ignored"; sed 's/^/       /' "$T/out.dirtywt"; fail=1
  fi

  miss_rc=0
  chk "$land" "$T/no-such-table.tsv" "$root" >"$T/out.miss" 2>&1 || miss_rc=$?
  if [ "$miss_rc" -eq 2 ]; then
    echo "ok   missing table is rc 2"
  else
    echo "FAIL missing table rc=$miss_rc (want 2)"; sed 's/^/       /' "$T/out.miss"; fail=1
  fi

  mkdir -p "$T/nointable/scripts" "$T/nointable/harness/scripts"
  inmiss_rc=0
  chk "$land" "$allow" "$T/nointable" >"$T/out.inmiss" 2>&1 || inmiss_rc=$?
  if [ "$inmiss_rc" -eq 2 ]; then
    echo "ok   missing incoming table is rc 2"
  else
    echo "FAIL missing incoming table rc=$inmiss_rc (want 2)"; sed 's/^/       /' "$T/out.inmiss"; fail=1
  fi

  die2() { echo "DIE2: $*" >&2; return 2; }
  eval "$(sed -n "/^# >>> main-chokepoint/,/^# <<< main-chokepoint/p" "$land")"
  MAIN="$root"; export MAIN
  eval "$(sed -n "/^log_box_configs()/,/^}/p" "$land")"
  boxlog_rc=0
  log_box_configs >"$T/out.boxlog" 2>&1 || boxlog_rc=$?
  box_ok=1
  if [ "$boxlog_rc" -ne 0 ]; then
    echo "FAIL live log_box_configs rc=$boxlog_rc"; box_ok=0
  fi
  while IFS=$'\t' read -r _p _c dep policy; do
    [ -n "${dep:-}" ] || continue
    grep -q "^box-config: ${dep} " "$T/out.boxlog" || { echo "FAIL box-config log missing $dep"; box_ok=0; }
    if [ "$policy" = required ]; then
      if grep -q "^box-config: ${dep} absent" "$T/out.boxlog"; then
        echo "FAIL required box-config $dep logged as absent"; box_ok=0
      fi
      grep -q "^box-config: ${dep} sha256=" "$T/out.boxlog" || { echo "FAIL required box-config $dep missing sha256="; box_ok=0; }
    fi
  done < <(awk -F'\t' '$2=="BOX_CONFIG_DEPENDENT"{print}' "$allow")
  if [ "$box_ok" -eq 1 ] && [ -s "$T/out.boxlog" ]; then
    echo "ok   land.sh logs box-config for every BOX_CONFIG_DEPENDENT depends_on"
  else
    echo "FAIL land.sh box-config log:"; sed "s/^/       /" "$T/out.boxlog"; fail=1
  fi

  mkdir -p "$T/boxplant/scripts"
  printf 'x\tBOX_CONFIG_DEPENDENT\t/no/such/required.toml\trequired\n' > "$T/boxplant/scripts/land-main-refs.tsv"
  MAIN="$T/boxplant"
  abs_rc=0
  log_box_configs >"$T/out.reqabs" 2>&1 || abs_rc=$?
  MAIN="$root"
  if [ "$abs_rc" -eq 0 ]; then
    echo "FAIL required-absent was accepted"; sed 's/^/       /' "$T/out.reqabs"; fail=1
  elif grep -q 'required box-config /no/such/required.toml is absent' "$T/out.reqabs"; then
    echo "ok   planted required dep absent fails the land"
  else
    echo "FAIL required-absent: rc=$abs_rc"; sed 's/^/       /' "$T/out.reqabs"; fail=1
  fi

  # cargo_generate_lockfile is legal only in the wrapper definition and test-parse5-pin.sh.
  _gen="cargo_"'generate_lockfile'
  cp "$land" "$T/land.sh"
  printf '\n%s\n' "$_gen" >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.genland" 2>&1; then
    echo "FAIL planted $_gen in land.sh was accepted"; sed 's/^/       /' "$T/out.genland"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.genland"; then
    echo "ok   planted ${_gen} in land.sh fails"
  else
    echo "FAIL generate in land.sh: want unlocked-build token"; sed 's/^/       /' "$T/out.genland"; fail=1
  fi

  cp "$root/scripts/lint-shell-pipes.sh" "$T/overlay/scripts/lint-shell-pipes.sh"
  printf '\n%s\n' "$_gen" >> "$T/overlay/scripts/lint-shell-pipes.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.genother" 2>&1; then
    echo "FAIL planted $_gen in lint-shell-pipes.sh was accepted"; sed 's/^/       /' "$T/out.genother"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.genother" && grep -q 'lint-shell-pipes.sh' "$T/out.genother"; then
    echo "ok   planted ${_gen} in lint-shell-pipes.sh fails"
  else
    echo "FAIL generate in other script: want unlocked-build token"; sed 's/^/       /' "$T/out.genother"; fail=1
  fi

  cp "$land" "$T/land.sh"
  printf '\n%s\n' "cargo_"'unlocked' >> "$T/land.sh"
  if chk "$T/land.sh" "$allow" "$root" >"$T/out.star" 2>&1; then
    echo "FAIL planted cargo_"'unlocked'" in land.sh was accepted"; sed 's/^/       /' "$T/out.star"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.star"; then
    echo "ok   planted cargo_* token other than the two allowed names fails"
  else
    echo "FAIL cargo_"'unlocked'": want unlocked-build token"; sed 's/^/       /' "$T/out.star"; fail=1
  fi

  # command cargo test in the checker must not be exempted by substring.
  mkdir -p "$T/overlay/scripts"
  cp "$root/scripts/check-land-main-refs.sh" "$T/overlay/scripts/check-land-main-refs.sh"
  printf '\n%s %s %s\n' 'command' 'c'argo 'test' >> "$T/overlay/scripts/check-land-main-refs.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.ckex" 2>&1; then
    echo "FAIL planted command c"'argo'" test in the checker was accepted"; sed 's/^/       /' "$T/out.ckex"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.ckex" && grep -q 'check-land-main-refs.sh' "$T/out.ckex"; then
    echo "ok   planted command c"'argo'" test in the checker fails"
  else
    echo "FAIL checker unlocked-build plant: want unlocked-build token"; sed 's/^/       /' "$T/out.ckex"; fail=1
  fi

  # Incoming-tree scan: bare token only on the $wt copy of style-parse-errors.sh.
  cp "$root/scripts/style-parse-errors.sh" "$T/overlay/scripts/style-parse-errors.sh"
  printf '\n%s %s\n' 'c'argo 'test' >> "$T/overlay/scripts/style-parse-errors.sh"
  if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.wtcal" 2>&1; then
    echo "FAIL planted bare token in incoming style-parse-errors.sh was accepted"; sed 's/^/       /' "$T/out.wtcal"; fail=1
  elif grep -q 'bare unlocked-build token' "$T/out.wtcal" && grep -q 'style-parse-errors.sh' "$T/out.wtcal"; then
    echo "ok   planted branch-only bare token in style-parse-errors.sh fails"
  else
    echo "FAIL incoming callee plant: want style-parse-errors.sh"; sed 's/^/       /' "$T/out.wtcal"; fail=1
  fi

  # NEXT_REGION_SHA256 accepts a new body for one land; neither pin fails.
  awk '$0=="# <<< main-chokepoint"{print "# planted next-region body"} {print}' "$land" > "$T/land.next.sh"
  nexth="$(awk '$0=="# >>> main-chokepoint"{o++;next} $0=="# <<< main-chokepoint"{c++;next} o==1&&c==0{print}' "$T/land.next.sh" | sha256sum | awk '{print $1}')"
  cp "$allow" "$T/allow.next"
  printf 'scripts/land.sh\tNEXT_REGION_SHA256\t%s\t-\n' "$nexth" >> "$T/allow.next"
  if chk "$T/land.next.sh" "$T/allow.next" "$root" >"$T/out.nextok" 2>&1; then
    echo "ok   edited region body matching NEXT_REGION_SHA256 is accepted"
  else
    echo "FAIL NEXT_REGION_SHA256 should accept the new body"; sed 's/^/       /' "$T/out.nextok"; fail=1
  fi
  if chk "$T/land.next.sh" "$allow" "$root" >"$T/out.nextno" 2>&1; then
    echo "FAIL edited region body with no NEXT pin was accepted"; sed 's/^/       /' "$T/out.nextno"; fail=1
  elif grep -q 'REGION_SHA256 mismatch' "$T/out.nextno"; then
    echo "ok   edited region body matching neither REGION nor NEXT fails"
  else
    echo "FAIL no-NEXT edit: want REGION_SHA256 mismatch"; sed 's/^/       /' "$T/out.nextno"; fail=1
  fi

  # Top-level-only: wrapping the first log_box_configs in `if false` must fail ordering.
  awk 'BEGIN{done=0} !done && $0=="log_box_configs"{print "if false; then"; print "  log_box_configs"; print "fi"; done=1; next} {print}' "$land" > "$T/land.iffalse.sh"
  if chk "$T/land.iffalse.sh" "$allow" "$root" >"$T/out.iffalse" 2>&1; then
    echo "FAIL log_box_configs inside if false was accepted"; sed 's/^/       /' "$T/out.iffalse"; fail=1
  elif grep -q 'log_box_configs before the first' "$T/out.iffalse"; then
    echo "ok   planted log_box_configs inside if false fails ordering"
  else
    echo "FAIL if-false log: want ordering error"; sed 's/^/       /' "$T/out.iffalse"; fail=1
  fi

  awk 'BEGIN{done=0} !done && $0=="log_box_configs"{print "f() {"; print "log_box_configs"; print "}"; done=1; next} {print}' "$land" > "$T/land.fnlog.sh"
  if chk "$T/land.fnlog.sh" "$allow" "$root" >"$T/out.fnlog" 2>&1; then
    echo "FAIL log_box_configs inside a function body was accepted"; sed 's/^/       /' "$T/out.fnlog"; fail=1
  elif grep -q 'log_box_configs before the first' "$T/out.fnlog"; then
    echo "ok   planted log_box_configs inside a function body fails ordering"
  else
    echo "FAIL function-body log: want ordering error"; sed 's/^/       /' "$T/out.fnlog"; fail=1
  fi

  # Each bash definition form, plus aliases, in land.sh and style-parse-errors.sh.
  _nm=cargo_locked
  _forms=()
  _forms+=("$(printf '%s()' "$_nm")")
  _forms+=("$(printf '%s ()' "$_nm")")
  _forms+=("$(printf 'function %s' "$_nm")")
  _forms+=("$(printf 'function %s()' "$_nm")")
  _forms+=("$(printf 'function %s ()' "$_nm")")
  _forms+=("$(printf '%s() { :; }' "$_nm")")
  _forms+=("$(printf '%s () { :; }' "$_nm")")
  _forms+=("$(printf 'function %s { :; }' "$_nm")")
  _forms+=("$(printf 'function %s() { :; }' "$_nm")")
  _forms+=("$(printf 'function %s () { :; }' "$_nm")")
  _forms+=("$(printf 'function %s { %s "$@"; }' "$_nm" _cargo_exec)")
  _forms+=("$(printf 'alias %s=%s' "$_nm" true)")
  _forms+=("$(printf 'alias %s=%s' c'argo' true)")
  _forms+=("$(printf '%s()' _cargo_exec)")
  _forms+=("$(printf 'function %s { :; }' _cargo_exec)")
  mkdir -p "$T/overlay/scripts"
  for _text in "${_forms[@]}"; do
    cp "$land" "$T/land.sh"
    printf '\n%s\n' "$_text" >> "$T/land.sh"
    if chk "$T/land.sh" "$allow" "$root" >"$T/out.defland" 2>&1; then
      echo "FAIL planted '${_text}' in land.sh was accepted"; sed 's/^/       /' "$T/out.defland"; fail=1
    elif grep -q 'bare unlocked-build token' "$T/out.defland"; then
      echo "ok   planted '${_text}' in land.sh fails"
    else
      echo "FAIL planted '${_text}' in land.sh: want unlocked-build token"; sed 's/^/       /' "$T/out.defland"; fail=1
    fi
    cp "$root/scripts/style-parse-errors.sh" "$T/overlay/scripts/style-parse-errors.sh"
    printf '\n%s\n' "$_text" >> "$T/overlay/scripts/style-parse-errors.sh"
    if chk "$land" "$allow" "$root" "$T/overlay" >"$T/out.defsty" 2>&1; then
      echo "FAIL planted '${_text}' in style-parse-errors.sh was accepted"; sed 's/^/       /' "$T/out.defsty"; fail=1
    elif grep -q 'bare unlocked-build token' "$T/out.defsty" && grep -q 'style-parse-errors.sh' "$T/out.defsty"; then
      echo "ok   planted '${_text}' in style-parse-errors.sh fails"
    else
      echo "FAIL planted '${_text}' in style-parse-errors.sh: want unlocked-build token"; sed 's/^/       /' "$T/out.defsty"; fail=1
    fi
  done

  # Unpromoted step 2: incoming table NEXT equals this tree's body.
  awk '$0=="# <<< main-chokepoint"{print "# planted unpromoted next body"} {print}' "$land" > "$T/land.unprom.sh"
  unpromh="$(awk '$0=="# >>> main-chokepoint"{o++;next} $0=="# <<< main-chokepoint"{c++;next} o==1&&c==0{print}' "$T/land.unprom.sh" | sha256sum | awk '{print $1}')"
  cp "$allow" "$T/allow.unprom"
  printf 'scripts/land.sh\tNEXT_REGION_SHA256\t%s\t-\n' "$unpromh" >> "$T/allow.unprom"
  mkdir -p "$T/ovnext/scripts"
  cp "$T/allow.unprom" "$T/ovnext/scripts/land-main-refs.tsv"
  if chk "$T/land.unprom.sh" "$T/allow.unprom" "$root" "$T/ovnext" >"$T/out.unprom" 2>&1; then
    echo "FAIL incoming NEXT equal to this tree's body was accepted"; sed 's/^/       /' "$T/out.unprom"; fail=1
  elif grep -q 'unpromoted step 2' "$T/out.unprom"; then
    echo "ok   planted incoming NEXT_REGION_SHA256 equal to the body fails (unpromoted step 2)"
  else
    echo "FAIL unpromoted NEXT: want unpromoted step 2"; sed 's/^/       /' "$T/out.unprom"; fail=1
  fi

  # Step 1: NEXT names a future body and differs from the current body.
  mkdir -p "$T/ovstep1/scripts"
  cp "$allow" "$T/allow.step1"
  printf 'scripts/land.sh\tNEXT_REGION_SHA256\t%s\t-\n' "$unpromh" >> "$T/allow.step1"
  cp "$T/allow.step1" "$T/ovstep1/scripts/land-main-refs.tsv"
  if chk "$land" "$T/allow.step1" "$root" "$T/ovstep1" >"$T/out.step1" 2>&1; then
    echo "ok   planted NEXT_REGION_SHA256 different from the current body (step 1) is accepted"
  else
    echo "FAIL step-1 NEXT (different from body) should pass"; sed 's/^/       /' "$T/out.step1"; fail=1
  fi

  return "$fail"
}

MODE=""
LAND="" ROOT="" OVERLAY="" MAIN_TABLE="" INCOMING_TABLE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --self-test) MODE=self_test; shift ;;
    --check) MODE=check; shift ;;
    --main-table)
      [ $# -ge 2 ] || { echo "usage: --main-table FILE" >&2; exit 2; }
      MAIN_TABLE="$2"; shift 2 ;;
    --incoming-table)
      [ $# -ge 2 ] || { echo "usage: --incoming-table FILE" >&2; exit 2; }
      INCOMING_TABLE="$2"; shift 2 ;;
    --overlay)
      [ $# -ge 2 ] || { echo "usage: --overlay DIR" >&2; exit 2; }
      OVERLAY="$2"; shift 2 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *)
      if [ "$MODE" = check ] && [ -z "$LAND" ]; then
        LAND="$1"
      elif [ -z "$ROOT" ]; then
        ROOT="$1"
      elif [ "$MODE" = check ] && [ -z "$OVERLAY" ]; then
        OVERLAY="$1"
      else
        echo "unexpected argument: $1" >&2; exit 2
      fi
      shift ;;
  esac
done
[ -n "$ROOT" ] || ROOT="$(cd "$SELF/.." && pwd)"

case "$MODE" in
  self_test)
    [ -n "$MAIN_TABLE" ] && [ -n "$INCOMING_TABLE" ] || { echo "usage: --self-test <root> --main-table FILE --incoming-table FILE" >&2; exit 2; }
    self_test "$ROOT" "$MAIN_TABLE" "$INCOMING_TABLE"
    ;;
  check)
    [ -n "$LAND" ] && [ -n "$ROOT" ] && [ -n "$MAIN_TABLE" ] && [ -n "$INCOMING_TABLE" ] \
      || { echo "usage: --check <land.sh> <incoming-tree> --main-table FILE --incoming-table FILE" >&2; exit 2; }
    check "$LAND" "$MAIN_TABLE" "$ROOT" "$INCOMING_TABLE" "$OVERLAY"
    ;;
  *)
    echo "usage: --check <land.sh> <incoming-tree> --main-table FILE --incoming-table FILE" >&2
    exit 2
    ;;
esac
