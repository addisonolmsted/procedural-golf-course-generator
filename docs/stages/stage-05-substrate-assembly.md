# Stage S5 — Substrate Assembly

**Status:** unclaimed
**Crate:** [`crates/course-substrate/`](../../crates/course-substrate/)

## Purpose

Gather everything upstream into **contract C2, the routing substrate**, and
draw the line. S5 is the last stage that knows how terrain was made; S6 onward
see only C2.

The stage is mostly bookkeeping, with one genuinely hard job: turning terrain
into **cost fields**. Everything the router will ever know about whether a
piece of ground is good for golf has to be expressed as a per-cell number here.
If S6 ever needs to ask a question C2 cannot answer, the fix is a new field in
S5 — never an import from upstream.

## Position

- **Upstream:** [S4](stage-04-finishers.md) (height),
  [S3](stage-03-transforms.md) (water, basins),
  [S2](stage-02-skeleton-kernel.md) (flow field, metadata),
  [S0](stage-00-archetype-draw.md) (scorer preset, plasticity).
- **Downstream:** [S6](stage-06-routing.md), and only S6.
- **Contracts:** emits **[C2](../contracts/C2-routing-substrate.md)**.
- **Streams:** `substrate/v1`.

## Inputs / Outputs

**In:** the finished 2 m heightfield; water bodies and basin inventory; S2's
flow field and `StructureMeta`; the `ScorerPreset` and `plasticity` from
`CourseSpec`.

**Out:** `RoutingSubstrate` — `substrate.json` + CGRID1 fields. Full shape,
field-by-field semantics, and invariants in
**[C2](../contracts/C2-routing-substrate.md)**; that document is authoritative
and this one does not restate it.

Two rungs: `height` at 2 m (1501²), everything else at 8 m (376²). The rungs
nest, so coarsening is subsampling, not resampling.

## Responsibilities

1. **Carry height through at 2 m**, storing the **bed** elevation under water
   bodies, not the water surface. Getting this backwards turns every lake into
   a plateau; it is the single most likely implementation bug in this stage.
2. **Build the exclusion masks.** `water` as *fractional* coverage per 8 m cell
   (an 8 m cell straddling a creek is genuinely half water; rounding either way
   gives impassable creeks or invisible ones). `protected` and
   `out_of_bounds` as the only hard booleans in the whole contract.
   `unbuildable` as a **soft** field — a 40% slope is expensive, not
   impossible, and encoding it as cost is what lets plasticity function.
3. **Build the cost fields.** The real work:
   - `earthwork` — cut-plus-fill m³ to bring the cell to the biome's target
     grades. Scaled by `plasticity`.
   - `clearing` — cover removal, expressed as an m³-equivalent so the scorer can
     sum it with earthwork without a unit conversion.
   - `borrow` — m³ of fill obtainable from the cell, derived from S3's closed
     basins and low-lying ground. **This is a resource, not a cost**; the
     scorer subtracts it. Without it, S6 routes courses needing fill from
     nowhere and S7's mass balance is unsolvable.
   - `drainage_risk` — ponding and erosion exposure from the flow field. Note
     it is also read by S7, the only cost field with a consumer past S6.
4. **Recompute flow on the final surface.** S2's flow field predates S3's datum
   ops and S4's texture. C2's `meta.flow_dir_rad` and `flow_accum` must describe
   the ground as it actually is.
5. **Pass metadata through unchanged.** `grain_axis_rad` and
   `wind_azimuth_rad` must be **bit-identical** to their C1 values. S5 may not
   recompute or refine them. That identity is what keeps S9's micro detail
   aligned with S2's macro structure nine stages later — and when it drifts, no
   single stage looks wrong, the terrain just stops reading as one place.
6. **Assemble the preset** and duplicate `plasticity` into C2, so a substrate
   loaded from disk is a complete input to S6 with no other file required.

## Biome expression

**Near-identity.** S5 has no biome dials of its own; it is a compiler. Biome
character reaches it entirely through the data it is compiling and the
`ScorerPreset` it passes along.

The one place biome shows through is the **shape of the cost fields**, and only
as a consequence:

| Biome | Cost-field character |
|---|---|
| Piedmont | Balanced. Moderate clearing (closed canopy), moderate earthwork. |
| Great Plains | Earthwork near zero almost everywhere; clearing near zero. Routing is nearly unconstrained, which is its own difficulty. |
| River Valley | Sharp `unbuildable` contrast between floodplain and valley sides; `borrow` high on the floor. |
| Sandhills | `clearing` identically zero; `water` identically zero; `borrow` near zero. **Three fields at zero simultaneously** — the case most likely to divide-by-zero in a naive scorer. |
| Heathland | `borrow` high (kettles); `water` patchy and fractional; `clearing` low. |
| Hill Country | `earthwork` and `unbuildable` dominate. The biome where plasticity matters most. |

## Determinism & budget

**400 ms** ([../02-performance-budget.md](../02-performance-budget.md)),
mostly cost-field construction. All per-cell; parallelize by fixed chunks.

Flow recomputation at 8 m is the one non-local operation — a single
Priority-Flood/D8 pass, not a per-cell search.

## Future-biome seams

C2 is meant to survive the packs, so S5 should too. The anticipated pressure is
**Lowcountry Marsh**, which tests whether fractional `water` plus
`water_surface_m` can express tidal marsh with a genuinely fuzzy, moving
boundary. If it cannot, the addition is a `water_permanence` field — noted in
C2 so the shape is decided in advance.

Build S5 so adding a cost field is a local change: a trait or a table of field
builders rather than a hand-written struct literal.

## Open questions

1. **Units for `clearing`.** "m³-equivalent" needs defining — presumably
   volume of material times a cost ratio, but the ratio has to come from
   somewhere. Proposal: make it a preset field so it is calibratable rather
   than a constant.
2. **How is `earthwork` computed without knowing the corridor?** Cut-fill to a
   target grade depends on which direction the hole runs. Proposal: compute the
   *isotropic* cost (cheapest grade in any direction) and let S6's fit term
   handle orientation. Needs confirming — it is the assumption S6's search
   quality rests on.
3. **Is 8 m the right rung for cost?** It is 16× cheaper than 2 m and about the
   scale of a fairway width, which is the argument for it. But a green complex
   is smaller than one 8 m cell. If S6 needs finer greens siting, C2 may need a
   third rung rather than S6 sampling `height` directly.
4. **Should `drainage_risk` be split** into ponding and erosion? S7 wants them
   separately; S6 does not care. Leaning single field until S7 proves otherwise.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical C2 artifact, cross-platform.
- [ ] **All C2 invariants hold and are tested**: self-sufficiency, two hard
      masks only, non-negative finite costs, nesting rungs, bed-not-surface,
      metadata bit-identity.
- [ ] **Dependency check in CI**: `course-routing` depends only on
      `course-contracts` and `course-world`. This is checkable from the
      dependency graph and should fail the build if violated.
- [ ] `meta.grain_axis_rad` and `meta.wind_azimuth_rad` are bit-identical to
      C1's (assert equality, not approximate equality).
- [ ] A C2 loaded from disk alone is a valid, complete input to S6 — proven by
      an S6 fixture test that loads only that file.
- [ ] Sandhills produces `water`, `clearing`, and `borrow` all identically zero,
      and S6 routes on it without a division by zero.
- [ ] `height` under every water body is the bed, verified against S3's water
      surfaces.
- [ ] No infinities anywhere in the cost fields.
- [ ] Golden C2 for seed 1, piedmont and heathland.
- [ ] Budget: < 400 ms.
