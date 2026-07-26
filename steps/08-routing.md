# Step 08 — Routing

**Status:** unclaimed

## Purpose

Lay out 9 holes (par 36) on the gate-passed terrain: tees, doglegs, greens,
corridors, and the green→next-tee walk, as a returning nine anchored to a
clubhouse site. Routing reads terrain/water/cover as constraints and
opportunities (a creek is a hazard to play over, not just an obstacle).

## Position

- Upstream: steps 05/06 (terrain, water, cover), step 02 (`params.route`),
  step 07 (must have passed).
- Downstream: step 09 builds what routing placed; step 10 validates it.

## Contract

Input: `HydroBundle`, `CoverMap`, `CourseSpec` (gate-passed). Randomness:
`route/v1`.

Output — `RoutingPlan`:

```
RoutingPlan {
  clubhouse: [x,y],
  holes: [ {                       // exactly 9, in play order
    par: 3|4|5,                    // sequence = params.route.par_sequence
    tee: [x,y],
    dogleg: [x,y] | null,          // turn point for par 4/5
    green: [x,y],
    corridor: [[x,y],…],           // closed outline, CCW
  } ],
}
```

Hole length conventions (from tee via dogleg to green): par 3 ≈ 120–210 m,
par 4 ≈ 300–430 m, par 5 ≈ 440–530 m.

Suggested shape (reference implementation exists): candidate-site scoring
(green/tee suitability fields) → sequence construction with backtracking
(returning nine, walkability) → simulated-annealing polish (corridor overlap,
crossing count, variety).

## Per-archetype behavior

None explicit — terrain/water/cover already encode it. Emergent expectations
to eyeball in P3: florida routes AROUND lakes (crossings as risk-reward),
piedmont crosses the creek ≤ `crossings_max` times, mountain follows benches,
sandhills flows freely between dunes.

## Hard requirements

1. Every corridor entirely inside the routable core; corridors respect
   `routable(class)` + slope except at declared crossings.
2. Crossing count ≤ `params.gate.crossings_max`; greens and tees never in
   water/wetland; green sites within a stated max slope before earthworks.
3. Returning nine: hole 1 tee and hole 9 green within a stated radius of the
   clubhouse; green→next-tee walks below a stated max distance.
4. Par sequence exactly `params.route.par_sequence`; hole-length conventions
   respected.
5. Corridors of distinct holes overlap at most at declared shared margins.
6. Deterministic double-run.

## References

- `main:golf-routing/` — the complete v1 9-hole router: `fields.rs` (site
  scoring), `sequence.rs`, `construct.rs` (returning-nine construction +
  backtracking), `anneal.rs`, `walk.rs` (A* walks). Algorithm reference —
  types are v1's.
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_routing` —
  the geometric stand-in downstream steps use today.

## Open questions

- Doglegs: explicit turn points (current contract) vs corridor centerline
  polylines (more general, heavier) — decide before building step 09's
  fairway shaping.
- Should routing be able to REQUEST a reroll (terrain passed the gate but
  routing can't close a nine), or must the gate be strengthened until this
  can't happen? Draft: routing may fail → counts as a gate-style reroll,
  reported distinctly.
