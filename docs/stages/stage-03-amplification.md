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

### 2. Reconstruct — gradient-domain exemplar quilting

**The mechanism is settled by the G-SPIKE experiment (`tools/spike/`), not by
preference.** Seven variants were run; the results are binding:

- **Drawn-coefficient synthesis is falsified.** A PCA basis with independent
  coefficient draws passed 8/8 metrics while being visually fake (no connected
  structure); correlated coefficient fields reinforce the patch lattice.
  Real structure requires real patch content.
- **Naive blending fails three ways**: Hann overlap-add cancels amplitude,
  low overlap shows seams, min-error cuts in the height domain leave creases
  (any height offset at a seam is a slope discontinuity).
- **The working mechanism**: per site, select a real patch from the
  conditioning bucket (best boundary match among a seeded top-k, amplitude
  matched to the bucket's conditional expectation); compose its **gradients**
  along a min-error seam; **Poisson-integrate** back to height;
  **re-band-limit** (integration invents long-wavelength swell from seam
  inconsistencies); true the within-band spectrum with a **calibrated radial
  equalizer** (integration and band-limiting reshape it, and a scalar cannot
  fix a shape drift); then the per-band closer sets amplitude.

Two levels, disjoint bands: **mid** 64–400 m on the 8 m grid (256 m patches)
and **fine** < 64 m at 2 m (64 m patches) — the pyramid question is closed;
single-scale cannot span the residual's 181 m variogram range. Selection and
seam decisions are **position-seeded** and raster-ordered — deterministic and
platform-stable.

**Known artifact, with owners:** the spike's output ran slightly *grainy*
(curvature ~1.5× hot, β marginally shallow — reviewer-confirmed). Levers, in
order: amplitude *matching* instead of a candidate std floor; equalizer
high-frequency rolloff; a cover/smoothness conditioning axis; sub-patch fill
discipline. **Do not tune these against the 6-tile spike corpus** — part of
real-tile smoothness is classifier artifact, and the despeckle sensitivity
analysis ([../calibration/lidar-pipeline.md](../calibration/lidar-pipeline.md))
establishes the true target first.

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

### 5. Fill the sub-patch band

Below the dictionary's finest resolved wavelength (~patch pitch ÷ components),
fill with **noise shaped to the measured PSD of the real residual** —
`course_world::noise`, band-limited, position-seeded, amplitude from the
variogram targets. This is the one place procedural noise touches the terrain,
and the rule that keeps it honest: *if a metric can tell the fill from the real
thing at that band, the fill is not allowed at that band.* See
[../00-architecture.md](../00-architecture.md), "Where noise lives".

### 6. Close the relief budget

S1 spends part of `relief_budget_m`, S2 spends more, and the residual adds the
rest — and **S3 is the closer**: it measures what S1 + S2 actually spent and
scales its total residual amplitude so the finished surface lands on the drawn
budget. Without a closer, three stages drawing independently from one budget
systematically overshoot. (The taper and the certified amplitude ceiling still
bound the scaling; the budget closes *within* them.)

### 7. Polish

One short fixed pass making the surface drainage-coherent: rills that actually
converge, no spurious pits. **Not a texture source** — texture is the
dictionary's job. This is what remains of the erosion stage, and it is a much
better-defined job than "texture that must not restructure."

**The pit boundary is decided, not discovered.** S2's closed-basin embryos are
recorded in the skeleton artifact, so polish knows exactly which depressions
are intended: it removes only pits that are *not* embryo-derived **and** are
below the embryo scale. A heathland kettle survives polish because polish can
look it up — not because a heuristic guessed right. (S7's repair uses the same
lookup via S4's basin inventory.)

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
3. **Sample patches** with their conditioning vectors — **mask-aware**: the
   spike showed the real fine band carries creek cuts, field boundaries, and
   road traces; extraction must respect the develop/water masks or the
   dictionary learns them as fabric.
4. **Curate the patch library per bucket** (the dictionary ships real
   patches, not a fitted basis) plus the per-bucket amplitude statistics and
   the band's radial-PSD target for the equalizer.
5. **Bake** to a compact deterministic asset, i16-quantized + zstd. The
   original **< 15 MB** target assumed a PCA basis; a patch library is
   larger — **re-estimate the budget in Phase F**, with patch-count
   discipline (per-bucket caps, dedup) as the lever.

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

**De-risk first: the dictionary spike.** The full campaign is expensive and
the dictionary is the least-proven component — do not let it be validated
last. Before fetching anything, fit a crude dictionary from the **6 clean
piedmont tiles already on disk**, reconstruct over a synthetic base, and score
`spectral_slope_beta` and the curvature family against held-out real. A week of
work that answers "can this work at all" before the campaign is paid for. If
the spike fails, the fallback conversation happens *before* 300 tiles are
fetched, not after.

**Blocker:** the corpus today is 38 clean tiles keyed to the *retired*
archetypes. The full campaign is a prerequisite for shipping quality (the spike
is not shipping quality). Note also that `tools/metrics` and `tools/dtm_metrics`
are currently non-runnable — both read `tools/parkland_atlas/out/cache`, absent
from this branch.

## Future-biome seams

None structural. A pack adds an exemplar pool and a fitted dictionary; the
reconstruction code is unchanged. This is the strongest argument for the
dictionary over a parametric detail layer: **a new biome is a data collection
job, not a modelling job.**

## Open questions

1. **Asset size for a patch library.** The < 15 MB target assumed a basis;
   real patches are bigger. Per-bucket caps + dedup + i16 + zstd are the
   levers. **Blocks the asset format**; re-estimate in Phase F.
2. **Bucket count and boundaries.** Too few and conditioning does nothing; too
   many and each bucket draws from too few patches (and too few distinct
   tiles). The corpus is the constraint: 38 tiles today, ~180 after the
   campaign.
3. ~~Single-scale or pyramid?~~ **Resolved by the spike: two levels** (mid
   64–400 m @ 8 m, fine < 64 m @ 2 m). Single-scale cannot span the
   residual's measured 181 m variogram range with 64 m patches.
4. **How is the exemplar pool drawn?** Per-course selection of 2–3 tiles from
   the biome's pool gives variety grounded in real places, but a small pool
   risks visible repetition across courses. Needs a number.
5. **Does the taper leave a visible band?** A corridor of suppressed texture
   beside every channel would be its own artifact. May need the taper to
   substitute channel-appropriate texture rather than reduce amplitude.
6. **Course-grid weighting.** How much do built grids contribute before they
   start teaching earthmoving? Suspect low, and only in fairway-scale buckets.

**Exit gate: G-TERRAIN** in
[../03-success-indicators.md](../03-success-indicators.md) — discriminants in
band, blind A/B ≤ 65%, name-the-biome ≥ 80%, dispersion ratio 0.7–1.3. The
criteria below are the correctness axis of that gate; the spike gate G-SPIKE
precedes all of it.

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
