# Envelope Certification

How offline fitting produces the **certified sampling envelopes** that
[S0](../stages/stage-00-archetype-draw.md) draws from, and why the runtime
never runs a gate.

## Why runtime never gates

The v2 pipeline **never rejects and never retries**
([../00-architecture.md](../00-architecture.md)). A seed always produces a
course. There is no gate stage, no attempt counter in use, no reroll.

That is only safe because the rejecting happens here, offline. The bargain:

> **Search parameter space slowly, once, offline. Certify the region that
> works. Sample only from inside it at runtime.**

Three things follow, and they are the practical reasons the design is worth its
cost:

1. **Bounded runtime.** Retrying multiplies the budget by an unbounded factor.
   A pipeline that can retry has no worst case anyone has measured, and the
   sub-10s budget in
   [../02-performance-budget.md](../02-performance-budget.md) becomes a hope.
2. **No retry-forked RNG.** Conditional stream advancement is the subtlest
   determinism bug available, and removing retries removes the possibility
   entirely ([../01-conventions.md](../01-conventions.md)).
3. **Failures are actionable.** A bad course is a **calibration bug** with a
   specific fix — tighten an envelope — rather than an unlucky seed to discard
   and forget. [S11](../stages/stage-11-validation.md)'s report is what routes
   it here.

The cost is that the envelope must be genuinely trustworthy. Everything below
is in service of that.

## The recoverable harness

**A complete 11-module harness implementing exactly this loop already exists**
on the `main` branch at `tools/calibration/`. It should be recovered and
rebound rather than rewritten.

| File | Bytes | Role |
|---|---:|---|
| `CALIBRATION.md` | 8 466 | The method writeup and v1 findings. **Read first.** |
| `param_schema.py` | 10 299 | Canonical θ, transforms, the `drives` intent map, θ↔config |
| `export_sampler.py` | 6 766 | Bakes fitted p(θ) → Rust |
| `inverse.py` | 5 570 | Density-ratio weighting → p(θ), `sample_courses()` |
| `forward.py` | 5 563 | θ → batch run → metric compute |
| `emulator.py` | 5 502 | Per-metric HistGBT surrogate + SALib Sobol ST |
| `validation.py` | 5 234 | Round-trip vs held-out real (energy distance / KS / PCA) |
| `coverage.py` | 4 824 | Reachability gate: real vs generator envelope |
| `run.py` | 3 793 | Orchestrator |
| `config.yaml` | 1 432 | Sobol n=512, 4 seeds, temper 0.5, holdout 0.3, R² gate 0.5 |
| `sample.py` | 609 | Sobol/LHS design in transformed space |

Recover with `git checkout main -- tools/calibration`. **Not done in this pass**
— see [MIGRATION.md](../../MIGRATION.md).

There is also a **newer port of the same pattern already on this branch**:
`tools/dtm_metrics/dtm_metrics/sandbox/` (~740 lines) — `campaign.py`,
`emulate.py`, `coverage.py`, `identify.py`, `decide.py`, `params.py`. It
expresses the same loop against the v2 metric schema and includes two ideas
worth keeping: a deliberately **inert `dummy` parameter** that establishes the
sensitivity noise floor, and an explicit **θ-versus-seed variance split**. Use
it as the structural template; use `tools/calibration/` for the method.

Note: **no CMA-ES anywhere.** The approach is Sobol design + surrogate +
importance weighting, not direct search. Do not introduce direct optimization
without a reason — it does not produce an envelope, only a point.

## The loop

### 1. Design

Sobol (or LHS) design over θ in **transformed space** — log for scale
parameters, logit for bounded ones — so the design is uniform where it matters.
v1 used n = 512 designs × 4 seeds = 2 048 runs.

### 2. Forward

Run each θ through the pipeline headlessly and measure with the battery. This
needs a **batch entrypoint against the current crates**, and it does not exist:
v1's `forward.py` calls `xtask forward-grid`, and `xtask` is not on this
branch. Building it is `course-cli`'s first job and it is a prerequisite for
everything else here.

Multiple seeds per θ is not optional — it is what separates parameter effects
from seed noise, and it feeds the θ-versus-seed variance split.

