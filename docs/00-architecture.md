# The v2 Pipeline

Twelve stages, S0 through S11, run in a fixed order. A seed goes in; a
complete, playable 9-hole (par-36) golf course comes out in under ten seconds,
byte-identically on every platform.

Repo map and cross-cutting invariants: [../ARCHITECTURE.md](../ARCHITECTURE.md).
Milestones and the claim protocol: [../PLAN.md](../PLAN.md).

## The three ideas

### 1. One invariant pipeline, config-only biomes

**Stages never branch per biome.** There is no `if piedmont` anywhere in the
pipeline, and adding a biome adds no code to any stage.

A biome is a data record — see [biomes/README.md](biomes/README.md) — with
five parts:

| Part | Meaning |
|---|---|
| Kernel selection | Which [`SkeletonKernel`](stages/stage-02-skeleton-kernel.md) builds the structure. All six base biomes select Kernel I, the fluvial engine. |
| **Exemplar pool** | The real tiles whose residual patches supply this biome's texture at [S3](stages/stage-03-amplification.md). **This is where identity actually lives** — see below. |
| Stage intensity dials | Per-stage scalars. A biome that does not use a module runs it at zero, not around it. |
| Calibration targets | The measured metric vector the biome is fit to ([calibration/targets.md](calibration/targets.md)). |
| Scorer preset | The weights S6 routes against, carried through contract C2. |
| `plasticity` | See below. |

The payoff is that a biome can be retuned, or a new one added, without a stage
author being involved — and that a bug found in one biome is a bug fixed in all
of them. With the exemplar pool carrying identity, **adding a biome is mostly a
data-collection job.**

### 2. Three guarded contracts

Most of the pipeline is expected to churn. Three interfaces are not:

| Contract | Between | Doc |
|---|---|---|
| **C1** | macro primitives → skeleton kernel | [contracts/C1-primitives-to-kernel.md](contracts/C1-primitives-to-kernel.md) |
| **C2** | skeleton → routing (*the routing substrate*) | [contracts/C2-routing-substrate.md](contracts/C2-routing-substrate.md) |
| **C3** | routing → realization (corridor graph + budgets) | [contracts/C3-corridor-realization.md](contracts/C3-corridor-realization.md) |

These three are golden-seed tested and versioned. Everything between them may
be replaced wholesale. C2 is the load-bearing one: it is the waist of the
pipeline, and S6 onward can see nothing except C2 — not the skeleton, not the
biome, not how any of it was made.

### 3. Offline calibration, runtime guarantees

**Runtime never rejects and never retries.** No gates, no reroll loop, no
"generate until it passes". A seed always produces a course.

That is only safe because the rejecting happens offline. A calibration harness
ingests lidar, measures it through a metric battery, and inverse-fits
**certified parameter envelopes** — regions of parameter space already known to
produce acceptable courses. Runtime samples only from inside them. See
[calibration/envelope-certification.md](calibration/envelope-certification.md).

The consequence worth stating plainly: a bad course is a **calibration bug**,
to be fixed offline by tightening an envelope. It is never a seed to discard.

## Plasticity

`plasticity` is a scalar in [0, 1] carried from S0 to S7.

- **0** — terrain dictates routing. The router must find holes that fit the
  land as it is; S7 grades almost nothing.
- **1** — routing dictates terrain. The router places the course it wants and
  S7 earthmoves the ground to match.

It is a single dial with two effects: it sets S6's feasibility strictness and
its fit-versus-cost balance, and it scales S7's grading budget. Sandhills sits
low (the dunes are the point); a heavily-engineered parkland biome sits high.

Plasticity is *not* a quality dial. High plasticity is not "better" — it is a
different kind of golf course, and the calibration targets differ accordingly.

## The twelve stages

