# Stage S5 — Site Selection & Substrate Assembly

**Status:** unclaimed
**Crate:** [`crates/course-substrate/`](../../crates/course-substrate/)

## Purpose

Two jobs. First, **choose where on the generated land the course goes** —
score the core and select a play window. Second, gather everything upstream
into **contract C2**, the routing substrate, and draw the line past which no
stage knows how terrain was made.

Site selection is the part that is new and the part that earns its keep. Real
course siting is an architect picking the good 150 acres out of 500; until now
the routable area was nailed to the whole core and routing took whatever ground
it was handed. Choosing a window improves the golf *and* is a large source of
between-seed variety — the same terrain yields a genuinely different course
depending on where the window lands.

## Position

- **Upstream:** [S4](stage-04-hydrology.md) (height, flow, water, basins);
  [S2](stage-02-skeleton-kernel.md) (`StructureMeta`);
  [S3](stage-03-amplification.md) (exemplar provenance);
  [S0](stage-00-archetype-draw.md) (scorer preset, plasticity).
- **Downstream:** [S6](stage-06-routing.md), and only S6.
- **Contracts:** emits **[C2](../contracts/C2-routing-substrate.md)**.
- **Streams:** `substrate/v1`.

## Inputs / Outputs

**In:** the amplified and datum-adjusted 2 m heightfield, flow field, water
bodies, and basin inventory from [S4](stage-04-hydrology.md); `StructureMeta`
originating at [S1](stage-01-macro-primitives.md) and passed through
[S2](stage-02-skeleton-kernel.md); the cover mosaic for `clearing`; the
`ScorerPreset` and `plasticity` from [S0](stage-00-archetype-draw.md); the
fitted siting-score weights.

**Out:** `RoutingSubstrate` — `substrate.json` plus CGRID1 fields.

```
RoutingSubstrate {
  c2_version: u32, seed: u64, spec_digest: [u8; 32],
  grid_full:   GridSpec,        // 2 m, 1501^2
  grid_cost:   GridSpec,        // 8 m, 376^2
  height:      Field<f64>,      // m, at grid_full; BED under water
  play_window: Rect,            // the 600 m selected window
  masks:       ExclusionMasks,  // water, water_surface_m, protected,
                                //   out_of_bounds, unbuildable
  cost:        CostFields,      // earthwork, clearing, borrow, drainage_risk
  meta:        SubstrateMeta,   // flow, grain axis, wind azimuth,
                                //   water provenance, plasticity
  preset:      ScorerPreset,
}
```

Field-by-field semantics and the full invariant list live in
**[C2](../contracts/C2-routing-substrate.md)**, which is authoritative; this doc
does not restate them.

## The siting geometry

Fixed, and it moves no world constant.

| Quantity | Value |
|---|---|
| World extent | 3000 m (`EXTENT_M`) |
| Search region | the core, `[750, 2250]²` (`CORE_MIN_M`/`CORE_MAX_M`, `in_core`) |
| Play window | **600 m axis-aligned square** (`PLAY_M`) |
| Window centre freedom | **±450 m per axis** from world centre |
| Minimum terrain margin | **750 m** from any played point to the world edge |

The arithmetic closes exactly: a 600 m window whose centre ranges ±450 m spans
`[750, 2250]²` — precisely the existing core. So the core becomes the *search
region* rather than the routable area, `in_core` still bounds everything, and
the margin guarantee holds by construction.

**That 750 m margin is a product requirement, not an accident.** A hole at the
edge of the played area must still have terrain running out beyond it; the
player must never see the world end. Any future change to `PLAY_M` or the
centre range must preserve it.

### Translation only — no rotation

Rotation was considered and rejected. It is **redundant**: S1 already draws
`grain_axis_rad` and the base-level edge at random, so rotating the window
relative to the terrain is the same thing as rotating the terrain relative to
the window, and the latter is already randomized.

It also costs three real things:

1. A rotated window needs resampling into the rotated frame, which breaks the
   **nesting property** the whole resolution ladder depends on — 8 m nodes
   coinciding with every 4th 2 m node ([../01-conventions.md](../01-conventions.md)).
2. It adds a bilinear resample to the determinism surface.
3. It hands the renderer a heightmap with a transform attached instead of a
   plain axis-aligned grid. **Deliverable heightmaps stay axis-aligned.**

Axis-alignment also buys freedom rather than costing it: without the
half-diagonal penalty, `offset + 300 ≤ 750` gives ±450 m instead of the ±326 m
a rotated window would allow.

## Responsibilities

### 1. Score the core for golf suitability

A per-cell score at 8 m over the core, combining: workable grade, drainage
safety, water proximity (an amenity up to a point, a hazard past it), relief
interest, and feasible-area contiguity.

**The scorer is calibrated, not hand-authored.** The 64 real golf-course grids
in `out/courses/` are ground truth: measuring what terrain real architects
actually chose turns siting from a heuristic into a fitted model. See
[../calibration/targets.md](../calibration/targets.md).

### 2. Scan and select the window

Slide the 600 m window over the ±450 m centre range and pick the best.
Candidate positions on the 8 m grid number ~113² ≈ 12 700 — cheap to score
exhaustively with a summed-area table, so no search heuristic is needed and
none should be introduced.

Selection must be **deterministic and totally ordered**: ties broken on a
stable key, never on float equality.

### 3. Assemble C2

Full field-by-field semantics and invariants live in
**[C2](../contracts/C2-routing-substrate.md)**; that document is authoritative
and this one does not restate it. The work:

- **Height at 2 m**, carrying the **bed** elevation under water bodies, not the
  water surface. Getting this backwards turns every lake into a plateau and is
  the most likely single implementation bug in this stage.
