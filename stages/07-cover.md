# Stage 07 — Cover Assignment

**Status:** unclaimed

## Purpose

Assign canopy, understory, and ground substrate from the archetype's biome
kit, conditioned on the terrain (aspect, wetness index, elevation band) and
any anthropogenic overlay. Cover precedes routing because corridors care
about it (fairway candidacy, tree density along lines of play).

## Position

- Upstream: Stage 05 (`terrain/`), Stage 06 (`hydro/` incl. HAND/wetness),
  Stage 02 (mask — the core stays more open), Stage 00 (`cover.*` knobs).
- Downstream: Stage 08 (routable ground definition), Stage 09 (corridor
  costs), Stage 13 (tree instancing reads the canopy map).
- Streams: `cover/v1` — attempt scope (clump lattices).

## Contract

Input: `terrain/`, `hydro/`, mask, `CourseSpec`.

Output — `cover/` artifact:

```
class.cgrid    // u8: turf_candidate | rough | sand | wetland | water | rock | forest
canopy.cgrid   // f32 density 0..1 (independent of class)
substrate.cgrid// u8: sand | loam | clay | peat | rock  (art + earthworks read it)
cover.json     // achieved fractions per class (reported vs θ targets)
```

Assignment: biome-kit weights (θ per archetype) × conditioning factors —
wetness index (HAND, water adjacency), slope, aspect (canopy denser on
cooler aspects where the archetype says so), elevation band (mountain
tree-line effects), clumped at `cover.clump_scale_m` so cover reads as
mosaics. Inside the mask: forest/wetland propensity damped by
`cover.mask_open_boost` — the cover half of the routability guarantee.
Water/wetland classes must agree exactly with Stage 06's entities.

## Per-archetype behavior (via biome kit knobs)

- sandhills: sand/waste 25–60%, open canopy, no wetland.
- piedmont: canopy 50–80%, floodplain forest + creek fringes.
- florida_lowland: marsh/wetland heavy, mid canopy, peat substrate lows.
- glacial_moraine: open heathland, kettle bogs, gravelly substrate.
- mountain_bench: rock above slope threshold, aspect-driven forest,
  open benches.

## Hard requirements

1. Water cells in `class` = Stage 06 water footprint exactly; empty hydro ⇒
   zero water/wetland cells and the stage still completes.
2. Achieved class fractions within tolerance of θ targets (reported).
3. Mask openness: forest+wetland fraction inside mask ≤ θ cap.
4. Deterministic; attempt-scope resample; resolution-consistent mosaics.

## References

- Retired step-era cover fixture (`archetype-pipeline:course-contracts/src/fixtures.rs::fixture_cover`)
  — the rule-stack shape.
- `terrain-v2:tools/dtm_atlas` masks + atlas water/tree percentages — the
  calibration targets for canopy/water fractions.

## Open questions

- Anthropogenic overlay (old field lines, stone walls) — which archetypes,
  and is it v1? Draft: defer; contract reserves an `overlay` input slot.
- Substrate: is peat/clay/sand enough for earthworks cost modeling in
  Stage 10? Coordinate with that doc before build.
