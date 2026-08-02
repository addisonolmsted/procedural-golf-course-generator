# Stage 03 — Guidance Strokes

**Status:** unclaimed

## Purpose

The thinned skeleton of authored intent: a handful of strokes that bias the
sim without finishing any landform. Trunk channel spline to the base-level
outlet, the province-boundary landform if Stage 01 drew one, one hero
feature, and the flatten datums (floodplain, lake, marsh levels). Everything
here is a SHALLOW bias — amplitude caps are part of the contract; the LEM
does the finishing.

## Position

- Upstream: Stage 00 (`strokes.*` knobs), Stage 01 (`Framing`), Stage 02
  (mask + crossings).
- Downstream: Stage 04 compiles strokes into forcing fields; Stage 06 reads
  the flatten datums back (lake/marsh levels become water bodies).
- Streams: `strokes/v1` — **attempt scope**.

## Contract

Input: `CourseSpec`, `Framing`, mask artifact, `res_m`.

Output — `strokes.json`:

```
Strokes {
  trunk: null | { spine: [[x,y],…],           // to base_level edge; crosses the
                  carve_depth_m: f64,          // mask only at designated crossings
                  halfwidth_m: f64 },          // carve_depth ≤ strokes.carve_cap_m
  boundary_landform: null | { kind: scarp | valley_wall | material_contact,
                              curve: [[x,y],…], relief_m: f64 },
  hero: null | { kind: <per-archetype menu>, center: [x,y], scale: f64 },
  datums: [ { kind: floodplain | lake | marsh, outline: [[x,y],…],
              elev_m: f64 } ],                 // flatten targets, honored by 04/05/06
  kettle_field: null | { points: [[x,y],…], radius_m: [f64,…],
                         depth_m: [f64,…] },   // all OUTSIDE the mask
}
```

## Per-archetype grammar (the stroke menu, weights via knobs)

| Archetype | Trunk | Hero menu | Datums | Extra |
|---|---|---|---|---|
| piedmont | dendritic trunk, meandering | bluff bend, confluence | floodplain | — |
| sandhills | none (infiltrated) | mega-blowout, high dune | none | dune-crest orientation = grain (carried as forcing anisotropy, not strokes) |
| florida_lowland | none | sink lake | lake + marsh levels (the map's organizing feature) | — |
| glacial_moraine | optional short trunk | esker ridge, big kettle | kettle-pond levels | kettle point-field |
| mountain_bench | trunk in the valley floor | scarp amphitheater, hanging bench | terrace treads | boundary landform usually present |

## Hard requirements

1. Amplitude caps: every stroke's relief contribution ≤ its θ cap; the
   composed strokes surface (as Stage 04 will compile it) stays within
   `strokes.total_relief_cap_m` of the regional trend. Strokes bias, never
   finish.
2. Trunk falls monotonically to the base-level edge and crosses the mask
   only inside designated crossing corridors.
3. Kettle points and lake datum outlines do not intersect the mask.
4. Datum elevations are consistent: nested datums (marsh ⊂ floodplain)
   ordered correctly; all datums below their local regional-trend elevation.
5. Deterministic; attempt-scoped resample; canonical JSON.

## References

- `crates/course-world/src/spline.rs` (`Spine`, `catmull_rom`).
- Retired step-03 planner (commit 0313432) — drainage routing to an edge and
  monotone-fall enforcement are prior art
  (`crates/course-macro/src/planner.rs`, `network.rs`).

## Open questions

- Hero feature: exactly one always, or θ-probable? Draft: probability knob,
  ≤ 1 per map.
- Does the trunk get tributary stubs as strokes, or does the LEM grow all
  tributaries? Draft: LEM grows them (that is what it is for); revisit if
  piedmont drainage density comes out low against the corpus.
