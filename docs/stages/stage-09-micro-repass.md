# Stage S9 — Micro Re-pass †

**Status:** unclaimed
**Crate:** [`crates/course-micro/`](../../crates/course-micro/)

## Purpose

Add the finest scale of detail — the ground texture a player actually sees and
the ball actually rolls over — and blend the seams between corridors.

The idea that makes S9 more than a noise pass: **detail is oriented by the
metadata thread, not sprinkled isotropically.** Swales follow flow direction.
Scroll arcs follow the trunk-river provenance. Aeolian rumple follows the wind
azimuth. Terracettes follow the contour. All of it traces back to
`grain_axis_rad` and `wind_azimuth_rad`, which have been carried bit-identically
from C1 through C2 to here.

That shared origin, nine stages deep, is what makes a generated site read as
one place rather than as several textures on the same hill. Isotropic noise at
this scale is the single most reliable way to make procedural terrain look
procedural.

S9 also emits the **surface fields** — firmness and TWI — that the playability
sim and the ball physics read.

## Position

- **Upstream:** [S8](stage-08-hole-layout.md) (layout, surface delta);
  [S7](stage-07-earthmoving.md) (graded patch);
  [S5](stage-05-substrate-assembly.md) via C2 (`meta` — the orientation
  thread).
- **Downstream:** [S10](stage-10-zoning-aesthetics.md),
  [S11](stage-11-validation.md). S9's output is the **final heightfield**.
- **Streams:** `micro/v1`.

## Inputs / Outputs

**In:** one hole's graded and shaped patch; its `HoleLayout` (for protected
zones); C2's `meta` and `water_plane_origin`; the neighbour list from C3 (for
seams); micro dials.

**Out:**

```
Micro {
  micro_version: u32,
  hole: u32,
  height: Grid<f64>,          // 0.5 m -- THE FINAL HEIGHTFIELD
  firmness: Grid<f64>,        // [0,1]; ball response
  twi: Grid<f64>,             // topographic wetness index
}
```

## Responsibilities

1. **Band-limited detail generation.** Detail confined to a specific wavelength
   band, with amplitude and band from lidar-measured variograms. Not broadband
   noise, and not fractal — real ground has a characteristic texture scale and
   reproducing the wrong one reads as wrong even when the amplitude is right.
2. **Orient every family by the metadata thread:**
   - **Swale fields** — along `flow_dir_rad`.
   - **Scroll arcs** — where `water_plane_origin` is a floodplain, following
     the trunk-river module's meander provenance.
   - **Aeolian rumple** — along `wind_azimuth_rad`.
   - **Terracettes** — contour-parallel on grazed slopes above a grade
     threshold.
3. **Respect protected zones.** Detail amplitude is **zero on greens and
   fairways**, tapering through approach into rough. A putting surface with
   micro-relief is unplayable, and this is the constraint most likely to be
   discovered late by a tester rather than early by a test.
