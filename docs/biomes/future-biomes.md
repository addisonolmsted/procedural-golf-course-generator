# Future Biomes

An index, not a design. Each pack is listed with the **seam** it needs, so that
the seams cut today are the right ones. Nothing here is scheduled and nothing
here should be implemented ahead of its pack.

The rule that keeps this cheap: a new biome is normally a **data addition** — a
record plus a certified envelope. It becomes a code change only when it needs a
new kernel, a new transform, or a new detail family, and the packs below are
grouped by which of those they need.

## The seams that exist today

| Seam | Where | Status |
|---|---|---|
| `SkeletonKernel` trait | [`course-skeleton/src/kernel.rs`](../../crates/course-skeleton/src/kernel.rs) | Declared. Kernel I (fluvial) is the only implementation. |
| Glacial transform | [`course-transforms/src/seams/glacial.rs`](../../crates/course-transforms/src/seams/glacial.rs) | Declared, ordered, empty. |
| Boundary retreat | [`course-transforms/src/seams/boundary_retreat.rs`](../../crates/course-transforms/src/seams/boundary_retreat.rs) | Declared, ordered, empty. |
| S9 detail families | [`course-micro/`](../../crates/course-micro/) | Add a module, run at zero elsewhere. |
| S10 cover classes | [`course-zoning/`](../../crates/course-zoning/) | Grows with every pack; keep the class set extensible rather than a closed match. |

## The packs

### Coastal — **the priority pack**

Dune systems, tidal marsh, cliff-backed links, barrier islands.

- **Seam:** boundary retreat (S3), plus a likely C2 addition.
- **Kernel:** I, plus significant transform work.
- **Contains [Lowcountry Marsh](#lowcountry-marsh-the-designated-adversary).**

### Desert

Alluvial fans, arroyos, desert pavement, mesa-and-butte.

- **Seam:** none. Kernel I with the fluvial engine running ephemerally, high
  `unbuildable`, `clearing` at zero.
- **Notes:** the cheapest pack to add — it is almost entirely dials. A good
  first proof that "a biome is data" actually holds.

### Highlands

Glaciated uplands, corries, moorland, steep incised valleys.

- **Seam:** glacial transform (S3).
- **Kernel:** I plus the glacial transform, not a new kernel — glacial
  structure is a modifier over a fluvial substrate.
- **Notes:** stresses `unbuildable` and `earthwork` well beyond base-six
  ranges; the scorer weights need a much wider certified envelope.

### The North

Drumlin fields, esker trains, kettle-and-kame, frost-patterned ground.

- **Seam:** glacial transform (S3); **needs an ice-flow direction in C1's
  `meta`** — anticipated in
  [C1](../contracts/C1-primitives-to-kernel.md) as a `meta` addition rather
  than a new field.
- **Kernel:** possibly Kernel II if drumlins prove unbuildable as a transform.
- **S9:** frost-polygon detail family.

### Fire & Stone

Volcanic cones, lava fields, lahar plains, caldera floors.

- **Seam:** **the only pack likely to require a genuine C1 addition.** A vent is
  a *located* feature and C1 has no vocabulary for located features — see
  [C1](../contracts/C1-primitives-to-kernel.md).
- **Kernel:** Kernel V (volcanic). Structure radiates from vents rather than
  organizing around drainage, so it is a true kernel replacement.
- **S9:** pitting and rubble detail family.

### Tropical

Karst towers, rainforest lowland, volcanic islands.

- **Seam:** karst is awkward. Sinkholes **capture drainage**, which is
  restructuring — so it belongs in S3 as a transform, not S4 as a finisher.
  Possibly Kernel IV.
- **S4:** a chemical-weathering pass would be a new pass, not a new dial. See
  [S4's future-biome note](../stages/stage-04-finishers.md).
- **Notes:** the least-resolved pack. Karst may not fit the architecture
  cleanly, and finding that out is worth doing before committing to it.

## Lowcountry Marsh — the designated adversary

Part of the Coastal pack, and singled out because of the role it plays.

**When it lands it takes over the adversarial contract-tester role for
[C2](../contracts/C2-routing-substrate.md).** [Heathland](heathland.md) remains
the adversary for the base six; Lowcountry Marsh becomes the adversary for the
contract itself.

Why it is the right adversary: near-zero relief, water as the *dominant*
structure rather than an obstacle within it, and routability determined almost
entirely by what is **not** water. It is the furthest thing from the fluvial,
relief-organized assumptions every base biome shares. **If C2 survives
Lowcountry Marsh, C2 is right** — and the point of naming it now is that C2 is
being designed today, so it should be designed with this case in mind rather
than retrofitted for it.

Two specific pressures, both already recorded in the contracts so their shapes
are decided in advance rather than improvised:

1. **Fuzzy, moving water boundaries.** Tidal marsh is neither land nor water,
   and the boundary moves. If fractional `water` plus `water_surface_m` cannot
   express it, the addition is `water_permanence: Field<f64>`
   ([C2](../contracts/C2-routing-substrate.md)).
2. **Carries.** A hole may have no walkable ground between tee and green. The
   addition is a `carries` span list on `Hole`, not a break in the spine
   ([C3](../contracts/C3-corridor-realization.md)) — and a zero-halfwidth span
   in [S8](../stages/stage-08-hole-layout.md)'s width profile.

## Adding a pack: the checklist

1. Write the biome records in `docs/biomes/`.
2. Collect a corpus and fit targets
   ([../calibration/targets.md](../calibration/targets.md)).
3. Certify envelopes
   ([../calibration/envelope-certification.md](../calibration/envelope-certification.md)).
4. Add `BiomeId` variants (with their pack) and the S10 cover classes.
5. **Only if needed:** implement a kernel, transform, or detail family behind
   its existing seam.

If step 5 is required for a biome that is not structurally novel, the seam was
cut in the wrong place and that is worth fixing rather than working around.
