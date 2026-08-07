# Metric Battery

The 22 metrics that define what a biome *is*, and which stage dial each one
calibrates.

## The purity rule

Every metric is a **pure function of `(height, cell_size, mask)`**. No I/O, no
geographic types, no knowledge of whether it is looking at a lidar tile or a
generated course.

This is the load-bearing property of the whole calibration architecture. Because
the function is pure, the *same code* measures a 3DEP tile and an
[S11](../stages/stage-11-validation.md) output, so real and synthetic terrain
land in one feature space and can be compared directly. Two implementations —
one in the ingestion path, one in the validation path — would drift, and the
drift would be indistinguishable from a generator improvement.

The rule has a concrete consequence for
[S11](../stages/stage-11-validation.md): it must call the same battery, not a
reimplementation. Test both paths against a shared analytic surface.

## Prior art

- **`tools/metrics/metrics/core.py`** (815 lines) — the pure kernels, including
  a pure-numpy priority-flood and D8. Already correct, already tested against
  analytic surfaces (plane, tilt, cone, sinusoid, fBm) in
  `tools/metrics/tests/test_metrics.py`. **Reuse; do not rewrite.**
- **`tools/metrics/metrics/features.py`** — `compute()`, the single assembly
  point. 49 scalars plus 4 distribution vectors on shared axes.
- **`tools/metrics/METRICS.md`** — per-metric definitions, units, detrend
  policy, resolution guards.
- **`tools/dtm_metrics/dtm_metrics/gates.py`** (241 lines) — the G1–G6
  admission gates. See below.

The v2 battery of 22 is a **subset of the existing 49, plus 4 new metrics**. The
v1 calibration harness used 37 and matched on 32; the reduction to 22 is a
deliberate narrowing to metrics that pass admission and have a named consumer.

## Admission gates

Not every metric that can be computed is a legitimate fit target.
`tools/dtm_metrics/dtm_metrics/gates.py` already implements the classification
and it should be run before any metric is trusted:

| Gate | Question |
|---|---|
| G1 | Injection robustness — does it respond to the signal it claims to measure? |
| G2 | Mask stability — does it survive a perturbed validity mask? |
| G3 | Split-half ICC — is it reliable within a single tile? |
| G4 | θ-share — does generator parameter variation move it more than seed variation? |
| G5 | Redundancy clustering — is it distinct from its neighbours? |
| G6 | Emulator identifiability — can it be inverted to a parameter? |

Output is a classification per metric:

- **`fit_target`** — may be used to fit a dial.
- **`validation`** — measured and reported, never fitted. Fitting to a metric
  that fails G4 or G6 optimizes noise.
- **`diagnostic`** — for humans, not for the loop.

**A metric's family (process/amplitude) and its gate class are independent
axes.** An amplitude metric can be a fit target; a process metric can fail
admission and be validation-only.

## Process versus amplitude

The central distinction in this project's calibration, and the thing most often
gotten wrong.

**Process metrics are identity.** They describe *how the landscape is
organized* — drainage density, slope–area scaling, network connectivity,
landform proportions. They are what makes heathland heathland and they are
nearly constant within a biome. They **must match**, pointwise, within a narrow
band. A generated heathland whose drainage density is piedmont's is not a
heathland with unusual parameters; it is a failure.

**Amplitude metrics are per-site continuous descriptors.** Local relief,
elevation range, variogram sill. They vary widely *within* a biome — real
piedmont sites range from 20 m to 90 m of local relief and all of them are
piedmont. They are matched **in distribution**, not pointwise, and S0's
descriptors draw from that distribution.

Getting this backwards produces the two classic failures: matching amplitude
pointwise gives every course in a biome the same relief, and matching process
in distribution gives biomes that blur into each other.

## The 22