### 3. Coverage / reachability gate

Before fitting anything, ask: **can the generator reach real terrain at all?**
Compare the real corpus's position in metric space against the convex region
the generator's outputs occupy.

v1's answer was **59% of real courses inside the generator envelope** — which
means 41% of real terrain was unreachable at any parameter setting. Fitting
harder would not have helped; the generator lacked the expressive range. This
gate is the one that catches a structurally wrong generator, and it is
therefore the most valuable single step in the loop.

**For v2 this gate is the referee on the whole realism bet.** The generator now
reaches real terrain in two stages: [S2](../stages/stage-02-skeleton-kernel.md)
covers the structural axes, [S3](../stages/stage-03-amplification.md) covers
the textural ones. Run coverage **separately on the shared invariants and on
the discriminants** — they fail for different reasons and have different fixes.

If discriminant reachability is poor, the dictionary is too small or its
conditioning too coarse: collect more, or re-bucket. If invariant reachability
is poor, the skeleton engine is wrong and no amount of texture will save it.
Conflating the two would send the effort to the wrong place, which is precisely
what v1's single 59% number could not distinguish.

### 4. Emulator and sensitivity

Fit a per-metric surrogate (HistGBT) mapping θ → metric, and run SALib Sobol to
get total-order indices. Two outputs:

- **Which dials actually drive which metrics** — validated against the `drives`
  intent map in `param_schema.py`. A dial that does not move its intended
  metric is a **HIT/SURPRISE** finding and usually means the dial is
  mis-implemented, not that the metric is wrong.
- **Which metrics are identifiable**, feeding gate G6 in
  [metric-battery.md](metric-battery.md).

Keep the **inert `dummy` parameter**: its sensitivity index is the noise floor,
and any real parameter scoring below it is indistinguishable from nothing.

Metrics failing the CV R² gate (v1 used 0.5; 34 of 37 passed) become
validation-only.

### 5. Inverse — density-ratio weighting

The step that turns "we can generate terrain" into "we generate terrain like
this biome". **Label-free**: train a calibrated logistic classifier to
distinguish real from synthetic in metric space, and use the density ratio
`r = p_real/p_synth` to weight the design points.

Practical details from v1, all worth carrying forward:

- **Temper** the weights, `w = r^0.5`. Untempered ratios collapse onto a few
  design points.
- **Clip** the top 2%.
- **Report ESS.** v1 achieved 355 of 512 — healthy. A collapsing ESS means the
  generator and the real corpus barely overlap, which is the coverage gate
  failing in a different disguise.

### 6. Validation on held-out real data

Fit on 70%, validate on 30%. Energy distance, KS per metric, PCA overlap. v1:
**energy distance 2.35 versus a 2.83 baseline — a 17% improvement.** Modest,
and honestly reported; a v2 number that is not better than this has not earned
the architecture change.

### 7. Certify and export

The weighted design becomes the **certified envelope**: the region S0 may
sample. Export it as deterministic Rust, following the precedent of
`golf-terrain/src/sampler/ptheta.rs` on `main` (163 KB, generated) — unit-space
design points, a weight CDF, and a `(lo, hi, log, int)` decode spec.

**Stamp the fingerprint.** The envelope's hash goes into every `spec.json` as
`envelope_fingerprint`, and a spec whose fingerprint does not match the loaded
envelope is **rejected at load, loudly**. This is the interlock that stops a
recalibration from silently changing generator output, and it is the mechanism
`crates/course-spec/src/prior.rs` already implements as `FINGERPRINT_GOLDEN` —
reuse the pattern.

## What an envelope must guarantee

Beyond "produces realistic metrics", the envelope carries hard guarantees that
downstream stages depend on for the no-retry rule to hold:

1. **[S3](../stages/stage-03-amplification.md)'s skeleton preservation.** Per
   biome, the maximum residual amplitude at which the taper still holds and the
   drainage network survives amplification intact. A **hard upper boundary** —
   S0 may never draw above it. Heathland's will be the tightest, since its
   deranged network is the easiest to accidentally integrate.

   This replaces what an earlier draft called the erosion "restructuring
   ceiling". The quantity is similar and the reason it is a hard boundary is
   identical, but it now bounds a synthesis amplitude rather than a simulation
   intensity — which is easier to certify, because amplitude is a parameter
   rather than an emergent outcome.