| # | Stage | Doc | Output | † |
|---|---|---|---|---|
| S0 | Archetype & site draw | [stage-00](stages/stage-00-archetype-draw.md) | config only: biome + site descriptors | † |
| S1 | Macro structure | [stage-01](stages/stage-01-macro-primitives.md) | contract **C1** | |
| S2 | Drainage skeleton | [stage-02](stages/stage-02-skeleton-kernel.md) | network, divides, base surface | † |
| S3 | **Amplification** | [stage-03](stages/stage-03-amplification.md) | the terrain, textured from real data | † |
| S4 | **Hydrology & transforms** | [stage-04](stages/stage-04-hydrology.md) | flow + water polygons + basin inventory | |
| S5 | **Site selection & substrate** | [stage-05](stages/stage-05-siting-substrate.md) | the play window + contract **C2** | |
| S6 | Routing | [stage-06](stages/stage-06-routing.md) | contract **C3** | |
| S7 | Earthmoving realization | [stage-07](stages/stage-07-earthmoving.md) | graded heightfield | |
| S8 | Hole layout | [stage-08](stages/stage-08-hole-layout.md) | hole geometry + zones | |
| S9 | Micro re-pass | [stage-09](stages/stage-09-micro-repass.md) | final heightfield + surface fields | † |
| S10 | Zoning & aesthetics | [stage-10](stages/stage-10-zoning-aesthetics.md) | cover mosaic + placement manifests | |
| S11 | Validation | [stage-11](stages/stage-11-validation.md) | the report | |

† = has offline parameter-tuning needs; its doc carries a Calibration section.

Data flows strictly forward. The only backward edge in the whole pipeline is
S7's single bounded repair pass, and it is bounded precisely so that it is not
a loop.

## Where realism comes from

The single most important structural fact about this pipeline, and the one that
determines what each stage is for.

`tools/macro_campaign/macro_campaign/structure.py` measures
`dist_to_channel_p50` at **104–120 m in every real archetype regardless of
relief**, and flags it as the most robust metric in the set. Drainage spacing
is a **law of landscapes, not a signature** — a sandhills tile and a
hill-country tile have nearly the same channel spacing.

So the skeleton cannot carry archetype identity, however well it is built.
Identity lives in hillslope form and texture. That splits the terrain half of
the pipeline cleanly in two:

| | Owns | Fitted to |
|---|---|---|
| **[S2](stages/stage-02-skeleton-kernel.md) skeleton** | structure, routability, drainage correctness | **shared invariants** — the quantities every archetype hits alike |
| **[S3](stages/stage-03-amplification.md) amplification** | texture, hillslope form, **archetype identity** | **discriminants** — the quantities that actually differ |

S2 authors a correct, characterless base. **S3 reconstructs detail from a
dictionary of real terrain patches**, fitted offline to that biome's lidar
corpus and conditioned on position within the skeleton. Realism is not
simulated and not parameterized — it is *sampled from the real thing*.

See [calibration/metric-battery.md](calibration/metric-battery.md) for the
invariant/discriminant split and its evidence.

## Deliberate reversals from v1

The previous architecture on branch `pipeline` was a 14-stage, sim-co-authored
pipeline. v2 reverses six of its decisions. They are recorded here with their
reasoning so they are not silently re-litigated later.

### Realism comes from data, not from simulation or parameters

**v1:** stage 05's landscape-evolution model was *the co-author*. Realism was
delegated to physics because the authored-primitive generator before it had
been measured against a 90-tile corpus and failed — 22 of 71 knobs inside the
real-tile IQR (archive commit `0313432`,
`tools/macro_campaign/out/report/compare.md`).

**v2:** S2 authors structure; S3 reconstructs texture from a fitted patch
dictionary.

Both previous attempts failed for the same underlying reason — they tried to
*produce* real terrain statistics from a model, whether authored primitives or
simulated physics. Noise had no structure; primitives had structure but wrong
statistics; a LEM has both but cannot be bounded or certified cheaply enough
for a sub-10s no-retry budget.

The third approach does not model the statistics at all. It **measures real
terrain, decomposes it into structure plus residual, and reconstructs the
residual conditioned on an authored structure.** What is traded away is the
possibility of terrain unlike anything in the corpus. What is bought is that
the texture is real by construction, plus controllability and certifiability.

**This is still a bet**, and
[calibration/envelope-certification.md](calibration/envelope-certification.md)
is where it gets settled: run the coverage gate **separately on invariants and
discriminants**. Poor discriminant reachability means the dictionary is too
small or too coarsely conditioned. Poor invariant reachability means the
skeleton engine is wrong and no texture will save it. v1's single 59% number
could not tell those apart.

### The course is sited, not centred

**v1:** the routable area was nailed to the whole core, `[750, 2250]²`, and
routing took whatever terrain it was handed.