| # | Metric | Family | Calibrates | Stage |
|---|---|---|---|---|
| **Relief & hypsometry** ||||
| 1 | `local_relief_p50` | amplitude | relief budget | [S0](../stages/stage-00-archetype-draw.md), [S1](../stages/stage-01-macro-primitives.md) |
| 2 | `elevation_range` | amplitude | relief budget | S0 |
| 3 | `hypsometric_integral` | process | catena form | [S2](../stages/stage-02-skeleton-kernel.md) |
| 4 | `hypsometric_bimodality` ★ | process | floor/step separation | S2 |
| **Slope** ||||
| 5 | `slope_p50` | amplitude | relief amplitude | S1, S2 |
| 6 | `slope_p90` | amplitude | steepness tail | S2 |
| 7 | `slope_bimodality` ★ | process | caprock/scarp structure | S2 stratigraphy |
| 8 | `slope_entropy` | process | slope organization | S2 |
| **Drainage & network** ||||
| 9 | `drainage_density` | **process, identity** | **tributary spacing** | **S2** |
| 10 | `network_connectivity` ★ | **process, identity** | **drainage integration dial** | **S2** |
| 11 | `slope_area_theta` | process | **profile-law exponent θ** | S2 catena |
| 12 | `dist_to_channel_p50` | process | flow-distance scale | S2 |
| 13 | `flow_accum_gini` | process | network concentration | S2 |
| **Spectrum & texture** ||||
| 14 | `variogram_range` | process | detail wavelength band | [S9](../stages/stage-09-micro-repass.md) |
| 15 | `variogram_sill` | amplitude | detail amplitude | S9 |
| 16 | `spectral_break_wavelength` | process | scale separation | S2 aeolian, S9 |
| 17 | `short_lag_roughness` | amplitude | erosion texture | [S4](../stages/stage-04-finishers.md) |
| **Anisotropy** ||||
| 18 | `anisotropy_ratio` | process | grain strength | S1, S9 |
| 19 | `anisotropy_axis_alignment` | process | orientation coherence | S1 → S9 (the thread) |
| **Landform organization** ||||
| 20 | `tpi_landform_fractions` | process | flat/riser/tread proportions | S2 stratigraphy |
| 21 | `bench_step_count_per_relief` ★ | **process, identity** | bench step height | S2 stratigraphy |
| 22 | `terrace_step_spacing` ★ | **process, identity** | terrace flight | S2 trunk-river |

★ = **not in the existing battery; needs implementing and gating.**

### The four new metrics

Each exists because a v2 biome's identity cannot be expressed without it, and
none of them is a standard terrain metric. All four need implementing in
`core.py` and putting through G1–G6 before use.

- **`network_connectivity`** — fraction of channel length that reaches base
  level. Near zero for [Heathland](../biomes/heathland.md), near one for
  everything else. **This is the single number that defines heathland**, and
  without it the biome cannot be fit at all.
- **`hypsometric_bimodality`** and **`slope_bimodality`** —
  [Great Plains](../biomes/great-plains.md) and
  [River Valley](../biomes/river-valley.md) both have genuinely bimodal
  distributions (surface/scarp, floor/terrace). A unimodal fit to a bimodal
  truth **scores acceptably and is wrong**, which is the worst kind of
  calibration failure because nothing flags it.
- **`bench_step_count_per_relief`** — separates
  [Hill Country](../biomes/hill-country.md) from any other steep terrain. Its
  slopes are stepped, not smooth, and no existing metric distinguishes those.
- **`terrace_step_spacing`** — the River Valley staircase.

## Distribution vectors

Alongside the 22 scalars, four distributions on shared axes, used for
distribution-matching rather than pointwise comparison: `slope_hist`,
`radial_psd`, `variogram`, `hypsometric_curve`. These come from the existing
`features.py` unchanged.

## Resolution guards

Every metric declares the resolutions at which it is valid.
`variogram_range` measured on a 2 m grid cannot resolve structure below ~6 m;
`drainage_density` depends on the channel-extraction threshold, which depends
on cell size. **Comparing a metric across resolutions without a guard is the
most common way to produce a confident wrong number.** The guards exist in
`METRICS.md`; carry them forward.

Since real tiles and generated courses are both at 2 m
([lidar-pipeline.md](lidar-pipeline.md)), this mostly does not bite — but it
will the moment someone measures a preview-resolution run.

## Open questions

1. **Are the four new metrics admissible?** They may fail G4 (θ-share) or G6
   (identifiability). If `network_connectivity` fails, heathland cannot be
   fitted and the biome definition needs revisiting rather than the metric.
2. **Is 22 the right number?** Narrowed from 37 for consumer-directedness. If
   the emulator cannot identify a dial from the reduced set, metrics come back.
3. **A bimodality measure that is not fragile.** Dip tests and mixture fits are
   both sensitive to bandwidth. Needs a robust choice before implementation.
4. **Does the battery need a golf-specific metric?** Everything here is
   geomorphology. Nothing measures whether terrain is *good for golf* — that is
   [S11](../stages/stage-11-validation.md)'s playability sim, which is not part
   of this battery and cannot be measured on a lidar tile. Worth stating that
   the gap is deliberate.
