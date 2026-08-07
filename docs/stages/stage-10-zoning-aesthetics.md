# Stage S10 — Zoning & Aesthetics

**Status:** unclaimed
**Crate:** [`crates/course-zoning/`](../../crates/course-zoning/)

## Purpose

Make it look like somewhere. S10 dresses the finished ground: the vegetation
and enclosure mosaic, what the substrate reveals where cover thins, the palette,
the light, and the placement manifests the renderer consumes.

This is where a biome stops being a set of dials and starts being recognizable.
Everything before S10 makes terrain that is *statistically* piedmont; S10 is
what makes it *look* like piedmont.

S10 is also the last stage permitted to touch appearance. S11 only measures.

## Position

- **Upstream:** [S9](stage-09-micro-repass.md) (final heightfield, firmness,
  TWI); [S8](stage-08-hole-layout.md) (zones, hazards);
  [S5](stage-05-substrate-assembly.md) via C2 (`meta.water_plane_origin`);
  [S0](stage-00-archetype-draw.md) (`aesthetic_seed`).
- **Downstream:** [S11](stage-11-validation.md), and the renderer.
- **Streams:** `zoning/v1`, seeded from `aesthetic_seed`.

## Inputs / Outputs

**In:** the final heightfield and surface fields; hole zones and hazards; water
bodies with their provenance; the biome's aesthetic record.

**Out:**

```
Zoning {
  zoning_version: u32,
  hole: u32,
  cover:      Grid<CoverClass>,   // 0.5 m -- fairway/rough/heather/sand/...
  canopy:     Grid<f64>,          // [0,1] density
  enclosure:  EnclosureModel,     // what is visible from where -- S11 reads this
  materials:  Grid<MaterialId>,
  palette:    PaletteChord,
  light:      LightRig,
  placements: Vec<Placement>,     // trees, rocks, furniture, cart path, scatter
}
```

## Responsibilities

1. **Vegetation and enclosure mosaic.** Assign cover classes conditioned on
   wetness (TWI), aspect, elevation band, and slope — not painted arbitrarily.
   Vegetation that ignores wetness is the most immediately noticeable
   procedural tell after isotropic noise: trees on a dry ridge and none in a
   wet hollow reads as wrong instantly, even to someone who could not say why.
2. **Build the enclosure model.** What a player can see from where. S11 checks
   sightlines against it, so it is a real model, not a rendering artifact.
3. **Substrate-reveal materials.** What the ground shows where cover thins —
   sand, clay, gravel, rock. Follows S1's `hardness` and the strata stack, so a
   scarp reveals the stratum it actually cut.
4. **Palette chord and fairway tint.** Per biome, seeded by `aesthetic_seed` so
   two piedmont courses differ in colour without differing in kind.
5. **Light rig.** Time of day, sun azimuth and elevation, atmosphere.
6. **Signature exaggeration.** Each biome is permitted **one** thing to overdo
   — see the table below. Applied here, budgeted per biome, and deliberately
   singular: a course that exaggerates three things reads as a caricature, and
   a course that exaggerates nothing reads as generic.
7. **Placement manifests.** Trees, rocks, furniture, cart paths, scatter — with
   seed-deterministic IDs so the renderer can stream and cull them stably.
8. **Dress water by provenance.** `meta.water_plane_origin` says whether a body
   is a kettle, an oxbow, or a floodplain pond. A kettle has a soft heathery
   margin; an oxbow has a silted flat and a cut bank. This is why the provenance
   thread is carried through C2 at all.

## Biome expression

S10 is the most biome-expressive stage in the pipeline. Full records in
[../biomes/](../biomes/); summary:

| Biome | Cover character | Signature exaggeration |
|---|---|---|
| **Piedmont** | Closed hardwood canopy, mown corridors cut through it | Corridor enclosure — the tunnel of trees |
| **Great Plains** | Open grassland, isolated tree groups, big sky | Horizon width and sky dominance |
| **River Valley** | Riparian gallery forest on the floor, open terraces above | The terrace flight read as stacked levels |
| **Sandhills** | Open sand and native grass; almost no canopy | Blowout scale — sand faces larger than life |
| **Heathland** | Heather and gorse, scattered pine, no closed canopy | Heather colour saturation in bloom |
| **Hill Country** | Scrub oak and cedar, thin soil, rock outcrop | Bench and scarp verticality |

## Determinism & budget

**700 ms total across 9 holes** — about **78 ms per hole**
([../02-performance-budget.md](../02-performance-budget.md)).

Placement uses `zoning/v1` seeded from `aesthetic_seed`. Placement IDs must be
**derived from position**, not from an incrementing counter — a counter makes
IDs depend on streaming order, and the renderer's culling then differs run to
run.

Cover classification is per-cell and parallel by fixed chunks.

## Future-biome seams

None structural — S10 is entirely data-driven from the biome's aesthetic
record. Adding a pack adds records: cover classes, palettes, placement kits,
and one signature exaggeration each.

The one thing to build for extension is the **cover class enum**, which will
grow with every pack (mangrove, saguaro, tussock, lava field). Make it
data-driven or trivially extensible rather than a closed match.

## Open questions

1. **What is the enclosure model, concretely?** A horizon-angle field per cell?
   A visibility graph between zones? S11 needs to query "what is visible from
   the tee", and the answer determines the representation. **Blocks S11's
   sightline check as much as it blocks S10.**
2. **Signature exaggeration budget.** "One thing, overdone" needs a magnitude.
   Presumably a multiplier with a certified ceiling, since an exaggeration that
   escapes its ceiling is exactly how procedural content becomes parody.
3. **Are placements instanced or unique?** Instancing is far cheaper and almost
   certainly right for trees, but hero features may want uniqueness. Leaning
   instanced with per-instance jitter.
4. **Does cover feed back into playability?** Rough density affects lie. If so,
   S9's firmness field and S10's cover both describe ball response and the
   division needs stating — currently ambiguous.
5. **Cart paths** — routed here, or in S8? They are placement, but they are
   also geometry constrained by the corridor. Leaning S10 with S8's zones as
   input.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical zoning output, cross-platform.
- [ ] **Placement IDs are position-derived**, stable under permuted streaming
      order. Test by permuting.
- [ ] Cover responds measurably to TWI, aspect, and elevation band: correlation
      between canopy density and wetness is significant and of the right sign.
- [ ] Water bodies are dressed according to `water_plane_origin` — a kettle and
      an oxbow of identical shape produce visibly different margins.
- [ ] Materials follow `hardness` and the strata stack; a scarp reveals the
      stratum it cut.
- [ ] Exactly **one** signature exaggeration is applied per biome, within its
      ceiling. Assert the count.
- [ ] All six biomes produce a complete manifest; Sandhills produces near-zero
      canopy without a degenerate or empty mosaic.
- [ ] The enclosure model answers S11's sightline queries — proven by S11's
      test, not by inspection.
- [ ] `aesthetic_seed` measurably varies palette between two runs of the same
      biome without changing cover class assignment.
- [ ] Golden manifests for seed 1, piedmont and heathland, holes 1 and 5.
- [ ] Budget: < 78 ms per hole, < 700 ms total.
