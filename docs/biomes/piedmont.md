# Piedmont

**Role: the primary golden archetype.** The reference case — every module at
moderate intensity, nothing degenerate, nothing at zero. If piedmont breaks,
something fundamental broke.

## Identity

Rolling fluvial country on deeply weathered crystalline rock with a thin
sedimentary veneer: the ground between mountain front and coastal plain. Mature,
well-integrated drainage — creeks in shallow wooded corridors, broad convex
interfluves between them, clay soils that hold water and support closed
hardwood canopy.

The golf is classic parkland. Corridors mown through forest, creeks crossing
holes at inconvenient angles, greens sited on interfluve noses or benched
beside a creek.

## Kernel & modules

Kernel I (fluvial). Everything at moderate intensity.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **low** | A creek, not a river. Narrow floodplain, no terrace flight, no scroll stamps. |
| Stratigraphy / bench | **low** | Thin veneer; slight benching at the weathering front. |
| Closed basin | **zero** | Fully integrated drainage; nothing closed. |
| Aeolian | **zero** | |
| Drainage integration | **high, positive** | The reference for what "integrated" means. |

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | Mid relief amplitude; thin strata; mid `grain_strength` from a geological fabric. Near-identity. |
| [S2](../stages/stage-02-skeleton-kernel.md) | Mid drainage density. Convex interfluve catena — the signature hillslope form. |
| [S3](../stages/stage-03-amplification.md) | **The reference dictionary.** Spur-and-hollow interfluves — the classic ridge-and-ravine texture. Mid amplitude. Piedmont is fitted first and the other five are validated partly by how their texture differs from it. |
| [S4](../stages/stage-04-hydrology.md) | Light floodplain along the trunk. Water table below the surface almost everywhere: creeks, no lakes. |
| [S9](../stages/stage-09-micro-repass.md) | Swales dominant, mid amplitude; light terracettes on steeper rough. |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Closed hardwood canopy with mown corridors cut through it. |

## Scorer preset & plasticity

**Plasticity: mid.** The land is workable but worth following. Balanced
`w_fit` and `w_earthwork`.

Cost fields are balanced: moderate `clearing` (closed canopy), moderate
`earthwork`, `borrow` available in creek bottoms, `water` present as narrow
fractional creek cells. No field is at zero — which is exactly why piedmont is
the primary golden and exactly why it is a poor test of anything.

## Calibration targets

**Corpus: v1 piedmont tiles exist but only 6 of 14 are clean.** Best-supplied
of the six and still thin. **Re-measure and scale up.**

| Metric | Role | Notes |
|---|---|---|
| `mean_abs_profile_curv` | **discriminant** | The spur-and-hollow signature. Piedmont's single most characteristic quantity, and exactly what a flow-distance catena cannot produce. |
| `spectral_slope_beta` | **discriminant** | The reference β; every other biome is described relative to it. |
| `variogram_range` / `sill` | **discriminant** | The dictionary's wavelength and amplitude reference. |
| `drainage_density`, `dist_to_channel_p50` | shared invariant | Must be hit, carries no identity — 104–120 m like everything else. |
| `slope_area_theta`, `hypsometric_integral` | shared invariant | |
| `local_relief_p50` | amplitude | Per-site, matched in distribution. |

Piedmont's role in calibration is to be the **anchor**: measured first, fitted
first, and the reference against which the other five biomes' discriminants are
sanity-checked. Nothing else can be validated until it exists.

## Signature exaggeration

**Corridor enclosure — the tunnel of trees.** Canopy pressed slightly closer to
the corridor edge than reality, and slightly taller, so a hole reads as a room.
This is the one thing piedmont overdoes.

## Open questions

1. **Is "thin strata" worth having at all**, or should piedmont run
   stratigraphy at zero? The weathering front does produce slight benching, but
   if it is not measurable, drop it and let Hill Country own the module.
2. **Creek crossing frequency.** Mid drainage density gives some number of
   crossings per course, and that number is a large part of what makes the golf
   piedmont. It should probably be an explicit calibration target rather than
   an emergent consequence.
3. Piedmont being the anchor means its corpus quality bounds everything. Is 28
   tiles enough, or should it be scaled up first?
