# Contract C1 — Macro Primitives → Skeleton Kernel

**Between:** [S1 macro primitives](../stages/stage-01-macro-primitives.md) →
[S2 skeleton kernel](../stages/stage-02-skeleton-kernel.md)
**Type:** `course_contracts::contracts::primitive_field::PrimitiveField`
**Status:** guarded. Golden-seed tested; changes bump `C1_VERSION` and
`PIPELINE_VERSION`.

## Why this contract exists

C1 is the seam that lets **future biome kernels plug in without touching S1**.
The base six all select Kernel I (fluvial), but a glacial or aeolian-dominant
kernel must be able to consume exactly the same primitive field and build
something structurally unrelated from it. So C1 describes *the site's
predisposition*, never *the landforms*.

The distinction is sharp and it is the thing to get right: C1 says "this
quadrant sits higher and the material grain runs NNE"; it does not say "there
is a ridge here." Where the ridge actually goes is the kernel's decision, and a
different kernel would put something else there.

## Shape

```
PrimitiveField {
  c1_version: u32,
  seed: u64,                    // provenance; matches RunIdentity
  spec_digest: [u8; 32],        // blake3 of the CourseSpec that produced this

  grid: GridSpec,               // 8 m rung, full extent (376²)

  // --- the predisposition fields, all full-grid, all 8 m ---
  tilt:        Field<f64>,      // m, the regional plane
  relief:      Field<f64>,      // m, low-frequency elevation predisposition
  hardness:    Field<f64>,      // [0,1], differential erosion resistance
  accommodation: Field<f64>,    // [0,1], room for deposition

  // --- structural metadata (see course-contracts::metadata) ---
  meta: StructureMeta {
    grain_axis_rad: f64,        // AXIS in [0,pi) -- material/structural fabric
    grain_strength: f64,        // [0,1], 0 = isotropic
    base_level: { edge: Edge, elev_m: f64 },   // where water leaves the box
    wind_azimuth_rad: f64,      // DIRECTION in [0,2pi)
    strata: [ Stratum { thickness_m, hardness, dip_rad, strike_axis_rad } ],
  },
}
```

`Field<f64>` is a `course_world::Grid` payload. All four fields share `grid`.

## Field-by-field

**`tilt`** — the regional plane, evaluated per node. Monotone toward
`meta.base_level.edge` within a tolerance; this is what fixes drainage
direction for the whole course. A kernel may build against the tilt but may not
contradict it: water leaves through the declared edge.

**`relief`** — low-frequency elevation predisposition in metres, mean-zero over
the core. This is *where the site is high and low*, at a wavelength no shorter
than ~400 m. It is emphatically not a heightfield: adding `tilt + relief` gives
a smooth blob, not terrain. The kernel treats it as a bias on where structure
wants to sit.

**`hardness`** — [0, 1] differential erosion resistance. Kernel I reads it as
erodibility for the catena library and as the reason tributaries deflect. The
stratigraphy module writes benches where `hardness` steps. Uniform 0.5 is
legal and means "no material contrast".

**`accommodation`** — [0, 1], room for deposition. High where a basin can fill,
low on an interfluve. The trunk-river module reads it for floodplain width; the
closed-basin module reads it to decide where basins can persist. This is the
field that carries "this is a valley floor" without naming a valley.

**`meta.grain_*`** — the structural fabric. An **axis** in `[0, π)`, never a
direction; see [../01-conventions.md](../01-conventions.md). Everything
downstream that orients detail — tributary elongation, aeolian bands,
terracettes, vegetation striping — traces back to this one axis. That single
shared origin is what makes a generated site read as coherent instead of as
several textures on the same hill.

**`meta.base_level`** — the edge (or corner) where water leaves and its
elevation in the local datum. Kernel I roots the trunk here. **This is the one
piece of C1 a kernel may not reinterpret.**

**`meta.wind_azimuth_rad`** — a direction in `[0, 2π)`. Zero-length is not
legal; a biome with no meaningful wind still declares one and runs the aeolian
module at zero intensity.

**`meta.strata`** — the stack, ordered top-down. May be empty (Heathland,
Sandhills). Consumed by the stratigraphy module shared by hill-country,
great-plains, and river-valley.

## Invariants

1. **Full-grid, no holes.** Every field is defined at every node. No NaN, no
   sentinel values, no masked regions. A kernel never has to check.
2. **Ranges are enforced at construction**, not asserted downstream. `hardness`
   and `accommodation` are clamped to [0, 1]; `grain_axis_rad` is reduced into
   `[0, π)`; `wind_azimuth_rad` into `[0, 2π)`.
3. **`relief` is mean-zero over the core** to within 1 cm. Absolute elevation
   is `tilt`'s job.
4. **Band-limited.** `relief` carries no energy below ~400 m wavelength. S1
   producing detail is S1 doing S2's job, and it shows up as double-structured
   terrain.
5. **The empty case is legal.** `strata` may be empty; `grain_strength` may be
   0; `hardness` may be uniform. Kernels must be correct on all three.
6. **No landform vocabulary.** C1 contains no field named `ridge`, `valley`,
   `channel`, or `bench`. If one appears, the contract has been violated in
   spirit even if it still compiles.

## Versioning

`C1_VERSION` is bumped by: adding or removing a field, changing a range or
unit, or changing the grid rung. Adding a `Stratum` member is a bump. Changing
*how S1 computes* a field without changing its meaning is not a bump, but it
does re-bless goldens.

## Future-biome seams

C1 is designed to be kernel-agnostic and is expected to survive the future
packs unchanged. Two known pressures:

- **The North (glacial):** needs an ice-flow direction. This should arrive as a
  `meta` addition, not a new field — glacial structure is a transform over a
  fluvial substrate, not a replacement for it.
- **Fire & Stone (volcanic):** needs a vent point set. This is the one case
  likely to require a genuine C1 addition, because a vent is a *located*
  feature and C1 currently has no vocabulary for located features. Deferred
  until the pack is designed; noting it here so it is not a surprise.
