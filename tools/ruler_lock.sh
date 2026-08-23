#!/usr/bin/env bash
# The RULER LOCK — see docs/network-first/README.md rule 3 and
# docs/sandhills/README.md §4.
#
# The measurement stack is shared, not copied: the purity rule in
# docs/calibration/metric-battery.md requires ONE implementation, because the
# same function must measure a 3DEP tile and generator output. Two copies would
# drift, and the drift would be indistinguishable from a generator improvement.
#
# TWO TIERS (attempt 5). Growing the corpus legitimately edits the sweep
# drivers' biome lists and the cull lists; it must NEVER edit a measurement
# kernel. One flat lock could not tell those apart, so a corpus expansion and a
# silent change to `anisotropy()` re-baselined identically. Now:
#
#   KERNELS  pure measurement + thresholds. Frozen for the whole build.
#            Re-baselining these requires `write --kernels`, on purpose.
#   SWEEPS   corpus-sweep drivers (which biomes to iterate) and the cull
#            lists. Re-baselined by `write` when the corpus grows.
#
#   tools/ruler_lock.sh verify           fail if anything drifted or is uncovered
#   tools/ruler_lock.sh write            re-baseline SWEEPS (kernels must be clean)
#   tools/ruler_lock.sh write --kernels  re-baseline everything (deliberate)
set -euo pipefail
cd "$(dirname "$0")/.."
LOCK=docs/ruler.lock

# --- KERNELS: what the ruler MEASURES. Frozen during a build. ---------------
KERNELS=(
  tools/metrics/metrics/core.py
  tools/metrics/metrics/features.py
  tools/metrics/metrics/io.py
  tools/metrics/metrics/config.yaml
  tools/metrics/METRICS.md
  tools/macro_campaign/macro_campaign/flow.py
  tools/macro_campaign/macro_campaign/structure.py
  tools/macro_campaign/macro_campaign/netstats.py
  tools/macro_campaign/macro_campaign/cgrid.py
  tools/golf_proxy/proxy.py
  tools/golf_proxy/proxy_thresholds.json
  tools/dtm_primitives/dtm_primitives/geomorphons.py
  tools/dtm_primitives/dtm_primitives/ridgepipe.py
  tools/dtm_primitives/dtm_primitives/blufffit.py
  tools/dtm_primitives/dtm_primitives/bowlfit.py
  tools/dtm_primitives/dtm_primitives/transects.py
  tools/dtm_primitives/dtm_primitives/profiles.py
)

# --- SWEEPS: what the ruler is POINTED AT. Moves when the corpus grows. -----
# These also carry policy constants (Policy B's threshold, near_par's grid), so
# a change here is still a real event -- just an expected one during a campaign.
SWEEPS=(
  tools/macro_campaign/horton_real.py
  tools/macro_campaign/horton_policy_compare.py
  tools/macro_campaign/pattern_survey.py
  tools/macro_campaign/trunk_probe.py
  tools/macro_campaign/valley_profile.py
  tools/macro_campaign/zone_stats.py
  tools/macro_campaign/junction_real.py
  tools/macro_campaign/real_planform.py
  tools/golf_proxy/proxy_corpus.py
  tools/macro_campaign/out/exclude.json
  tools/macro_campaign/out/review_v2.json
)

FILES=( "${KERNELS[@]}" "${SWEEPS[@]}" )

case "${1:-verify}" in
  write)
    if [ "${2:-}" != "--kernels" ] && [ -f "$LOCK" ]; then
      drift=0
      for f in "${KERNELS[@]}"; do
        want=$(grep "  $f\$" "$LOCK" | awk '{print $1}' || true)
        [ -n "$want" ] || continue
        got=$(shasum -a 256 "$f" | awk '{print $1}')
        [ "$want" = "$got" ] || { echo "KERNEL CHANGED: $f"; drift=1; }
      done
      if [ "$drift" -ne 0 ]; then
        echo
        echo "A measurement kernel moved. Changing the generator and the ruler"
        echo "together makes every result uninterpretable. If this is genuinely"
        echo "intended, re-baseline it explicitly:"
        echo "    tools/ruler_lock.sh write --kernels"
        exit 1
      fi
    fi
    { echo "# RULER LOCK — regenerate only by deliberate decision."
      echo "# $(git rev-parse --short HEAD) on $(git rev-parse --abbrev-ref HEAD)"
      echo "# ${#KERNELS[@]} kernels (frozen) + ${#SWEEPS[@]} sweep/cull files"
      for f in "${FILES[@]}"; do shasum -a 256 "$f"; done
    } > "$LOCK"
    echo "wrote $LOCK (${#KERNELS[@]} kernels + ${#SWEEPS[@]} sweeps)"
    ;;
  verify)
    [ -f "$LOCK" ] || { echo "no lock file at $LOCK — run 'tools/ruler_lock.sh write'"; exit 1; }
    # COVERAGE first. Digest-checking alone passes when a file is added to
    # FILES and never locked, or dropped from the lock -- i.e. the lock could
    # silently stop covering part of the ruler. Found by negative-testing on
    # 2026-08-22, when six admitted dtm_primitives files verified OK while
    # entirely absent from the lock.
    missing=0
    for f in "${FILES[@]}"; do
      grep -q "  $f\$" "$LOCK" || { echo "NOT IN LOCK: $f"; missing=1; }
    done
    while read -r _ f; do
      [ -n "$f" ] || continue
      printf '%s\n' "${FILES[@]}" | grep -qx "$f" || { echo "LOCKED BUT NOT IN FILES: $f"; missing=1; }
    done < <(grep -v '^#' "$LOCK")
    if [ "$missing" -ne 0 ]; then
      echo "RULER LOCK INCOMPLETE — re-baseline deliberately: tools/ruler_lock.sh write"
      exit 1
    fi
    if grep -v '^#' "$LOCK" | shasum -a 256 -c --status; then
      echo "ruler OK — ${#KERNELS[@]} kernels frozen, ${#SWEEPS[@]} sweep/cull files unchanged"
    else
      echo "RULER DRIFTED:"; grep -v '^#' "$LOCK" | shasum -a 256 -c 2>&1 | grep -v ': OK$'
      exit 1
    fi
    ;;
  *) echo "usage: $0 {write|verify}"; exit 2;;
esac
