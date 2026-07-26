# Step 07 — Routability Gate

**Status:** unclaimed

## Purpose

Decide — cheaply, deterministically, and WITHOUT modifying terrain — whether
the generated core can plausibly host 9 holes. Passing bundles proceed to
routing; failing ones trigger a deterministic reroll (step 01's reroll rule).
The gate is the pipeline's quality throttle: its metrics are also the
batch-harness health dashboard.

## Position

- Upstream: step 05 (`HydroBundle`), step 06 (`CoverMap`), step 02
  (`params.gate` thresholds).
- Downstream: step 08 runs only on gate-passed bundles; the orchestrator
  consumes pass/fail for the reroll loop.

## Contract

Input: `HydroBundle`, `CoverMap`, `CourseSpec`. No randomness. No mutation.

Output — `GateReport`:

```
GateReport {
  pass: bool,
  metrics: { name → f64 },   // published names below — keep stable
  reasons: [string],         // non-empty iff !pass; each names the failed check
}
```

Published metrics (all evaluated on the routable-core window only):

- `core_slope_lt8_frac` — fraction of core cells with slope < 8%.
- `core_routable_patch_ha` — largest connected component of
  `routable(class) ∧ slope < 15%`.
- `core_water_blockage_frac` — water + wetland fraction.
- `core_stream_crossings` — crossing pressure from streams intersecting the
  core.
- `corridor_feasibility` — the real check: greedy placement of 9 corridors
  (par-36 lengths ≈ 2×470 m, 5×360 m, 2×155 m; ~55 m wide) into the routable
  patch graph, counting forced water crossings. Area-proxy is not enough —
  a big-but-shredded patch must fail.

Thresholds come from `params.gate` (per archetype: mountain accepts a lower
slope fraction but the corridor check stays strict). Reroll protocol: bounded
attempts (suggest 8); attrition (attempt count + failing metrics) is reported
upward — a systematically failing archetype is a step-03 calibration bug, and
the gate's job is to make that visible, not to hide it.

## Per-archetype behavior

Only via `params.gate` thresholds. Expected pass-rate targets (P4): ≥ ~70%
per archetype; mountain may run lower — measured, not assumed.

## Hard requirements

1. Pure function: no mutation of any input artifact, no RNG.
2. `reasons` exactly explains failure (each reason maps to one metric +
   threshold).
3. Metric names are a published contract — renaming is a version bump.
4. Fast: target ≪ 1 s at canonical resolution (it runs on every reroll).
5. Deterministic double-run.

## References

- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_gate` —
  metrics 1–4 implemented (area proxy in place of corridor placement).
- `main:golf-routing/src/fields.rs` — site-scoring ideas useful for the
  corridor-feasibility check.

## Open questions

- Corridor feasibility algorithm: greedy chord placement vs a coarse
  corridor-graph search — pick the cheapest one that correctly rejects
  shredded patches.
- Should the gate emit hints (e.g. best clubhouse candidate regions) that
  step 08 may use, or stay strictly judgmental?
