# Stage S3 — Amplification †

**Status:** unclaimed
**Crate:** [`crates/course-amplify/`](../../crates/course-amplify/)

## Purpose

Turn a correct-but-characterless base surface into terrain that looks like a
real place in a real region, by **reconstructing detail from a dictionary of
real terrain patches** fitted offline to that biome's lidar corpus and
conditioned on position within the drainage skeleton.

S3 is where realism and archetype identity come from. Two measured facts put
them here rather than upstream:

1. **The skeleton cannot carry identity.**
   `tools/macro_campaign/macro_campaign/structure.py` finds
   `dist_to_channel_p50` sits at **104–120 m in every archetype regardless of
   relief**, and flags it as the most robust metric in the set. Drainage
   spacing is a shared invariant of real landscapes, not a signature. Identity
   lives in hillslope form and texture.
2. **A distance transform cannot produce that form.** S2's catena is
   `f(flow_distance, hillslope_position)`, so every point equidistant from a
   channel gets the same treatment — smooth, tubular interfluves, the
   characteristic "candle wax" failure of hydrology-first synthesis. Real
   hillslopes alternate spur and hollow at a wavelength *shorter* than channel
   spacing, and that structure is not a function of distance-to-channel at all.

The dictionary supplies exactly what the base cannot, from data rather than
from parameters. **This is the project's central realism bet** — the previous
two attempts failed as unconditioned noise (no structure) and as authored
primitives (22/71 knobs inside the real-tile IQR, archive `0313432`).

## Position

- **Upstream:** [S2](stage-02-skeleton-kernel.md) — base surface, channel
  network, flow distance, divides, `StructureMeta`;
  [S0](stage-00-archetype-draw.md) — the exemplar pool and amplification dials.
- **Downstream:** [S4](stage-04-hydrology.md), which derives hydrology from
  S3's output.
- **Contracts:** none directly. S3's height becomes C2's `height` after S4.
- **Streams:** `amplify/v1`.

## Inputs / Outputs

**In:** the 2 m base surface; flow distance, flow direction, channel polylines
and divides at 8 m; `StructureMeta` (`grain_axis_rad`, `wind_azimuth_rad`); the
biome's exemplar pool and `amplify.*` dials; the fitted dictionary asset.

**Out:**

```
Amplified {
  amplify_version: u32,
  dictionary_fingerprint: [u8; 32],   // which dictionary produced this
  exemplars: Vec<ExemplarId>,         // the tiles whose DNA this course carries
  height:    Field<f64>,              // m, at 2 m -- the amplified surface
  residual:  Field<f64>,              // m, what S3 added (diagnostic + S11)
}
```

`exemplars` is provenance worth keeping: it records that this course's texture
came from, say, Uwharrie tile 7 blended with tile 12. It makes a generated
course traceable to real ground.

## Responsibilities

### 1. Compute the conditioning vector

Per cell, the vector that selects a dictionary bucket:

| Field | Source | Why |
|---|---|---|
| `flow_distance_norm` | S2, normalized channel→divide | position on the hillslope |
| `hillslope_position` | S2, relative elevation channel→divide | catena position, independent of absolute distance |
| `slope` | base surface | texture scales with gradient |
| `aspect_rel_grain` | base surface vs `grain_axis_rad` | orientation relative to fabric, **not** absolute compass |
| `tpi` | base surface | ridge / slope / hollow class |
| `relief_position` | base surface | where in the local relief range |

`aspect_rel_grain` being *relative* is what lets one dictionary serve any grain
orientation. Absolute aspect would tie the dictionary to the compass and
require a bucket per direction.

These are the same conditioning fields
`tools/dtm_metrics/dtm_metrics/surfaces.py` already computes (`lp_slope`,
`lp_aspect`, `tpi`, `relief_pos`) — its macro provider is documented as
pluggable, and that is the seam the offline fitter plugs into.

### 2. Reconstruct

For each patch location, select the bucket, draw basis coefficients from that
bucket's fitted distribution, and rebuild the residual patch. Coefficient draws
are **position-seeded** — derived from world coordinates plus the stream key,
never from an incrementing counter — so a cell's detail does not depend on
processing order.

### 3. Blend

Overlap-add with feathering. Patch synthesis visibly tiles when done naively;
the detectors are `variogram_range` and `spectral_slope_beta`, and S11 measures
both.

