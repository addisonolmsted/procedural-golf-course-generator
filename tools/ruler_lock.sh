#!/usr/bin/env bash
# The RULER LOCK — see docs/network-first/README.md rule 3.
#
# The measurement stack is shared, not copied: the purity rule in
# docs/calibration/metric-battery.md requires ONE implementation, because the
# same function must measure a 3DEP tile and generator output. Two copies would
# drift, and the drift would be indistinguishable from a generator improvement.
#
# So instead of copying it with provenance comments, we digest-lock it. Any
# edit to a locked file during the build is a loud failure, not a silent
# re-baseline.
#
#   tools/ruler_lock.sh write    regenerate the lock (a deliberate act)
#   tools/ruler_lock.sh verify   fail if anything drifted
set -euo pipefail
cd "$(dirname "$0")/.."
LOCK=docs/network-first/ruler.lock

FILES=(
  tools/metrics/metrics/core.py
  tools/metrics/metrics/features.py
  tools/metrics/metrics/io.py
  tools/metrics/metrics/config.yaml
  tools/metrics/METRICS.md
  tools/macro_campaign/macro_campaign/flow.py
  tools/macro_campaign/macro_campaign/structure.py
  tools/macro_campaign/macro_campaign/netstats.py
  tools/macro_campaign/macro_campaign/cgrid.py
  tools/macro_campaign/horton_real.py
  tools/macro_campaign/horton_policy_compare.py
  tools/macro_campaign/junction_real.py
  tools/macro_campaign/real_planform.py
  tools/golf_proxy/proxy.py
  tools/golf_proxy/proxy_thresholds.json
  tools/macro_campaign/out/exclude.json
  tools/macro_campaign/out/review_v2.json
)

case "${1:-verify}" in
  write)
    { echo "# RULER LOCK — regenerate only by deliberate decision."
      echo "# $(git rev-parse --short HEAD) on $(git rev-parse --abbrev-ref HEAD)"
      for f in "${FILES[@]}"; do shasum -a 256 "$f"; done
    } > "$LOCK"
    echo "wrote $LOCK ($(grep -c . "$LOCK") lines)"
    ;;
  verify)
    [ -f "$LOCK" ] || { echo "no lock file at $LOCK — run 'tools/ruler_lock.sh write'"; exit 1; }
    if grep -v '^#' "$LOCK" | shasum -a 256 -c --status; then
      echo "ruler OK — $(grep -vc '^#' "$LOCK") files unchanged"
    else
      echo "RULER DRIFTED:"; grep -v '^#' "$LOCK" | shasum -a 256 -c 2>&1 | grep -v ': OK$'
      exit 1
    fi
    ;;
  *) echo "usage: $0 {write|verify}"; exit 2;;
esac
