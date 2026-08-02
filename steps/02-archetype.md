# Step 02 — Archetype Selection & Parameter Sampling

**Status:** built (claude, 2026-07-26)

## Purpose

Turn the seed into a fully-specified course recipe: pick one archetype
(weighted draw, or forced) and sample every generation parameter from that
archetype's prior. After this step, no downstream step makes an archetype
decision — they only read parameters (plus the one `hydrology_mode` switch).

## Position

- Upstream: step 01 (`RunIdentity`).
- Downstream: every step reads its own knobs from the params map; the
  archetype id rides to the end (it is the art layer's key).

## Contract

Implementation: [`crates/course-spec/`](../crates/course-spec/).

Input: `RunIdentity`, plus optional `SpecOverrides` (lab/tooling use): a
forced archetype and/or per-knob value overrides.

Output — `CourseSpec`, artifact `spec.json` (compact canonical JSON, field
order as declared, no trailing newline — byte-identical across runs):

```
CourseSpec {
  identity: RunIdentity,          // embedded verbatim (traceability)
  archetype: sandhills | piedmont | florida_lowland | glacial_moraine | mountain_bench,
  hydrology_mode: fluvial | infiltrated | water_table | kettle | alpine,
                                  // fixed per archetype — the ONE mode switch
  hole_count: 9,                  // fixed contract constants, typed
  par_sequence: [4,4,3,5,4,3,4,5,4],   // (not prior data: sums to 36 by contract)
  prior_version: string,          // provenance of the prior file used
  prior_fingerprint: string,      // blake3 hex of the prior file's raw bytes
  params: { "<section>.<knob>": f64, ... },   // the sampled parameter map
  overrides: { forced_archetype, params },    // recorded verbatim (may be empty)
}
```

**Params are data, not schema.** `params` is a flat map keyed
`"<section>.<knob>"` (sections so far: `landform`, `noise`, `hydro`,
`cover`, `gate`, `earthworks`); the knob set is whatever the committed prior
file declares for the selected archetype. Adding/removing a knob, or
re-fitting values, is a prior-file edit + explicit golden re-bless — never a
schema change here, so future steps evolve their knob sets without touching
this module. Archetypes MAY declare different knob sets (an archetype can
carry knobs no other archetype has); consumers fail loudly on a knob their
archetype's prior does not declare.

Prior file: `crates/course-spec/data/archetype_priors.json` — per archetype,
a selection `weight` plus an 11-point quantile table per knob:

```
{ "prior_version": "...",
  "archetypes": {
    "<archetype>": { "weight": f64, "params": { "<section>.<knob>": { "q": [11 values] } } } } }
```

- **Selection**: when not forced, one uniform draw from `arch/select/v1`
  against the weights normalized over all archetypes, walking the
  code-declared archetype order. Weights are relative — a future rare biome
  just gets a small weight; weight 0 = never drawn (still forceable). When
  forced, `arch/select/v1` is not opened at all (streams are domain-split,
  so params are unaffected; forcing the natural pick is a byte-level no-op
  on params).
- **Sampling**: one uniform draw from `arch/params/v1` per knob, in sorted
  knob-key order, mapped through the knob's piecewise-linear inverse CDF.
  Independent draws initially; joint copula when the fitted prior lands.
  Fixed non-sampled values (the `gate.*` thresholds) are degenerate tables
  (all 11 values equal): one schema, one draw per knob always, so promoting
  a constant to a distribution never shifts another knob's draw.
- **Overrides** are applied after all sampling (the draw transcript is
  override-independent) and recorded verbatim in the artifact.

The prior file's content hash (blake3 of the raw bytes) is embedded in every
`spec.json` and pinned in goldens, so any prior edit is an explicit re-bless;
loading a spec sampled from a different prior fails loudly. Loading also
re-validates the params map against the committed prior — the key set must
equal the archetype's declared knob set, every override key must be a
declared knob, and every non-overridden value must lie within its table's
range (overridden knobs are exempt: they are recorded verbatim, not
sampled). A hand-edited params map cannot enter the pipeline. The committed
file is `campaign-pilot-3`: 47 `landform.*` tables fitted from the step-03
data campaign's 27-tile exemplar pilot (`tools/macro_campaign`). The rest
stay hand-authored provisional, each for a recorded reason — a design lever
(the core relief cap), an archetype-identity zero, too few samples, or an
estimator that measurement showed cannot inform the knob (gating policy and
evidence: the campaign README + `fit_knobs.QUARANTINED`).

## Per-archetype behavior

Entirely expressed by the prior tables + the `hydrology_mode` switch.
Directional anchors (test-pinned, median-based since `campaign-pilot-1` so
re-fits with heavier tails don't break them): sandhills infiltration ≈ 1,
dune wavelength 150–400 m, densest basin field; florida the flattest
archetype (med relief < 20 m) with water table ≤ ~1.5 m; mountain med
relief 90–160 m with 2–5 benches; moraine kettled (med basin_count 6–30)
with depression_keep ≈ 1; piedmont the fluvial mid-point.

## Hard requirements

1. Every knob's sampled value lies inside its archetype's table range;
   quantile tables are monotone (validated on load). Enforced at generation
   AND at the `spec.json` load boundary (knob-set equality + range check on
   non-overridden values). (Tested:
   `sampled_values_in_table_range_many_seeds`, prior rejection tests, the
   `from_json_rejects_*` param suite.)
2. `par_sequence` sums to 36 with `hole_count = 9`. (Typed constants;
   tampered artifacts refuse to load.)
3. Overrides are recorded verbatim in the artifact. (Tested:
   `override_applied_and_recorded_verbatim`.)
4. Deterministic: same `RunIdentity` ⇒ identical `CourseSpec` — byte-equal
   `spec.json`. (Tested: `determinism_double_run`,
   `determinism_across_reroll_chain`, `golden_spec_seed_1`.)

Golden values (bless only on an intentional contract event, together):
`builtin_fingerprint_golden` (prior bytes), `golden_spec_seed_1`
(tests/golden_spec_seed_1.json — the full artifact for seed 1). Re-bless
helper: `cargo run -p course-spec --example bless_golden >
crates/course-spec/tests/golden_spec_seed_1.json`.

Re-bless log:
- 2026-07-26 (`placeholder-2`): +12 `landform.*` knobs for step 03's
  planner (`valley_halfwidth_m`, `valley_wall_grade`, `valley_fall_grad`,
  `meander_intensity`, `meander_wavelength_mult`, `ridge_len_m`,
  `bench_tread_w_m`, `bench_scarp_h_m`, `basin_spacing_m`, `basin_depth_m`,
  `basin_radius_m`, `blowout_eccentricity`) with provisional hand-authored
  quantiles (anchored to terrain-v2's fitted landform prior + presets);
  inapplicable archetypes carry near-degenerate tables. The step-03 data
  campaign (`tools/macro_campaign`) refits VALUES only — never the schema.
- 2026-07-27 (`campaign-pilot-1`): 37 `landform.*` tables fitted from the
  27-tile exemplar pilot (gated: design knobs, quarantined estimators,
  identity-zero families keep provisional; small-n winsorize p10–p90 —
  policy encoded in `tools/macro_campaign/macro_campaign/fit_knobs.py`).
  `placeholder_anchors` moved to median-based assertions with fitted
  reality (florida med relief 13.6 m from FL lake-district tiles). Applying
  the fitted relief also forced the course-macro `MACRO_VERSION 2`
  re-anchoring fix + golden re-bless (see steps/03).
- 2026-07-28 (`campaign-pilot-2`): same corpus, repaired estimators — 50
  tables fitted (valley fall and major-ridge counting left quarantine;
  dune wavelength and both meander knobs are quarantined *with evidence*,
  see `fit_knobs.QUARANTINED`). Adds the `landform.ridge_crest_hw_m` knob
  (measured 7.8 m piedmont → 92.5 m sandhills) that step 03's ridge shaping
  reads, and an optional `fit_provenance` block recording how many tiles the
  QA cull removed plus that cull's digest — so a prior version is traceable
  to the exact corpus it came from.
- 2026-07-30 (`campaign-pilot-3`): the CORPUS was rebuilt, not just refitted.
  A tile-lab QA review found the exemplar tiles were 5-64% developed and
  that three detectors were measuring in the wrong place (steps/03). Centers
  moved onto protected land, an OSM screen auto-culls built-up tiles, and an
  archetype-MEMBERSHIP cull drops tiles that lack the defining feature
  (`macro_campaign/character.py`). 47 tables fitted. The moraine
  `basin_count` anchor widened to 10-60: the old 6-30 band came from farmed
  Kettle Moraine country at ~12 kettles/tile, while genuinely pitted
  protected outwash measures ~37. Every archetype median shifted, so the
  course-macro goldens re-blessed with it (no planner change).

Note: `serde_json` is used with its `float_roundtrip` feature (workspace
Cargo.toml) — without it, f64 parsing is inexact in the last ULP and
artifact JSON would not round-trip byte-identically.

## References

- `archetype-pipeline:course-contracts/src/{archetype,spec}.rs` +
  `data/archetype_priors.json` — the prior art this was adapted from; its
  typed `ParamPack` was replaced by the data-driven params map, its priors
  file seeded the placeholder (with `weight` added and `gate.*` flattened
  to degenerate tables).
- `terrain-v2:tools/landform_prior/` — quantile-table + copula prior fitting
  pattern for the fitted prior to come.

## Open questions

- ~~Archetype draw weights: uniform, or player/market-weighted?~~ Resolved
  2026-07-26: relative per-archetype `weight` in the prior file, normalized
  at draw time; all equal (1.0) for now. A future rare biome gets a small
  weight; a weight edit is a prior edit (fingerprinted, re-blessed).
- Which knobs are correlated strongly enough to need the copula in v1?
