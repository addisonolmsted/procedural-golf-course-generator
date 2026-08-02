# Stage 08 — Routability Gate

**Status:** unclaimed

## Purpose

The cheap check that the authored guarantee survived the physics. With the
mask upstream this should reject RARELY — a high fail rate is a Stage 02/04
calibration bug, and the gate's job is to make that visible, not to filter
it away. On failure, resample stages 2–3 (attempt++), never the θ/framing
upstream and never a Stage-05 re-tune.

## Position

- Upstream: Stage 05 (`terrain/`), Stage 06 (`hydro/`), Stage 07 (`cover/`),
  Stage 02 (mask + invariant thresholds), Stage 00 (`gate.*` targets).
- Downstream: Stage 09 runs only on pass; the orchestrator consumes
  pass/fail for the attempt loop (bounded by `MAX_ATTEMPTS`).
- Streams: none — RNG-free by contract.

## Contract

Input: all upstream artifacts. Pure analysis, no mutation.

Output — `gate.json`:

```
GateReport {
  pass: bool,
  metrics: {                       // published names — keep stable
    mask_slope_hist: [...],        // slope histogram inside the mask
    mask_slope_le8_frac, mask_slope_le15_frac,
    buildable_patch_ha,            // largest connected buildable component
    corridor_transects_ok_frac,    // width transects through the mask ≥ w_min
    offcorridor_channel_crossings, // channels > a_chan crossing off-slot
    water_blockage_frac,
    connectivity_ok: bool,         // mask still one component on the surface
  },
  reasons: [string],               // non-empty iff !pass, one per failed check
}
```

Checks are exactly the Stage-02 invariant plus archetype targets from
`gate.*`: slope histogram bounds, largest buildable component ≥ target,
corridor transects (sample lines across every mask corridor; each must have
≥ w_min of contiguous buildable width), off-corridor channel crossings = 0,
water blockage ≤ cap, connectivity preserved.

Reroll protocol: fail ⇒ `RunIdentity::reroll()` (attempt++), which redraws
ONLY attempt-scoped streams (mask, strokes, forcing noise, cover clumps);
archetype, θ, framing are stable-scoped and survive. Bounded attempts;
attrition (attempt count + failing metrics) reported upward.

## Hard requirements

1. Pure + deterministic; `reasons` maps 1:1 to failed metrics.
2. Fast: ≤ ~200 ms at sim res (it runs per attempt).
3. Metric names are a published contract (rename = version bump).
4. Batch harness hook: a seed-sweep binary reporting pass rate per archetype
   (target ≥ 90% with the mask upstream; measured, recorded in PLAN.md).

## References

- The Stage-02 invariant definition (single source of truth for thresholds).
- Retired gate prior art: `archetype-pipeline:course-contracts` gate fixture
  (metrics/reasons shape).

## Open questions

- Corridor transect sampling density — enough to catch a pinch without
  costing the 200 ms budget.
- Should the gate also sanity-check plausibility (drainage density within
  calibration band) or is that strictly the campaign's compare loop? Draft:
  campaign only — the gate guards routability, not realism.
