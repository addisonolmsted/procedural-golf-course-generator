# Contract C3 — Routing → Realization

**Between:** [S6 routing](../stages/stage-06-routing.md) →
[S7 earthmoving](../stages/stage-07-earthmoving.md),
[S8 hole layout](../stages/stage-08-hole-layout.md)
**Type:** `course_contracts::contracts::corridor_graph::CorridorGraph`
**Status:** guarded. Golden-seed tested; changes bump `C3_VERSION` and
`PIPELINE_VERSION`.

## Why this contract exists

C3 is where **the course stops being a search result and becomes a
construction plan**. S6 explored a space and picked an answer; S7 and S8 build
it. Everything about *how* the answer was found — beam states, scores, rejected
alternatives — dies at this boundary.

C3 is also the pipeline's **streaming seam**
([../02-performance-budget.md](../02-performance-budget.md)). S0–S6 are
whole-course and blocking; S7–S10 are per-hole and stream. C3 is therefore
structured so that a single `Hole` entry is independently realizable: hole 1 can
be built and played while hole 6 has not been looked at.

## Shape

```
CorridorGraph {
  c3_version: u32,
  seed: u64,
  spec_digest: [u8; 32],

  holes:   [Hole; 9],           // play order
  nodes:   Vec<Node>,           // tees, greens, junctions
  budgets: Budgets,
  plasticity: f64,              // [0,1], carried from C2
}

Hole {
  index: u32,                   // 0..9, play order
  par: u32,                     // from course_spec::PAR_SEQUENCE
  tee: NodeId,
  green: NodeId,
  spine: Vec<Vec2>,             // centreline, tee -> green, world metres
  halfwidth_m: Vec<f64>,        // per spine vertex; the corridor envelope
  earthwork_budget_m3: f64,     // this hole's share of the total
  borrow_sites: Vec<BorrowSite>,// where its fill comes from
  target_grade: f64,            // rise/run, from the scorer preset
  neighbors: Vec<HoleIndex>,    // corridors this one abuts -- seam list
}

Node { id: NodeId, kind: Tee | Green | Junction, at: Vec2, elev_m: f64 }

BorrowSite { at: Vec2, radius_m: f64, volume_m3: f64, becomes_water: bool }

Budgets {
  total_cut_m3: f64,
  total_fill_m3: f64,
  total_clearing_m3: f64,
  per_hole_ms: f64,             // streaming budget, from 02-performance-budget
}
```

## Field-by-field

**`holes`** is in **play order**, and `holes[0]` is the hole that must realize
first. The streaming order is not a separate field because it is exactly this
order — encoding it twice invites the two to disagree.

**`spine`** is the corridor centreline in world metres, tee to green. It is a
polyline, not a spline: S8 fits the spline (`course_world::Spine`) when it
needs arc-length parameterization. Keeping C3 a polyline means S6 does not
have to commit to a smoothing that S8 might disagree with.

**`halfwidth_m`** is per spine vertex, so a corridor can pinch through a gap
and open on a landing area. Together with `spine` it defines the **corridor
envelope** — the region S7 may grade and S8 may build in. Outside the envelope
is not this hole's business.

**`earthwork_budget_m3`** is this hole's allocation, already divided. S6 does
the dividing because only S6 sees all nine holes at once; S7 streams and cannot
rebalance. **A hole that overruns its budget does not borrow from another
hole** — it grades less. This is what makes per-hole streaming safe.

**`borrow_sites`** are the located fill sources S6 committed to, derived from
C2's `borrow` field. `becomes_water: true` means the pit is intended to be a
pond and S10 should dress it as one; `false` means it is backfilled or hidden.
Sites are assigned to exactly one hole — a shared borrow pit would couple two
streaming units and is forbidden.

**`neighbors`** is the seam list: which corridors this one abuts. S9's blending
pass reads it to resolve seams using only data both holes already had. It is
computed once by S6, where the whole layout is visible.

**`target_grade`** is copied per hole rather than read from a preset so that a
`Hole` is self-contained — the same self-sufficiency rule as C2.

## Invariants

1. **Exactly 9 holes, par summing to 36.** Enforced against
   `course_spec::PAR_SEQUENCE`, which is the authority.
2. **A returning loop.** `holes[0].tee` and `holes[8].green` are within the
   clubhouse radius of one another. S6 owns the radius; C3 records the result.
3. **Every hole is independently realizable.** Given `RoutingSubstrate` and one
   `Hole`, S7 and S8 can build that hole completely. No hole depends on another
   hole's output. This is the streaming guarantee and it is testable directly.
4. **Corridor envelopes may overlap; grading may not.** Two corridors can share
   ground (a shared fairway edge is normal), but S7's grading is clipped so
   that a cell is graded by exactly one hole — the lower-indexed one. The seam
   is then blended by S9. Without this rule, streamed hole 6 would modify
   ground hole 1 was already played on.
5. **Budgets balance.** `sum(holes[i].earthwork_budget_m3) <= total_cut_m3 +
   total_fill_m3`, and available borrow covers required fill:
   `sum(borrow_sites.volume_m3) >= total_fill_m3`. S6 must not emit a plan S7
   cannot fund; this is checked at C3 construction, loudly.
6. **Spines stay inside the core.** Every spine vertex satisfies
   `course_world::world::in_core`.
7. **`halfwidth_m.len() == spine.len()`**, and every halfwidth is positive.
8. **No search residue.** C3 carries no scores, no alternatives, no beam state.
   If S7 or S8 ever wants to know *why* the router chose something, the answer
   is that they may not.

## Versioning

`C3_VERSION` bumps on field changes as usual. Note that C3 is cheaper to change
than C2: only S7–S10 sit downstream, and its goldens are per-hole rather than
whole-field.

## Future-biome seams

C3 is biome-agnostic by construction — it contains no biome data at all, only
geometry and budgets — and is expected to survive the future packs unchanged.

The one anticipated pressure is **Lowcountry Marsh** and any other pack where a
hole may be genuinely discontinuous (a forced carry over water, with no walkable
ground between). Today `spine` is a single polyline and implicitly continuous.
If a pack needs carries, the addition is a `carries: Vec<(usize, usize)>` span
list on `Hole` rather than a break in the spine — recorded here so the shape is
decided in advance rather than improvised under deadline.
