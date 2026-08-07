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
| [S3](../stages/stage-03-transforms.md) | Light floodplain along the trunk. Water table below the surface almost everywhere: creeks, no lakes. |
| [S4](../stages/stage-04-finishers.md) | **Mid** — balanced creep and rilling. The calibration reference for both. |
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

**Corpus: v1 piedmont tiles exist (28 extracted) and must be re-measured
against the new battery.** The best-supplied of the six.

| Metric | Family | Notes |
|---|---|---|
| Drainage density | process | The reference value; all others are described relative to it. |
| Slope–area θ | process | Well-constrained by the existing corpus. |
| Hypsometric integral | process | Convex interfluves give a characteristic signature. |
| Local relief | amplitude | Per-site, matched in distribution. |
| Short-lag roughness | amplitude | The S4 reference. |
| Variogram range/sill | amplitude | The S9 reference. |

Piedmont's role in calibration is to be the **anchor**: it is measured first,
and the other five biomes' targets are validated partly by how they differ from
it.

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
