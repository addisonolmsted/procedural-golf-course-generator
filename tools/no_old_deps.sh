#!/usr/bin/env bash
# Discipline rule 1: a restart's crates may not depend on the machinery the
# restart exists to replace. The previous restarts failed by reusing machinery
# silently; this makes the rule mechanical instead of aspirational.
#
#   attempt 4 (docs/network-first/README.md): may not use the retired v2 crates.
#   attempt 5 (docs/sandhills/README.md):     may use course-world + course-seed
#                                             ONLY -- attempt 4's generator
#                                             crates are forbidden too.
set -euo pipefail
cd "$(dirname "$0")/.."

RETIRED='course-skeleton|course-primitives|course-amplify|course-transforms|course-framing|course-spec'
ATTEMPT4='course-draw|course-template|course-network|course-relief'

N4=(course-draw course-template course-network course-relief course-fabric course-water course-lab)
N5=(course-sandhills)

fail=0

# $1 = crate name, $2 = forbidden regex (cargo form, e.g. course-relief)
check() {
  local c="$1" forbidden="$2"
  local m="crates/$c/Cargo.toml"
  [ -f "$m" ] || { echo "missing $m"; fail=1; return; }
  if hits=$(grep -nE "$forbidden" "$m"); then
    echo "FORBIDDEN DEPENDENCY in $m:"; echo "$hits"; fail=1
  fi
  # use-statement form: course-relief -> course_relief
  local uses="${forbidden//-/_}"
  if hits=$(grep -rnE "use +($uses)" "crates/$c/src" 2>/dev/null); then
    echo "FORBIDDEN IMPORT in crates/$c/src:"; echo "$hits"; fail=1
  fi
}

for c in "${N4[@]}"; do check "$c" "$RETIRED"; done
for c in "${N5[@]}"; do check "$c" "$RETIRED|$ATTEMPT4"; done

if [ "$fail" -eq 0 ]; then
  echo "no-old-deps OK — ${#N4[@]} attempt-4 crates, ${#N5[@]} attempt-5 crate(s) clean"
else
  echo
  echo "Anything wanted must be ADDED TO THE ALLOWLIST first --"
  echo "  attempt 4: docs/network-first/README.md"
  echo "  attempt 5: docs/sandhills/README.md §3"
  echo "then COPIED with a provenance comment naming its source commit."
  exit 1
fi
