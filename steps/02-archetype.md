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
file is a hand-authored placeholder until the data campaign (PLAN.md)
delivers a fitted file with the same schema.

## Per-archetype behavior

Entirely expressed by the prior tables + the `hydrology_mode` switch.
Directional anchors the placeholder encodes (test-pinned): sandhills
infiltration ≈ 1, dune wavelength 150–400 m; florida relief < 10 m and water
table ≤ ~1.5 m; mountain relief 80–180 m with 2–5 benches; moraine
basin_count 4–14 with depression_keep ≈ 1; piedmont the fluvial mid-point.

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
(tests/golden_spec_seed_1.json — the full artifact for seed 1).

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