4. **Blend seams.** Where corridors abut (C3's `neighbors`), blend across the
   boundary using **only data both holes already had**. Never revisit a
   previously-streamed hole — hole 1 may already have been played on.
5. **Emit firmness and TWI.** Firmness from substrate material, slope, and
   wetness; TWI from the local flow accumulation and slope. These are the
   fields the ball physics reads, and they are the reason S9 rather than S10 is
   the last stage that can make a hole playable.

## Biome expression

Which detail families are active, and their amplitudes.

| Biome | Active families |
|---|---|
| **Piedmont** | Swales dominant; light terracettes on steeper rough. Mid amplitude. |
| **Great Plains** | Swales, very low amplitude. Almost nothing else — the ground is genuinely smooth, and adding texture to make it "interesting" destroys the biome. |
| **River Valley** | **Scroll arcs dominant** on the floodplain — the signature feature, and the only biome where they carry real amplitude. Swales on the valley sides. |
| **Sandhills** | **Aeolian rumple dominant**, strongly aligned to `wind_azimuth_rad`. No swales (no flow). Highest micro amplitude of the six. |
| **Heathland** | Low amplitude, patchy. Hummocky rather than directional — the one biome where `grain_strength` is low enough that orientation barely reads. |
| **Hill Country** | **Terracettes dominant** on the grazed slopes; swales in the incised valleys. Strong contour alignment. |

Each family is a dial. Great Plains runs scroll arcs at zero; Sandhills runs
swales at zero. No branches.

## Determinism & budget

**1 400 ms total across 9 holes** — about **155 ms per hole**
([../02-performance-budget.md](../02-performance-budget.md)) at 0.5 m,
corridor-local.

Noise must be **position-seeded, not sequence-seeded**: value derived from
world coordinates plus the stream key, so a cell's detail does not depend on
iteration order or on which hole reached it first. This is what makes seam
blending consistent between two holes that render in either order, and it is
non-negotiable for a streaming stage.

Use `course_world::noise` (integer-hash gradient noise) rather than a float-hash
scheme.

## Calibration †

| Dial | Metric it is fit to | Family |
|---|---|---|
| Detail wavelength band | variogram range; spectral break | process |
| Detail amplitude | variogram sill; short-lag roughness | amplitude |
| Swale spacing | flow-oriented autocorrelation | process |
| Aeolian band spacing | directional variogram along wind azimuth | process |
| Terracette spacing | contour-parallel autocorrelation, grade-conditioned | process |
| Firmness mapping | — (no direct lidar analogue; hand-tuned against play) | — |

**Tile data required:** high-resolution DTM (1 m native, ideally sub-metre) for
all six biomes. Coarser tiles cannot resolve the wavelengths S9 operates at.

**The despeckle caveat is critical here and it cuts both ways.** Lidar noise
lives at exactly S9's wavelengths. Under-despeckle and S9 learns to reproduce
sensor noise as terrain texture; over-despeckle and the real micro-texture is
removed along with it and S9 learns that the ground is smooth. S9's variograms
are the measurement most sensitive to that choice in the whole battery — see
[../calibration/lidar-pipeline.md](../calibration/lidar-pipeline.md), and
expect to need a sensitivity analysis rather than a single despeckle setting.

## Future-biome seams

New detail families are added as modules, run at zero for biomes that do not
use them — the same pattern as S2's modules. Anticipated: pitting and rubble
(Fire & Stone), solution features (karst), frost polygons (The North). None
requires a structural change.

## Open questions

1. **Where does the taper from fairway to rough begin and end?** Amplitude zero
   on the green is unambiguous; the fairway edge is not. Needs a profile, and
   it likely differs by biome (sandhills fairway edges are genuinely rough).
2. **Is firmness derivable at all, or is it authored?** No lidar analogue
   exists, so it is hand-tuned against play. That makes it the one field in S9
   that cannot be certified offline — flag it as a validation-only quantity.
3. **Seam blending width.** Wide blends hide the seam but smear both holes'
   character. Proposal: blend over 2× the detail wavelength.
4. **Does S9 need to re-emit TWI after seam blending?** Blending changes local
   slope. Presumably yes; confirm the ordering.
5. **Interaction with S7's patch edges.** [S7's doc](stage-07-earthmoving.md)
   asks whether it should already match elevations at shared edges. Settle the
   division of labour in one place, not two.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical final heightfield, cross-platform.
- [ ] **Noise is position-seeded**: a cell's detail is identical regardless of
      which hole processes it first, and regardless of hole processing order.
      Test by permuting the streaming order.
- [ ] **Zero detail amplitude on greens and fairways**, verified by measuring
      short-lag roughness inside those zones against the pre-S9 surface.
- [ ] Every detail family is measurably oriented: directional autocorrelation
      aligns with the declared axis to within tolerance.
- [ ] Seam blending uses only data available to both holes — proven by
      permuting streaming order and asserting identical output.
- [ ] Family dials at zero produce exactly zero contribution (Sandhills swales,
      Great Plains scroll arcs).
- [ ] Variogram range and sill land inside the biome's target band.
- [ ] Firmness and TWI are defined everywhere in the corridor, in range, no NaN.
- [ ] Golden final heightfield for seed 1, piedmont and heathland, holes 1 and 5.
- [ ] Budget: < 155 ms per hole, < 1 400 ms total.
