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
| Stage intensity dials | Per-stage scalars. A biome that does not use a module runs it at zero, not around it. |
| Calibration targets | The measured metric vector the biome is fit to ([calibration/targets.md](calibration/targets.md)). |
| Scorer preset | The weights S6 routes against, carried through contract C2. |
| `plasticity` | See below. |

The payoff is that a biome can be retuned, or a new one added, without a stage
author being involved — and that a bug found in one biome is a bug fixed in
all of them.

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
| S1 | Macro primitives | [stage-01](stages/stage-01-macro-primitives.md) | contract **C1** | |
| S2 | Skeleton kernel | [stage-02](stages/stage-02-skeleton-kernel.md) | the structural heightfield | † |
| S3 | Transforms & datum ops | [stage-03](stages/stage-03-transforms.md) | height + water polygons + basin inventory | |
| S4 | Simulation finishers | [stage-04](stages/stage-04-finishers.md) | textured heightfield | † |
| S5 | Substrate assembly | [stage-05](stages/stage-05-substrate-assembly.md) | contract **C2** | |
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

## Four deliberate reversals from v1

The previous architecture on branch `pipeline` was a 14-stage, sim-co-authored
pipeline. v2 reverses four of its decisions. They are recorded here with their
reasoning so they are not silently re-litigated later.

### The sim is demoted; the skeleton is authored

**v1:** stage 05's landscape-evolution model was *the co-author*. Realism was
delegated to physics because the authored-primitive generator before it had
been measured against a 90-tile corpus and failed — 22 of 71 knobs inside the
real-tile IQR (archive commit `0313432`,
`tools/macro_campaign/out/report/compare.md`).

**v2:** S2's fluvial engine authors the structure directly. S4 erosion is
texture-only and *never restructures*.

The reasoning is not that the v1 measurement was wrong — it stands. It is that
the failure was specific to authoring in **landform-primitive space**: placing
ridges and valleys as shapes and hoping the statistics followed. S2 authors in
**flow-distance / drainage space** instead — trunk splines, tributary growth at
a calibrated spacing, a flow-distance transform, derived divides, and a catena
library — which produces drainage-network statistics by construction rather
than by luck.

What is traded away is emergent surprise. What is bought is controllability and
the ability to certify envelopes offline, which is what makes the no-retry
guarantee possible at all. A long-running LEM cannot be certified cheaply and
cannot be bounded tightly enough for a sub-10s budget.

**This is a bet.** [calibration/targets.md](calibration/targets.md) is where it
gets settled: if the fluvial engine cannot hit the process metrics that the v1
LEM hit, the bet was wrong and S2 needs the simulation back.

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
(Great Plains, River Valley, and the rebuilt Heathland) have no corpus at all
yet. See [calibration/targets.md](calibration/targets.md) — this is the
pipeline's largest open data debt and it blocks S0, S2, S4, and S9.

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
