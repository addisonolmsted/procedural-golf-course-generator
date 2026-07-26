# Step 02 — Archetype Selection & Parameter Sampling

**Status:** unclaimed

## Purpose

Turn the seed into a fully-specified course recipe: pick one of the five
archetypes and sample every generation parameter from that archetype's
calibrated prior. After this step, no downstream step makes an archetype
decision — they only read parameters.

## Position

- Upstream: step 01 (`RunIdentity`).
- Downstream: every step reads its own section of the output; the archetype
  id rides to the end (it is the art layer's key).

## Contract

Input: `RunIdentity`, optional forced archetype, optional per-parameter
overrides (lab/tooling use).

Output — `CourseSpec`:

```
CourseSpec {
  identity: RunIdentity,
  archetype: sandhills | piedmont | florida_lowland | glacial_moraine | mountain_bench,
  hydrology_mode: fluvial | infiltrated | water_table | kettle | alpine,
                                  // fixed per archetype — the ONE mode switch
  params: {                       // one section per consuming step
    landform:   { relief_amp_m, core_relief_cap_m, tilt_grade, valley_count,
                  ridge_count, bench_count, basin_count, dune_wavelength_m },
    noise:      { base_amp_m, base_wavelength_m, octaves, ridged_mix,
                  aniso_ratio, floor_damp, slope_gain, grain_align },
    hydro:      { rainfall, infiltration, erodibility, water_table_depth_m,
                  depression_keep, channel_threshold_ha },
    cover:      { canopy_frac, sand_exposure, wetland_bias, clump_scale_m },
    gate:       { slope_lt8_min, routable_patch_ha_min, water_blockage_max,
                  crossings_max },              // fixed thresholds, not sampled
    route:      { hole_count: 9, par_sequence: [4,4,3,5,4,3,4,5,4] },  // fixed
    earthworks: { cut_fill_budget_m3, green_area_m2, tee_area_m2, feather_m },
  },
}
```

Sampling: each continuous knob has an 11-point quantile table per archetype in
a committed prior file; sample by inverse-CDF on a uniform draw from
`arch/params/v1` (independent draws initially; joint copula when the fitted
prior lands). Archetype choice (when not forced) draws from `arch/select/v1`.

The prior file is produced by the data campaign (PLAN.md); until then a
hand-authored placeholder with the same schema is used. The prior file's
content hash is folded into goldens so a prior edit is always an explicit
re-bless.

## Per-archetype behavior

Entirely expressed by the prior tables + the `hydrology_mode` switch.
Directional anchors the placeholder must encode: sandhills infiltration ≈ 1,
dune wavelength 150–400 m; florida relief < 10 m and water table ≤ ~1.5 m;
mountain relief 80–180 m with 2–5 benches; moraine basin_count 4–14 with
depression_keep ≈ 1; piedmont the fluvial mid-point.

## Hard requirements

1. Every knob's sampled value lies inside its archetype's table range;
   quantile tables are monotone (validated on load).
2. `par_sequence` sums to 36 with `hole_count = 9`.
3. Overrides are recorded verbatim in the artifact (reproducibility).
4. Deterministic: same `RunIdentity` ⇒ identical `CourseSpec`.

## References

- `archetype-pipeline:course-contracts/src/spec.rs` +
  `data/archetype_priors.json` — a working first draft of this exact
  contract (schema-final placeholder, 5×30 knobs).
- `terrain-v2:tools/landform_prior/` — quantile-table + copula prior fitting
  pattern; `terrain-v2:golf-landform/src/prior.rs` — loading/shrinkage.

## Open questions

- Archetype draw weights: uniform, or player/market-weighted?
- Which knobs are correlated strongly enough to need the copula in v1?