- **Exclusion masks** — `water` as *fractional* coverage per 8 m cell (a cell
  straddling a creek is genuinely half water; rounding either way gives
  impassable creeks or invisible ones); `protected` and `out_of_bounds` as the
  only hard booleans; `unbuildable` deliberately **soft**, because a 40% slope
  is expensive rather than impossible and encoding it as cost is what lets
  plasticity function.
- **Cost fields** — `earthwork` (cut+fill m³ to reach target grade, scaled by
  plasticity), `clearing` (m³-equivalent so it sums with earthwork without a
  unit conversion), `borrow` (**a resource, not a cost** — the scorer subtracts
  it; without it S6 routes courses needing fill from nowhere), and
  `drainage_risk` (the only cost field with a consumer past S6 — S7 reads it).
- **`play_window`** — the selected rect. S6 routes inside it.
- **Metadata pass-through** — `grain_axis_rad` and `wind_azimuth_rad`
  **bit-identical** to their C1 values. S5 may not recompute or refine them.
  That identity is what keeps S9's micro detail aligned with S2's macro
  structure; when it drifts, no single stage looks wrong — the terrain just
  stops reading as one place.
- **Preset and plasticity**, duplicated into C2 so a substrate loaded from disk
  is a complete input to S6 with no other file required.

## Biome expression

**Near-identity for assembly** — S5 is a compiler and has no biome dials of its
own. Biome character reaches C2 entirely through the data being compiled and
the preset being passed along.

Siting, however, does differ in *difficulty*:

| Biome | Cost-field character and siting problem |
|---|---|
| Piedmont | Balanced. Moderate clearing and earthwork. The reference case. |
| Great Plains | Costs near zero almost everywhere, so the **siting score is nearly flat** — the window search has little to discriminate on, and the same symmetry problem shows up again at S6. |
| River Valley | Sharp contrast between floodplain and valley sides; `borrow` high on the floor. Siting must resist parking the whole window on the flat. |
| Sandhills | `water`, `clearing`, and `borrow` **all identically zero** — the case most likely to divide by zero in a naive scorer. |
| Heathland | `borrow` high (kettles), `water` patchy and fractional, `clearing` low. Many small obstacles. |
| Hill Country | `earthwork` and `unbuildable` dominate; feasible ground scarce and often non-contiguous. **Siting matters most here** — a badly placed window may have no viable route at all, which under no-retry has no fallback. |

## Determinism & budget

**400 ms** ([../02-performance-budget.md](../02-performance-budget.md)) —
mostly cost-field construction; the window scan is a summed-area table and is
nearly free.

All per-cell work parallelizes by fixed chunks. Window selection is totally
ordered with a stable tie-break.

## Future-biome seams

C2 is meant to survive the packs, so S5 should too. The anticipated pressure is
**Lowcountry Marsh**, which tests whether fractional `water` plus
`water_surface_m` can express tidal marsh with a genuinely fuzzy, moving
boundary. If it cannot, the addition is a `water_permanence` field — noted in
C2 so the shape is decided in advance.

Build S5 so adding a cost field is local: a table of field builders rather than
a hand-written struct literal.

## Open questions

1. **What exactly is in the siting score?** The term list above is a sketch.
   The calibrated answer comes from the 64 course grids. **Blocks
   implementation of `siting`.**
2. **Should siting see routability directly?** A cheap proxy — largest
   contiguous feasible region inside the window — is probably worth more than
   any smooth score, especially for Hill Country. Suspect yes.
3. **Units for `clearing`.** "m³-equivalent" needs defining. Proposal: make the
   cost ratio a preset field so it is calibratable rather than a constant.
4. **How is `earthwork` computed without knowing the corridor?** Cut-fill to a
   target grade depends on hole direction. Proposal: compute the *isotropic*
   cost and let S6's fit term handle orientation. This is the assumption S6's
   search quality rests on — confirm it.
5. **Is 8 m the right cost rung?** About a fairway width, and 16× cheaper than
   2 m. But a green complex is smaller than one 8 m cell; if S6 needs finer
   greens siting, C2 may need a third rung.
6. **Does the window need a halo** for corridors that bulge past it? Probably a
   ~50 m tolerance, which the 750 m margin absorbs easily.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical C2, cross-platform.
- [ ] **The siting arithmetic is asserted as a test**: `PLAY_M = 600`, centre
      range ±450 m, selected window always within `[750, 2250]²`, minimum
      distance from any window edge to the world edge ≥ 750 m.
- [ ] Window selection is exhaustive over the candidate grid and totally
      ordered; no float-equality tie-breaks.
- [ ] Siting measurably beats a fixed centred window on the golf-suitability
      score across 1 000 seeds — otherwise the stage is not earning its budget.
- [ ] **All C2 invariants hold and are tested**: self-sufficiency, two hard
      masks only, non-negative finite costs, nesting rungs, bed-not-surface,
      metadata bit-identity.
- [ ] **Dependency check in CI**: `course-routing` depends only on
      `course-contracts` and `course-world`. Enforced from the dependency
      graph.
- [ ] `meta.grain_axis_rad` and `meta.wind_azimuth_rad` bit-identical to C1's
      (assert equality, not approximate equality).
- [ ] A C2 loaded from disk alone is a complete input to S6 — proven by an S6
      fixture test that loads only that file.
- [ ] Sandhills produces `water`, `clearing`, and `borrow` all identically
      zero, and S6 routes on it without a division by zero.
- [ ] `height` under every water body is the bed, verified against S4's
      surfaces.
- [ ] No infinities anywhere in the cost fields.
- [ ] Golden C2 for seed 1, piedmont and heathland.
- [ ] Budget: < 400 ms.