### 4. Taper near the skeleton

**The one hard constraint.** Residual amplitude tapers toward zero inside
channel corridors and along the trunk. The skeleton is what guarantees
routability and drainage coherence; amplification is decorating it, not
competing with it.

This taper replaces the "may never restructure" rule the old S4 carried. That
rule was unenforceable in spirit — any erosion strong enough to look real moves
channel heads, the least stable feature in a landscape. A taper is a
constructive constraint instead of a policed one, and unlike the old rule it
does not need channel heads to stay put: [S4](stage-04-hydrology.md) re-derives
hydrology from the amplified surface afterwards, so small movements are simply
absorbed.

### 5. Polish

One short fixed pass making the surface drainage-coherent: rills that actually
converge, no spurious pits. **Not a texture source** — texture is the
dictionary's job. This is what remains of the erosion stage, and it is a much
better-defined job than "texture that must not restructure."

## Biome expression

Entirely through **which dictionary buckets are populated and from which
exemplar pool**. There is no biome branch — the same reconstruction runs for
all six, reading different fitted data.

| Biome | Amplification character |
|---|---|
| **Piedmont** | Spur-and-hollow interfluves, the classic ridge-and-ravine texture. Mid amplitude. The reference case, and the anchor the dictionary is validated against first. |
| **Great Plains** | Very low amplitude, but **not smooth** — the caprock breaks into small scarps and benches, which is texture with structure. The biome most likely to be wrongly rendered as "flat plus noise". |
| **River Valley** | Two sharply different regimes in one course: scroll-and-swale on the floodplain, spur-and-hollow on the valley sides. The strongest test that conditioning actually switches behaviour within a single tile. |
| **Sandhills** | Dune flanks and blowouts, strongly oriented to `wind_azimuth_rad`. **Highest amplitude of the six**, and the case where `aspect_rel_grain` matters most. |
| **Heathland** | Low, hummocky, weakly oriented — `grain_strength` is low enough that orientation barely reads. Nearly isotropic conditioning; a good check that the dictionary does not force anisotropy where there is none. |
| **Hill Country** | Stepped: benches and risers with terracettes on the treads. The texture *is* the stratigraphy showing through, so the conditioning must pick up `tpi` strongly. |

## Determinism & budget

**900 ms** at the 2 m rung — the slot vacated by the old erosion finisher
([../02-performance-budget.md](../02-performance-budget.md)). Reconstruction is
a basis multiply per patch; with ~32×32 patches at 50% overlap over 1501²,
that is on the order of 300 M multiply-adds. **Comparable to the erosion pass
it replaces, but unmeasured — verify early**, because if it overruns, the
recourse is fewer basis components, and that trades directly against realism.

Determinism requirements, all load-bearing:

- **Position-seeded, never sequence-seeded.** Test by permuting patch
  processing order and asserting identical output.
- Coefficient draws use fixed counts — no rejection, no data-dependent
  iteration ([../01-conventions.md](../01-conventions.md)).
- The dictionary asset is content-hashed and its fingerprint recorded in the
  artifact; a mismatch fails at load, loudly.
- The polish pass has a fixed iteration count. No convergence criterion.

## Calibration †

S3 is the most calibration-dependent stage in the pipeline — it *is* fitted
data.

### Building the dictionary (offline)

1. **Decompose** each clean corpus tile into `lowpass` + `residual`
   (`s2 = s1 − lowpass`, already implemented in
   `tools/dtm_metrics/dtm_metrics/surfaces.py`, whose docstring says outright:
   *"the residual is what the noise layer must reproduce"*).
2. **Extract the skeleton** of each real tile so its residual can be
   conditioned the same way the generator will condition its own —
   `dtm_primitives/network.py`, `ridgepipe.py`, `geomorphons.py`. Per-tile
   vector skeletons already exist on disk in `out/extract/*.regions.json`.
3. **Sample patches** with their conditioning vectors; bucket the conditioning
   space.
4. **Fit a basis per bucket** (PCA / sparse coding) plus the coefficient
   distribution.
5. **Bake** to a compact deterministic asset. Target **< 15 MB total**:
   ~32×32 patches, ~32 components, i16-quantized, zstd.

### Metrics