**v2:** [S5](stages/stage-05-siting-substrate.md) scores the core and selects a
**600 m play window** — translation only, centre free ±450 m per axis.

Real course siting is an architect choosing the good 150 acres out of 500.
Doing the same here improves the golf *and* is a large source of between-seed
variety, since the same terrain yields a different course depending on where
the window lands.

The geometry closes exactly and **no world constant moves**: a 600 m window
whose centre ranges ±450 m spans precisely the existing core, so `in_core`
still bounds everything and the **750 m minimum terrain margin** — a hole at
the edge of play must still have ground running out past it — holds by
construction.

Rotation was considered and rejected: redundant with S1's randomized grain axis
and base-level edge, and it would break the resolution ladder's nesting
property and hand the renderer a heightmap with a transform attached.

### The pre-authored routability mask is gone

**v1:** stage 02 authored the routability guarantee *before terrain existed* —
a connected mask that every later stage was forbidden to break. "The mask is
law."

**v2:** there is no mask. Routability comes from `plasticity` plus S7 grading:
either the terrain is gentle enough to route on, or the router is permitted to
move earth until it is.

The mask solved a real problem, but it did so by constraining every downstream
stage, and it only worked because a gate could reject and reroll when the
constraint failed. With no retries, a pre-authored guarantee has nothing to
fall back on. Certified envelopes serve the same purpose earlier and without
the cross-stage coupling.

### No runtime retries

**v1:** a stage-08 gate failure incremented an attempt counter and resampled
stages 2–3. `course-seed` still carries the machinery: `MAX_ATTEMPTS = 8` and
the `stable`/`attempt` stream scoping that let stages 0–1 survive a reroll.

**v2:** nothing retries. No v2 stage raises an attempt, so `attempt` is pinned
at 0 for every run.

The machinery is **retained deliberately**, not removed: it costs nothing while
attempt is 0, and deleting it would rekey every RNG stream in the pipeline for
no behavioural gain. Treat it as vestigial, not as an option.

### Clean break on biomes

**v1:** five archetypes — `piedmont`, `sandhills`, `florida_lowland`,
`glacial_moraine`, `mountain_bench` — with a fitted 88-knob prior and a 2.3 GB
tile corpus behind them.

**v2:** the six Heartland biomes. The old prior, corpus, and galleries are
reference-only; nothing carries over as fitted data. Three of the six
(Great Plains, River Valley, and the rebuilt Heathland) have no corpus at all,
and of the 90 tiles on disk only **38 are clean**.

See [calibration/targets.md](calibration/targets.md) — the pipeline's largest
open data debt, and it now blocks more than it used to. S0, S2, and S9 need it
for targets; **S3 needs it to exist at all**, since without an exemplar corpus
there is no dictionary and no texture.

### Variety is multiplicative, not just continuous

**v1 →  early v2:** the framing stage's categorical window classes and typed
province boundaries were replaced with smooth continuous fields.

**v2, corrected:** they are restored. [S0](stages/stage-00-archetype-draw.md)
draws a **categorical structural class**, [S1](stages/stage-01-macro-primitives.md)
realizes it and places **structural discontinuities**, and envelopes are
certified as a **union of modes** rather than one blob.

Continuous descriptors alone give courses that differ by degree; categorical
draws give courses that differ in kind. Discontinuities — a scarp, a material
contact — also carry disproportionate golf value, because a line where the
ground changes character is where dramatic holes come from, and no smooth field
can produce one.

There is a real tension here that the earlier draft did not name: **a certified
envelope exists to be narrow, and narrowness is antagonistic to variety.**
Multi-modal envelopes plus categorical structure plus exemplar selection plus
window siting are how the pipeline gets variety *without* widening any single
envelope.

## Future biomes

The base six are all served by Kernel I plus its five modules. Future packs —
Coastal, Desert, Highlands, The North, Fire & Stone, Tropical — plug in behind
contract C1 as additional kernels and behind S3 as additional transforms. The
seams exist and are labeled today; they are empty. See
[biomes/future-biomes.md](biomes/future-biomes.md).

The one to keep in mind while designing C2: **Lowcountry Marsh** (Coastal pack)
resumes the adversarial contract-tester role when it lands — near-zero relief,
water as the dominant structure, and a routing problem that looks nothing like
the base six. If C2 survives Lowcountry Marsh, C2 is right.
