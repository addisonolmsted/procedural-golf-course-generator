#!/usr/bin/env bash
# Discipline rule 1 (docs/network-first/README.md): the attempt-4 crates may not
# depend on the retired pipeline crates. The previous restart failed by reusing
# machinery silently; this makes the rule mechanical instead of aspirational.
set -euo pipefail
cd "$(dirname "$0")/.."

NEW=(course-draw course-template course-network course-relief course-fabric course-water)
FORBIDDEN='course-skeleton|course-primitives|course-amplify|course-transforms|course-framing|course-spec'

fail=0
for c in "${NEW[@]}"; do
  m="crates/$c/Cargo.toml"
  [ -f "$m" ] || { echo "missing $m"; fail=1; continue; }
  if hits=$(grep -nE "$FORBIDDEN" "$m"); then
    echo "FORBIDDEN DEPENDENCY in $m:"; echo "$hits"; fail=1
  fi
  if hits=$(grep -rnE "use +(course_skeleton|course_primitives|course_amplify|course_transforms|course_framing|course_spec)" "crates/$c/src" 2>/dev/null); then
    echo "FORBIDDEN IMPORT in crates/$c/src:"; echo "$hits"; fail=1
  fi
done
if [ "$fail" -eq 0 ]; then
  echo "no-old-deps OK — ${#NEW[@]} attempt-4 crates clean"
else
  echo; echo "Anything wanted must be ADDED TO THE ALLOWLIST in"
  echo "docs/network-first/README.md first, then COPIED with a provenance comment."
  exit 1
fi
