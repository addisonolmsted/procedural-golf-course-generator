# Stage S0 — Archetype & Site Draw †

**Status:** unclaimed
**Crate:** [`crates/course-spec/`](../../crates/course-spec/) (slot retained
from v1; contents are pre-v2 — see [MIGRATION.md](../../MIGRATION.md))

## Purpose

Turn a seed into a complete configuration and nothing else. S0 draws the biome,
then a **categorical structural class**, then that biome's continuous site
descriptors from **certified envelopes** — regions of parameter space already
proven offline to produce acceptable courses — and finally the **exemplar
pool** whose real terrain will supply the course's texture.

S0 is where the no-retry guarantee is cashed in. Every later stage is allowed to
assume its parameters are workable because S0 could not have drawn parameters
that were not.

It is also the pipeline's **primary variety source**, and variety here is
deliberately multiplicative rather than continuous — see below.

## Position

- **Upstream:** the seed. `course_seed::RunIdentity`.
- **Downstream:** every stage. S1–S4 read intensity dials; S3 reads the
  exemplar pool; S5 reads the scorer preset; S6–S7 read `plasticity`; S10 reads
  the aesthetic seed.
- **Contracts:** produces no guarded contract — it produces config, which
  travels alongside every artifact.
- **Streams:** `arch/select/v1` (biome draw), `arch/params/v1` (descriptors).
  Both **stable** scope.

## Inputs / Outputs

**In:** `RunIdentity` (seed, `PIPELINE_VERSION`, attempt — always 0 under v2),
plus optional caller overrides for forcing a biome or pinning a descriptor.

**Out:** `CourseSpec`, written as `spec.json`.

```
CourseSpec {
  spec_version: u32,
  seed: u64, pipeline_version: u32,
  envelope_fingerprint: [u8; 32],   // which certified envelope produced this
  biome: BiomeId,                   // one of the six
  structure_class: StructureClass,  // CATEGORICAL -- see below
  descriptors: SiteDescriptors,
  exemplars: Vec<ExemplarId>,       // 2-3 real tiles supplying texture DNA
  dials: StageDials,                // per-stage intensities, flattened
  preset: ScorerPreset,             // passed through to C2 unchanged
  overrides: Vec<Override>,         // what the caller forced, recorded verbatim
}

StructureClass {
  window: ValleyFloor | Interfluve | EscarpmentFace
        | BasinMargin | PiedmontSlope | TerraceFlight,
  provinces: 1 | 2,                 // 2 => a structural boundary exists
  boundary_kind: Option<Scarp | ValleyWall | MaterialContact>,
}

SiteDescriptors {
  relief_budget_m: f64,       // total vertical range the site may use
  density_target: f64,        // drainage density, channel-km per km^2
  plasticity: f64,            // [0,1]; see docs/00-architecture.md
  wind_azimuth_rad: f64,      // DIRECTION [0,2pi)
  water_table_m: f64,         // depth below the local datum; may be negative
  strata: Vec<Stratum>,       // may be empty
  aesthetic_seed: u64,        // S10's sub-seed
}
```

## Responsibilities

1. **Draw the biome** from adjustable relative weights, or accept a forced one.
2. **Draw the structural class** — the categorical draw. Window class and
   province configuration, weighted per biome.
3. **Draw the descriptors jointly from the biome's certified envelope**, and
   conditioned on the structural class. Not independently: relief budget and
   drainage density are correlated in real landscapes, and independent
   marginals produce parameter combinations that exist nowhere on Earth. That
   was measurably the problem with the v1 quantile-table prior, which had no
   covariance at all.
4. **Draw the exemplar pool** — 2–3 tiles from the biome's corpus whose
   residual patches supply this course's texture at
   [S3](stage-03-amplification.md). Recorded by id, so a generated course is
   traceable to real ground.
5. **Derive the stage dials and the scorer preset** from the biome record plus
   the drawn class and descriptors.
6. **Record provenance.** `envelope_fingerprint` identifies the certified
   envelope. A spec whose fingerprint does not match the loaded envelope is
   rejected at load, loudly — the interlock that stops a recalibration from
   silently changing generator output.

## Where variety comes from

S0 is the pipeline's primary variety source, and the design point is that
variety here is **multiplicative, not just continuous**. Continuous descriptors
alone give courses that differ by degree; categorical draws give courses that
differ in kind.

| Source | Kind | Contribution |
|---|---|---|
| Biome | categorical | 6 |
| **Structural class** | categorical | 6 window classes × province config |
| Descriptors | continuous, **multi-modal** | the certified envelope |
| **Exemplar pool** | combinatorial | which real tiles supply texture |
| Grain axis, base-level edge | continuous | also supplies the orientation variety that made window rotation unnecessary at [S5](stage-05-siting-substrate.md) |

Two of these are restorations. v1's framing stage had window classes and typed
province boundaries; early v2 replaced them with smooth fields and lost the
multiplicative term without arguing for it.

**Multi-modal envelopes matter for the same reason.** A certified envelope
exists to be narrow — that is its safety function — and narrowness is directly
antagonistic to variety. Certifying a *union of well-separated modes* (a
Gaussian mixture, the leaning representation in
[../calibration/envelope-certification.md](../calibration/envelope-certification.md))
resolves the tension: the parameter space stays fully certified while
remaining genuinely multi-modal.