The **discriminants** — the metrics that actually separate archetypes, per
[../calibration/metric-battery.md](../calibration/metric-battery.md):

| Dial / property | Metric | Note |
|---|---|---|
| Residual amplitude | `variogram_sill`, `rms_roughness_*` | amplitude family |
| Texture wavelength | `variogram_range`, `spectral_slope_beta` | **β is the primary roughness invariant** (METRICS.md) |
| Orientation | `anisotropy_ratio`, `anisotropy_orientation_deg` | must track `grain_axis_rad` |
| Hillslope form | curvature family, `tpi_landform_fractions` | the spur/hollow signature |
| Seam artifacts | `variogram_range`, radial PSD | tiling shows up as spurious periodicity |

### Corpus

**Two sources**, weighted separately:

- **Natural tiles** — the primary source. ~30 clean tiles per biome from the
  campaign ([../calibration/lidar-pipeline.md](../calibration/lidar-pipeline.md)).
- **Real golf-course grids** — 64 on disk (1500² @ 2 m). Real played ground,
  and therefore real golf texture. **Keep them weighted separately**: they are
  graded, not natural, and blending them in unweighted would teach the
  generator to reproduce earthmoving as terrain.

**Blocker:** the corpus today is 38 clean tiles keyed to the *retired*
archetypes. The campaign is a prerequisite. Note also that
`tools/metrics` and `tools/dtm_metrics` are currently non-runnable — both read
`tools/parkland_atlas/out/cache`, absent from this branch.

## Future-biome seams

None structural. A pack adds an exemplar pool and a fitted dictionary; the
reconstruction code is unchanged. This is the strongest argument for the
dictionary over a parametric detail layer: **a new biome is a data collection
job, not a modelling job.**

## Open questions

1. **Patch size and basis rank.** 32×32 @ 2 m = 64 m square with ~32
   components is the working proposal, sized to the < 15 MB budget. Both are
   calibration outcomes, not guesses — fit and measure. **Blocks the asset
   format.**
2. **Bucket count and boundaries.** Too few and conditioning does nothing; too
   many and each bucket is fitted from too few patches. The corpus is the
   constraint: 38 tiles today, ~180 after the campaign.
3. **Single-scale or pyramid?** One scale plus a spectral fill for the finest
   detail is cheaper; a 2–3 level pyramid is more faithful. Start single-scale
   and let β decide.
4. **How is the exemplar pool drawn?** Per-course selection of 2–3 tiles from
   the biome's pool gives variety grounded in real places, but a small pool
   risks visible repetition across courses. Needs a number.
5. **Does the taper leave a visible band?** A corridor of suppressed texture
   beside every channel would be its own artifact. May need the taper to
   substitute channel-appropriate texture rather than reduce amplitude.
6. **Course-grid weighting.** How much do built grids contribute before they
   start teaching earthmoving? Suspect low, and only in fairway-scale buckets.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical output, cross-platform.
- [ ] **Position-seeded**: permuting patch processing order produces
      bit-identical output. Test directly.
- [ ] Fixed draw counts and a fixed polish iteration count, asserted. No
      convergence criterion anywhere in the crate.
- [ ] Dictionary fingerprint mismatch fails at load with a specific error.
- [ ] **No biome branch in the crate** — grep for biome names, expect zero hits.
- [ ] **The taper holds**: channel centrelines and divides move less than
      tolerance between the base surface and the amplified surface, all six
      biomes, 1 000 seeds.
- [ ] **Discriminants land inside the biome's target band** on generated
      output: `spectral_slope_beta`, `anisotropy_ratio`, curvature family,
      `tpi_landform_fractions`.
- [ ] **Anisotropy tracks the grain**: `anisotropy_orientation_deg` follows
      `grain_axis_rad` within tolerance across seeds.
- [ ] **No tiling artifacts**: radial PSD shows no spurious peak at the patch
      pitch or its harmonics.
- [ ] **Conditioning demonstrably works**: within a single River Valley course,
      floodplain and valley-side cells have measurably different residual
      statistics.
- [ ] Heathland produces near-isotropic texture — the dictionary does not force
      anisotropy where the biome has none.
- [ ] Held-out validation: energy distance versus real tiles beats the v1
      baseline of 2.35.
- [ ] Golden output for seed 1, piedmont and heathland.
- [ ] Budget: < 900 ms at 2 m.
