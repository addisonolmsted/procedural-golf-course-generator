# Stage 06 — Hydrographic Extraction

**Status:** unclaimed

## Purpose

Re-run flow on the final surface and distill the water the game keeps as
OBJECTS: creek centerline splines with accumulation-scaled width profiles
(authored representation, sim-derived geometry), lakes and wetlands from the
flatten datums and retained depressions, and the water-table surface for
lowland archetypes.

## Position

- Upstream: Stage 05 (`terrain/`), Stage 03 datums (via `forcing.json`
  pass-through), Stage 00 (`hydro.*` knobs).
- Downstream: Stage 07 (wetness conditioning), Stage 08 (blockage/crossing
  checks), Stage 09 (hazards/constraints), Stage 11 (re-validation baseline),
  Stage 13 (hazard finalization).
- Streams: none (pure function of the surface).

## Contract

Input: `terrain/`, datum targets, `CourseSpec`.

Output — `hydro/` artifact:

```
flow.rcv.cgrid / flow.acc.cgrid   // D8 receivers + effective accumulation
hydro.json {
  creeks:   [ { spine: [[x,y],…],              // downstream-ordered spline
                width_profile: [[s, w_m],…],   // accumulation-scaled hydraulic
                strahler } ],                  //   geometry: w = c·A^e (θ)
  lakes:    [ { outline, surface_elev_m, source: datum | depression } ],
  wetlands: [ { outline, kind: fringe | kettle_bog | floodplain } ],
  water_table: null | { elev.cgrid ref },      // lowland archetypes
}
```

- Creeks: cells above the effective-accumulation channel threshold, traced
  to splines (`course-world::Spine`), widths from hydraulic geometry
  exponents (θ `hydro.chan_w_coef` / `chan_w_exp`) — never simulated.
- Lakes: datum outlines filled to datum elev + retained depressions below
  the θ pond threshold; wetlands from HAND bands around lakes/creeks and
  kettle bottoms.
- **The empty case is legal**: sandhills yields zero creeks/lakes/wetlands;
  every consumer must be correct on empty collections.

## Hard requirements

1. Every creek spline descends monotonically; widths positive,
   monotone-nondecreasing downstream between junctions.
2. Lake surfaces: outline cells' terrain ≤ surface elev; no lake leaks (a
   lake's outline is closed on the flow model).
3. Consistency with Stage 05's depression policy: every retained depression
   is accounted for (lake, wetland, or explicitly dry below pond threshold).
4. Deterministic; golden per archetype; empty-case test (sandhills).

## References

- `main:golf-terrain/src/water.rs` — port seed: entity extraction from a
  flow tree, HAND wetlands, hydraulic widths, outline tracing (2,647 lines
  of prior art).
- `tools/macro_campaign/macro_campaign/flow.py` — the Python mirror used by
  calibration (must produce matching drainage density on exported tiles).

## Open questions

- Braided/multi-thread channels for floodplain datums: out of scope v1?
  Draft: yes, single-thread only.
- Do wetlands need a per-cell moisture grid for Stage 07, or are outlines
  enough? Draft: also export the HAND grid — it is nearly free and Stage 07
  wants it.