## Biome expression

Every biome moves every descriptor; this stage *is* biome expression. See
[../biomes/](../biomes/) for the six records. Sketch:

| Biome | Relief budget | Density | Plasticity | Strata | Typical window classes |
|---|---|---|---|---|---|
| Piedmont | mid | mid | mid | thin | interfluve, piedmont slope |
| Great Plains | low | low | mid-high | present | basin margin, interfluve |
| River Valley | low-mid | mid-high | mid | present | valley floor, terrace flight |
| Sandhills | mid | ~0 | low | empty | interfluve |
| Heathland | low-mid | very low (~0.07) | low-mid | empty | basin margin |
| Hill Country | high | mid | low | thick | escarpment face, terrace flight |

Note that `density_target` still varies by biome and still must be *hit* — but
it is a **shared invariant**, not an identity marker. Real archetypes all sit
at `dist_to_channel_p50` 104–120 m regardless of relief, so drainage spacing
does not discriminate. Identity comes from S3's texture. See
[../calibration/metric-battery.md](../calibration/metric-battery.md).

## Determinism & budget

**1 ms** ([../02-performance-budget.md](../02-performance-budget.md)). Pure
sampling; if this is measurable, something is wrong.

Draw counts must be fixed per biome — never data-dependent. Rejection sampling
against the envelope boundary is **forbidden** (it is a retry in miniature and
it forks the RNG). Use an invertible transform into the envelope instead, so
every draw lands inside by construction.

## Calibration †

S0 does not tune dials; it consumes the output of everything that does.

- **What calibrates it:** the certified envelopes from
  [../calibration/envelope-certification.md](../calibration/envelope-certification.md).
  The envelope *is* S0's parameter space.
- **Metrics:** all of them, indirectly. S0's correctness criterion is that
  courses drawn from its envelope score inside the biome's target vector in
  [../calibration/targets.md](../calibration/targets.md).
- **Structural-class weights** are fitted too: how often a real landscape of
  this biome presents as a valley floor versus an interfluve is measurable from
  the corpus, and should be, rather than hand-assigned.
- **Exemplar pool size** is a calibration outcome. Too small and courses in a
  biome share visible texture; too large and each course averages toward the
  biome mean.
- **Tile data required:** none directly, but S0 cannot be implemented in final
  form until envelopes exist for all six biomes, and three of the six have no
  corpus at all. **This is S0's blocker.** Build against a provisional
  hand-authored envelope with the real schema, and swap the data in when
  calibration lands.

## Future-biome seams

Adding a biome is a data addition: a new record and a new certified envelope.
`BiomeId` gains a variant and no stage changes. Packs also group biomes for
weighting, so `BiomeId` should carry its pack.

## Open questions

1. **Envelope representation.** A Gaussian mixture in transformed space is the
   presumed shape — it gives multi-modality and rejection-free sampling in one,
   which is exactly what S0 needs. v1's `landform_prior` fitter used a Gaussian
   copula over reach residuals and is the closest prior art. Alternatives: a
   convex hull with rejection-free interior sampling, or a normalizing flow.
   Settle in
   [../calibration/envelope-certification.md](../calibration/envelope-certification.md)
   before S0 is implemented, because the sampling code depends on it.
2. **Is the envelope per biome, or per (biome × structure class)?** Per-pair is
   more faithful — an escarpment-face piedmont genuinely occupies different
   parameter space than an interfluve one — but it multiplies the corpus needed
   to certify by six. Leaning per-biome with class-conditioned means.
3. **Are `dials` derived or drawn?** Deriving them from descriptors is fewer
   free parameters and easier to certify; drawing them is more expressive.
   Leaning derived.
4. **How many exemplars per course, and how are they blended?** 2–3 is the
   working proposal. Blending weights are themselves a draw.
5. **Forced-biome overrides and envelopes.** If a caller forces a descriptor
   outside the certified envelope, S0 should presumably proceed and mark the
   spec uncertified rather than refuse. Confirm.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical `spec.json`, on macOS and Linux.
- [ ] All six biomes produce specs whose descriptors lie inside their declared
      envelope, over 10 000 seeds, with **zero** rejections and **zero**
      data-dependent draw counts (assert the draw count directly).
- [ ] Descriptors are jointly distributed: the sampled correlation matrix
      matches the envelope's to within tolerance over 10 000 seeds.
- [ ] **Multi-modality survives sampling**: if the envelope has separated modes,
      the sampled population is measurably multi-modal, not collapsed to the
      mean.
- [ ] **Structural class is drawn and reaches S1**: all six window classes
      appear at their fitted rates over 10 000 seeds, and each produces a valid
      downstream run.
- [ ] Exemplar ids are drawn, recorded, and resolve against the shipped
      dictionary; an unknown id fails at load with a specific error.
- [ ] A spec whose `envelope_fingerprint` does not match the loaded envelope is
      rejected at load with a specific error.
- [ ] Forced biome and forced descriptors are recorded verbatim in `overrides`.
- [ ] Empty `strata` is produced for Sandhills and Heathland and accepted
      downstream.
- [ ] Golden spec for seed 1, both golden biomes (piedmont, heathland).
- [ ] Budget: < 1 ms.
