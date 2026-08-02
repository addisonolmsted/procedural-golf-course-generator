# Stage 00 — Seed & Archetype

**Status:** built (crates/course-seed + crates/course-spec; rev pending — see
"Planned rev" below)

## Purpose

Turn one master `u64` seed into named, domain-split randomness and a fully
specified course recipe: the archetype (art kit + parameter distribution) and
the parameter vector θ drawn from that archetype's distribution. Everything
downstream is a pure function of this stage's outputs.

## Position

- Upstream: nothing (pipeline entry).
- Downstream: every stage opens its registered streams via `RunIdentity`;
  every stage reads its knobs from `CourseSpec.params`; the archetype id
  rides to Stage 13 (it keys the art kit).

## Contract (as built)

Implementations: [`crates/course-seed/`](../crates/course-seed/) (seed,
streams, reroll) and [`crates/course-spec/`](../crates/course-spec/)
(archetype + θ).

**`RunIdentity`** — artifact `run.json` (compact canonical JSON, integer
fields, loud load validation: unknown fields, wrong `pipeline_version`,
attempt outside `0..MAX_ATTEMPTS=8` all rejected):

```
RunIdentity { seed: u64, pipeline_version: u32, attempt: u32 }
```

Streams: `RunIdentity::stream("<name>")` → blake3-keyed ChaCha8, platform
stable (only float op is the IEEE-exact 53-bit mantissa mapping). Names must
be registered in `crates/course-seed/src/streams.rs` (mirrored, test-enforced,
in ARCHITECTURE.md). Reroll rule: attempt `a>0`'s working seed is draw `a-1`
of the master-keyed `reroll/v1` stream — the whole retry sequence replays
from the master seed alone.

**`CourseSpec`** — artifact `spec.json` (canonical bytes; load re-validates
version/attempt/hydrology/route constants, prior fingerprint, and the params
map against the committed prior — knob-set equality + range check on
non-overridden values):

```
CourseSpec {
  identity, archetype, hydrology_mode,          // mode fixed per archetype
  hole_count: 9, par_sequence: [4,4,3,5,4,3,4,5,4],
  prior_version, prior_fingerprint,             // blake3 of prior bytes
  params: { "<section>.<knob>": f64 },          // data-driven knob map
  overrides: { forced_archetype, params },      // recorded verbatim
}
```

Selection: weighted draw over per-archetype `weight` in the prior file
(`arch/select/v1`; skipped entirely when forced). Sampling today: one uniform
draw per knob in sorted key order through 11-point quantile tables
(`arch/params/v1`); overrides applied post-sampling. Prior file:
`crates/course-spec/data/archetype_priors.json`, currently `campaign-m5`
(32 `landform.*` knobs campaign-fitted for the retired step-03 generator —
dormant data pending the rev; other sections hand-authored provisional).
Goldens: fingerprint + full seed-1 artifact; re-bless via
`cargo run -p course-spec --example bless_golden`.

## Planned rev (the Stage-0 contract of the new pipeline) — TWO changes

**Rev A — two-tier stream scoping (course-seed, `PIPELINE_VERSION` → 2).**
The new reroll semantics: a Stage-8 gate fail resamples **stages 2–3 only** —
archetype, θ, and site framing must SURVIVE a reroll. Registry v2 gives every
stream a scope:

- `stable` streams (stages 0–1) — keyed off the MASTER seed: identical on
  every attempt.
- `attempt` streams (stages 2+) — keyed off the attempt seed as today.

`stream()` consults the registry for the scope. This is a rekey of stage-0/1
streams ⇒ version bump + golden re-bless, done together with the registry
rename (v2 stream names per stage, retiring `macro/place/v1` and the other
step-era names).

**Rev B — θ from a Gaussian component, covariance intact (course-spec).**
Replace independent per-knob quantile draws with: per archetype, a mixture of
Gaussian components in TRANSFORMED space. Prior schema v2 per archetype:

```
{ "weight": f64,
  "theta": {
    "transforms": { "<knob>": "identity" | "log" | "logit" },
    "components": [ { "weight": f64, "mu": [d], "chol": [d][d] } ],
    "support":    { "<knob>": [lo, hi] } } }      // post-transform clamp
```

Draw: component (one uniform), z ~ N(0,I) via Box–Muller (libm — platform
stable), θ_t = μ + L·z, inverse-transform, clamp to support. Correlated knobs
(relief ↔ wall grade ↔ drainage density) now move together — parameter-level
plausibility. Output stays `params: BTreeMap<String,f64>`: downstream
contract unchanged. Until the campaign fits real components, ship a
provisional DIAGONAL build derived from the current quantile tables
(μ = transformed median, σ from the transformed IQR / 1.349). The knob SET
itself is re-cut for the new stages (sections `framing.*`, `mask.*`,
`strokes.*`, `forcing.*`, `lem.*`, `hydro.*`, `cover.*`, `gate.*`,
`route.*`, `earthworks.*`, `micro.*`, `placement.*` as each stage doc
declares its needs).

## Per-archetype behavior

Entirely data: weights, transforms, components. The `hydrology_mode` switch
(fluvial / infiltrated / water_table / kettle / alpine) remains the one
non-parameter difference.

## Hard requirements

1. Byte-identical double-run; platform-stable; artifacts traceable
   (`RunIdentity` embedded in every manifest). (All tested today.)
2. After Rev A: stage-0/1 outputs byte-identical ACROSS attempts of one
   master seed (new test), stage-2+ streams differ per attempt.
3. After Rev B: sampled θ respects support clamps; Cholesky factors validated
   (lower-triangular, positive diagonal) on prior load; determinism goldens
   re-blessed once, deliberately.
4. Overrides recorded verbatim; forced archetype never shifts θ draws.

## References

- Current code: `crates/course-seed/`, `crates/course-spec/` (see their
  tests for the golden/bless conventions; re-bless log in git history of
  `steps/02-archetype.md` at commit 0313432).
- Campaign fitting: `tools/macro_campaign/` (`fit_knobs.py` gating policy —
  the pattern for the GMM fitter), corpus in `tools/macro_campaign/out/`.

## Open questions

- Mixture component count per archetype: start at 1 (single Gaussian) and
  let the campaign justify more?
- Which knob families are cross-stage correlated strongly enough that they
  must share one component block vs independent blocks per section?
