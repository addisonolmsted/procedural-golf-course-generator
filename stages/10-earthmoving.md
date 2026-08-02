# Stage 10 — Hole Earthmoving & Contouring

**Status:** unclaimed

## Purpose

Per-hole construction on the shared base: displacement layers that turn
routed corridors into finished holes — fairway contouring from spine + width
profiles, cross-section libraries, green construction under functional
constraints, choke points opened within budget. The design follows the
plan-space / surface-space split: decisions made in the hole's own
coordinate frame (along-spine s, across-spine t), applied as displacement
fields on the world grid.

## Position

- Upstream: Stage 09 (`RoutingPlan`), Stages 05/06/07 (base terrain, water,
  cover/substrate), Stage 02 (allowance = the budget), Stage 00
  (`earthworks.*` knobs).
- Downstream: Stage 11 validates drainage and iterates with this stage;
  Stage 12 blends seams; Stage 13 places furniture on the built surfaces.
- Streams: `earthworks/v1` — attempt scope (bunker shapes, contour jitter).

## Contract

Input: routing plan + terrain artifact set.

Output — `earthworks/` artifact:

```
layers: per hole, ordered displacement patches at RES_EARTHWORKS_M (0.5 m):
  hole<N>.<pass>.cgrid            // f32 delta_m, edges feather to exactly 0
earthworks.json {
  passes: [ … the executed pass list per hole … ],
  features: [ { kind: tee_pad | green | bunker | swale | choke_cut,
                hole, center, params } ],
  budget: { allowance_m3, spent_m3, per_hole: [...] },
}
final composition: height(p) = base(p) + Σ layers covering p   // THE rule
```

**The ten-pass plan-space/surface-space split is the user's prior design and
must be imported into this doc before build** (open question #1). Contract
placeholder for the pass pipeline shape:
plan-space passes (spine fairing, width profile solve, cross-section
selection, green pad + surrounds solve, bunker set placement) → surface-space
passes (displacement rasterization, tie-in feathering, choke-point cuts,
drainage pre-grading, detail contouring). Ten passes total in the source
design.

Constraints: green surfaces within playable slope bounds (θ), tee pads level
within tolerance, budget: Σ|cut|+|fill| ≤ allowance (choke cuts included),
never raise ground across a Stage-06 creek or into a lake below surface
elev, cross-sections respect substrate (rock cuts cost more against the
budget).

## Hard requirements

1. Feather-to-zero on every layer boundary; composed surface has no
   discontinuity above θ at any patch edge (tested at 0.5 m and 2 m).
2. Budget respected; spent_m3 reported per hole and total.
3. Green/tee functional constraints met (slope bounds, pinnable-area
   fraction).
4. No new water violations (creek/lake rules above) — Stage 11 will catch,
   this stage must not knowingly create.
5. Deterministic; attempt-scoped; double-run.

## References

- `main:golf-holes/` — v1 build-out (`fairway.rs`, `green.rs`, `tee.rs`,
  `bunker.rs`, `water_fit.rs`) — behavior prior art.
- `crates/course-world/src/{profile.rs,spline.rs}` — `Profile` and `Spine`
  are exactly the plan-space substrate this stage needs.

## Open questions

1. **Import the ten-pass design** — the authoritative pass list and the
   plan-space/surface-space split live with the user; transcribe into this
   doc before claiming the stage.
2. Cross-section library: authored set per archetype (data files) vs
   parametric family (θ)? Draft: parametric with archetype θ, library later.
3. Bunker count/style per archetype — art-kit coupling; coordinate with
   Stage 13.
