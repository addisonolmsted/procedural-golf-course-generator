# Stage 09 — Routing

**Status:** unclaimed

## Purpose

Lay out the 9-hole, par-36 returning loop on the gated terrain: beam search
over the clubhouse-anchored hole sequence, over-provisioned green candidate
sites, dispersion reduction, annealing refinement. The Stage-02 earthmoving
allowance enters as a SOFT cost — corridors that need dirt are penalized,
not forbidden.

## Position

- Upstream: Stages 05/06/07 (terrain, water, cover), Stage 02 (mask,
  clubhouse anchor, allowance), Stage 00 (`route.*` knobs + the typed
  constants `hole_count = 9`, `par_sequence` summing 36).
- Downstream: Stage 10 builds what routing placed; Stages 11/13 read the
  plan.
- Streams: `route/v1` — attempt scope.

## Contract

Input: gated artifact set.

Output — `routing.json`:

```
RoutingPlan {
  clubhouse: [x,y],
  holes: [ { par, tee: [x,y], dogleg: [x,y] | null, green: [x,y],
             corridor: [[x,y],…],              // closed outline
             earthwork_need_m3: f64 } ],       // the soft-cost bill, per hole
  walk_paths: [ [[x,y],…] ],                   // green -> next tee
  totals: { length_m, earthwork_need_m3, crossings },
}
```

Machinery (the design carried over from the v1 router — port, not
reinvention): site-scoring LUTs over local terrain windows (green/tee/
landing suitability), pairwise terms (walkability, corridor overlap,
crossing costs), sequence-global terms (returning nine, par mix order,
direction dispersion), beam search over the loop construction with
over-provisioned green candidates, then simulated-annealing refinement.
Earthwork need per corridor estimated from cut/fill to a playable envelope —
weighted by `route.earthwork_cost_w` against the Stage-02 allowance.

## Hard requirements

1. All corridors within the mask except declared crossing segments; total
   crossings ≤ θ cap; greens/tees never in water/wetland.
2. Par sequence exactly the contract constant; hole lengths within the par
   conventions (par 3 ≈ 120–210 m, 4 ≈ 300–430 m, 5 ≈ 440–530 m tee→green
   via dogleg).
3. Returning nine: hole 1 tee + hole 9 green within θ radius of the
   clubhouse; walk distances ≤ θ cap.
4. Σ earthwork_need over the plan ≤ Stage-02 allowance × θ headroom (soft
   budget respected in aggregate).
5. Deterministic (beam + anneal seeded from `route/v1` only); double-run.
6. May fail (can't close a loop): returns a typed failure that the
   orchestrator treats like a gate fail (attempt++), reported distinctly.

## References

- `main:golf-routing/` — the complete prior implementation: `fields.rs`
  (site scoring), `sequence.rs`, `construct.rs` (returning-nine beam +
  backtracking), `anneal.rs`, `walk.rs`. Port onto the new artifact types.

## Open questions

- Dogleg representation: single turn point vs centerline polyline — decide
  with Stage 10 (its spine model consumes this).
- Are walk paths A*-routed here (v1 did) or deferred to Stage 13 cart
  paths? Draft: rough walk paths here (sequence cost needs them), final
  path geometry in Stage 13.