2. **[S5](../stages/stage-05-siting-substrate.md)'s siting viability.** Every θ
   in the envelope must yield terrain on which *some* 600 m window scores
   acceptably. Siting helps here — it gets to choose the best ground rather
   than accept the middle — but it cannot rescue a site that is uniformly
   unroutable.
3. **[S6](../stages/stage-06-routing.md)'s search viability.** Every θ in the
   envelope must yield a substrate on which the router finds a valid route
   *inside the selected window*. For [Hill Country](../biomes/hill-country.md)
   this means the envelope must guarantee **sufficient contiguous feasible
   area within a 600 m window**, since scarce non-contiguous feasible ground
   can make routing genuinely impossible — and under no-retry, "impossible" has
   no fallback.
4. **[S7](../stages/stage-07-earthmoving.md)'s fundability.** Available borrow
   must cover required fill across the envelope, or the mass-balance rule is
   unsatisfiable.
5. **Budget compliance.** No θ in the envelope may push a stage past its
   allocation in
   [../02-performance-budget.md](../02-performance-budget.md).

These are **checkable envelope constraints**, not aspirations, and they should
be verified by sampling the envelope densely and running the checks — the same
Sobol machinery, a different acceptance test.

## Rebinding the recovered harness

`tools/calibration/` needs three changes to run against v2:

1. **`param_schema.py`** — bound to v1's `TerrainParams`/`ErosionParams`.
   Rebind to the v2 biome records and stage dials, and rebuild the `drives`
   intent map against the metric table in
   [metric-battery.md](metric-battery.md).
2. **`forward.py`** — calls `xtask forward-grid`, which does not exist here.
   Rebind to a new `course-cli` batch entrypoint. **This is the prerequisite
   for the whole loop.**
3. **`export_sampler.py`** — targets `golf-terrain/src/sampler/ptheta.rs`.
   Retarget to the v2 envelope format that `course-spec` loads.

`sample.py`, `coverage.py`, `emulator.py`, `inverse.py`, and `validation.py`
should port with little or no change — they operate on metric matrices and are
agnostic to what produced them.

Also blocking: `tools/metrics` and `tools/dtm_metrics` are **currently
non-runnable** for lack of `tools/parkland_atlas/out/cache`. See
[lidar-pipeline.md](lidar-pipeline.md) and
[MIGRATION.md](../../MIGRATION.md).

## Open questions

1. **Envelope representation.** A Gaussian mixture in transformed space is
   presumed — `tools/landform_prior/landform_prior/fit.py` (on this branch,
   marked retired but a good pattern) has the closest prior art: weighted
   quantile tables, log-log regressions, a Gaussian copula over residuals, and
   Kish-ESS shrinkage. Alternatives: a convex hull with rejection-free interior
   sampling, or a normalizing flow. **Blocks S0's implementation**, because
   S0's sampling code depends on the shape.
2. **Rejection-free sampling.** S0 may not reject
   ([S0's determinism note](../stages/stage-00-archetype-draw.md)), so the
   envelope needs an invertible transform placing every draw inside by
   construction. A GMM gives this naturally; a convex hull does not.
3. **How are the five hard guarantees certified?** Dense sampling plus checks
   is the obvious approach, but "dense" needs a number, and the failure
   probability that number implies needs stating.
4. **Recertification cadence.** Every generator change invalidates the
   envelope in principle. In practice? Probably: re-run coverage and validation
   on every stage change, full refit on contract changes.
5. **What if a biome cannot be certified** — no region of parameter space
   satisfies both the metric targets and the four hard guarantees? That is a
   real possible outcome, most likely for
   [Hill Country](../biomes/hill-country.md) (S4's ceiling versus its
   appearance target) or [Heathland](../biomes/heathland.md). The answer is a
   generator change, not a relaxed guarantee — but the escalation path should
   be written down before it is needed.
