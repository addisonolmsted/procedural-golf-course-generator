# Project Plan

Goal: a deterministic generator that turns a seed into a complete, playable
9-hole (par-36) golf course — final heightfield, water, cover, routing, finished
hole construction, and the object manifest — in one of six named biomes, in
under ten seconds, on terrain that is **varied, interesting, plausible, and golf
routable**:

- *Varied* — variety is **multiplicative**: a categorical structural class and
  an exemplar pool at [S0](docs/stages/stage-00-archetype-draw.md), structural
  discontinuities at [S1](docs/stages/stage-01-macro-primitives.md),
  multi-modal envelopes, and a **sited play window** at
  [S5](docs/stages/stage-05-siting-substrate.md). Continuous descriptors alone
  give courses that differ by degree; these give courses that differ in kind.
- *Interesting* — [S8](docs/stages/stage-08-hole-layout.md)'s dispersion pass
  stops nine holes from being one hole nine times, and siting puts the course on
  the best ground rather than the middle of the map.
- *Plausible* — **[S3](docs/stages/stage-03-amplification.md) reconstructs
  texture from real lidar residuals**, so terrain statistics are sampled from
  the real thing rather than modelled. S2 supplies the structure the
  reconstruction is conditioned on. The
  [calibration loop](docs/calibration/) is the referee.
- *Routable* — `plasticity` plus [S7](docs/stages/stage-07-earthmoving.md)
  grading, with the guarantee enforced by
  [certified envelopes](docs/calibration/envelope-certification.md) rather than
  by a runtime gate.

Cross-cutting rules and the repo map: [ARCHITECTURE.md](ARCHITECTURE.md).
The architecture itself: [docs/00-architecture.md](docs/00-architecture.md).
**How we know each stage worked**: [docs/03-success-indicators.md](docs/03-success-indicators.md)
— the per-stage gates, the dispersion-ratio variety target, the qualitative
protocols, and the golfability proxy. Milestones below reference its gates.
Where pre-v2 code went: [MIGRATION.md](MIGRATION.md).

## The pipeline

| # | Stage | Doc | Output | Budget |
|---|---|---|---|---:|
| S0 | Site & biome draw † | [stage-00](docs/stages/stage-00-archetype-draw.md) | `CourseSpec` — biome, structural class, descriptors, exemplars | 1 ms |
| S1 | Macro structure | [stage-01](docs/stages/stage-01-macro-primitives.md) | contract **C1** | 120 ms |
| S2 | Drainage skeleton † | [stage-02](docs/stages/stage-02-skeleton-kernel.md) | network, divides, base surface | 900 ms |
| S3 | **Amplification** † | [stage-03](docs/stages/stage-03-amplification.md) | the terrain, textured from real data | 900 ms |
| S4 | **Hydrology & transforms** | [stage-04](docs/stages/stage-04-hydrology.md) | flow + water + basin inventory | 250 ms |
| S5 | **Site selection & substrate** | [stage-05](docs/stages/stage-05-siting-substrate.md) | play window + contract **C2** | 400 ms |
| S6 | Routing | [stage-06](docs/stages/stage-06-routing.md) | contract **C3** | 1 300 ms |
| S7 | Earthmoving | [stage-07](docs/stages/stage-07-earthmoving.md) | graded heightfield | 1 600 ms |
| S8 | Hole layout | [stage-08](docs/stages/stage-08-hole-layout.md) | hole geometry + zones | 900 ms |
| S9 | Micro re-pass † | [stage-09](docs/stages/stage-09-micro-repass.md) | final heightfield + surface fields | 1 400 ms |
| S10 | Zoning & aesthetics | [stage-10](docs/stages/stage-10-zoning-aesthetics.md) | cover mosaic + manifests | 700 ms |
| S11 | Validation | [stage-11](docs/stages/stage-11-validation.md) | the report | 400 ms |

† = has offline calibration needs. Data flows strictly forward; the only
backward edge is S7's single bounded repair pass. S0–S6 are blocking (~3.9 s to
first route); S7–S10 stream per hole. Full table:
[docs/02-performance-budget.md](docs/02-performance-budget.md).

## Milestones

- **M0 — scaffold + docs (done 2026-08-06).** v1 stage-01 work preserved on
  `pipeline` (`1ad56c7`); `heartland` branch cut; 14 crate scaffolds;
  27 documentation files; `MIGRATION.md`.
- **M0b — pipeline revision (done 2026-08-07).** S3/S4 swapped: amplification
  from a real-terrain patch dictionary replaces erosion; hydrology moved after
  it. Site selection added at S5. Categorical structure and discontinuities
  restored at S0/S1. Metric targets reorganized onto the
  invariant/discriminant axis.
- **M1 — contracts + the stream rekey.** Implement `course-contracts`: C1, C2,
  C3 as real types with construction-time invariant enforcement, plus fixture
  builders. Nothing else can be tested in isolation until these exist. **Do
  this first.** Rekey the stream registry from the v1 names to the v2 names the
  stage docs declare, in **one** `PIPELINE_VERSION` bump — batched deliberately,
  because rekeying re-blesses every golden. See the registry table in
  [ARCHITECTURE.md](ARCHITECTURE.md).
- **M2 — the batch entrypoint.** `course-cli` headless driver with a
  `forward-grid` equivalent. Small, and it unblocks the entire calibration
  loop — `tools/calibration/forward.py` has nothing to call without it.
- **M3 — S1 + S2 (the critical path).** Build early, measure early. S1's exit
  test is **class legibility** (name the window class from the implied-terrain
  hillshade). S2's exit is **gate G-SKELETON**: shared invariants in band on
  the base surface, **conditioning-space overlap ≥ ~90% vs real tiles** (the
  leading indicator that predicts S3's success before S3 exists), the
  golfability proxy in regime, P1 overlay review — and explicitly NOT judged
  on texture, which is expected to look like candle wax.
- **M3.5 — the dictionary spike (de-risk, before the campaign).** Fit a crude
  dictionary from the **6 clean piedmont tiles already on disk**
  (`surfaces.py` residuals, coarse buckets), reconstruct over a synthetic
  base. ~A week. The least-proven component must not be validated last: if the
  spike fails, the fallback conversation happens *before* 300 tiles are
  fetched. Prerequisite: recover `tools/parkland_atlas` from `main` (also the
  first step of M4). **Gate: G-SPIKE** — blocking go/no-go
  ([success indicators](docs/03-success-indicators.md)).
- **M4 — corpus + battery.** The long pole, and it gates almost everything.
  Implement the four missing metrics and run the G1–G6 gates; **confirm the
  invariant/discriminant split empirically** rather than assuming it; then
  collect corpora in the order in
  [targets.md](docs/calibration/targets.md) — piedmont first (the anchor), then
  heathland (the golden most likely to invalidate an assumption). Target ~30
  clean tiles per biome, ~180 total, at ~40% attrition.
- **M4.5 — settle C0 with the frontend team.** The
  [delivery contract](docs/contracts/C0-delivery.md)'s **TBD-frontend** rows:
  heightmap format, quantization, play-grid extent, beyond-window LOD. Must
  land before S3 is implemented — quantization depth bounds how much fine
  texture is worth generating.
- **M5 — the dictionary.** Fit the patch basis and coefficient distributions
  per conditioning bucket, offline in `course-calibration`. Ship target
  **< 15 MB**. Report **per-bucket tile diversity**, not just patch count — a
  bucket sourced from one tile will reproduce that tile.
- **M6 — S3 + S4 + S5.** Amplification, hydrology, siting, C2 assembly.
  Exits through the two big gates: **G-TERRAIN** (discriminants in band,
  energy distance beats 2.35, blind A/B ≤ 65%, name-the-biome ≥ 80%,
  dispersion ratio 0.7–1.3, `skeleton_agreement` ≥ 0.9) and **G-SITE**
  (chosen window beats centred + median-random on the fitted score;
  golfability proxy inside the real-course band). G-TERRAIN + G-SITE on
  piedmont, heathland, and sandhills = **terrain-complete**, the declared
  precondition for starting routing work.
- **M7 — first certified envelope.** Recover and rebind `tools/calibration`
  from `main`; run the loop for piedmont; produce an envelope S0 can sample.
  **The coverage gate here is the referee on the realism bet** — run it
  separately on invariants and discriminants, because they fail for different
  reasons and have different fixes.
- **M8 — S0 + S6.** Envelope sampling and routing. First end-to-end route.
- **M9 — S7 + S8.** First playable hole; the streaming seam becomes real.
- **M10 — S9 + S10 + S11.** Final detail, dressing, and the measurement
  apparatus. First complete course.
- **M11 — all six biomes certified.** Corpora, dictionaries, targets, and
  envelopes for the remaining five. This is the milestone that turns one
  working biome into a generator.

## Parallelization protocol

- Each stage doc carries a **Status** line: `unclaimed → claimed (<agent>,
  <date>) → built → verified`. Claim before starting; **the doc is the stage's
  source of truth**, not the code.
- An agent owns exactly its stage's doc, its module, and its fixtures. It never
  edits another stage's module.
- **Contract changes are not unilateral.** Propose in the doc's Open Questions,
  resolve with the user, then update every affected doc plus
  [ARCHITECTURE.md](ARCHITECTURE.md) *before* writing code.
- Never weaken a fixture, an invariant, or an acceptance criterion to make a
  stage pass. If a criterion is wrong, change it deliberately and say so.
- **Dependency note for claiming:** M1's contracts unblock everything. Given
  fixtures, S1–S4 are independent of each other; S5 needs C1/C2 shapes; S6 needs
  only C2; S7–S10 need C3 and can be built per-hole against fixtures.

## Known blockers

Recorded here so they are not rediscovered one stage at a time.

| Blocker | Blocks | Where |
|---|---|---|
| **Three of six biomes have no corpus**; only **38 clean tiles** exist and they are keyed to the retired archetypes | **S3 entirely** (no corpus ⇒ no dictionary ⇒ no texture), plus S0/S2/S9 targets | [targets.md](docs/calibration/targets.md) |
| **Four of the 22 metrics do not exist** and are ungated | heathland, great-plains, river-valley, hill-country fits | [metric-battery.md](docs/calibration/metric-battery.md) |
| No batch entrypoint (`xtask forward-grid` is not on this branch) | the entire calibration loop | M2 |
| ~~metric tools non-runnable~~ **Resolved 2026-08-07**: battery verified end-to-end on real tiles (`tools/spike/smoke.py`); parkland cache proved unnecessary for v2 | — | [MIGRATION.md](MIGRATION.md) |
| `tools/dtm_atlas/out/` DTM store absent (needed by `gates.py` for metric admission) | M4's G1–G6 gates | refetchable via `dep3.py`; indices committed |
| Dictionary patch size, basis rank, bucket boundaries undecided | S3's asset format | [stage-03](docs/stages/stage-03-amplification.md), settled by the M3.5 spike |
| **C0 delivery format TBD with the frontend team** (heightmap format, quantization, LOD) | S3 implementation (quantization bounds texture depth) | [C0-delivery.md](docs/contracts/C0-delivery.md) |
| Envelope representation undecided (GMM vs hull vs flow) | S0's sampler | [envelope-certification.md](docs/calibration/envelope-certification.md) |
| Siting score undefined (needs fitting to the 64 course grids) | S5's `siting` | [stage-05](docs/stages/stage-05-siting-substrate.md) |
| S7's template library and S8's surface passes undesigned | S7, S8 | prior art in `golf-holes/` on `main` |
| Enclosure model representation undecided | S10, and S11's sightline check | [stage-10](docs/stages/stage-10-zoning-aesthetics.md) |

## The open architectural risks

Two, in order of how much they would cost to be wrong about.

**1. The dictionary may not carry identity.** The whole architecture now rests
on the claim that archetype identity is texture, and that a conditioned patch
dictionary can reproduce it. If generated piedmont and generated hill country
come out indistinguishable on the discriminants, neither the skeleton nor the
dictionary is doing its job and there is no third fallback in reserve.

This is why M7's coverage gate must be run **separately on invariants and
discriminants** — the two failure modes have different fixes and v1's single
59% number could not tell them apart.

**2. Heathland's negative drainage integration** may not be expressible as "the
same fluvial growth with a dial turned down". If it needs genuinely different
growth, that is a second kernel wearing a module's clothes and should be
admitted as such. Cheaper to be wrong about than (1), and it is why heathland is
second in the corpus collection order despite piedmont being the anchor. See
[S2's open questions](docs/stages/stage-02-skeleton-kernel.md).

## Current status

**M0 complete (2026-08-06); M0b complete (2026-08-07).** Branch `heartland`,
cut from `pipeline` at `1ad56c7`. Workspace holds 21 crates — 3 shared
substrate reused unchanged, 14 v2 scaffolds, and 4 retained from v1 pending
retirement; `cargo check --workspace` is clean and the 112 retained tests pass.
The documentation tree is complete: 12 stage docs, 3 contracts, 8 biome files,
4 calibration docs, plus the three root documents.

**All twelve stages are unclaimed. Spike prerequisite complete (2026-08-07):**
`tools/parkland_atlas` + `assets/atlas.bin` recovered from `main`; the
measurement chain verified end-to-end on a real clean piedmont tile
(`tools/spike/smoke.py` — cgrid → `surfaces.build` → the 49-scalar battery,
whole-tile and residual-only); the 6 clean piedmont tiles inventoried; first
fit-target numbers recorded (residual std 1.95 m, β 3.59, variogram range
181 m). **The M3.5 spike itself is unblocked and is the next step**, alongside
M1 (contracts) and M2 (batch entrypoint). C0's TBD-frontend rows (M4.5) need a
conversation with the frontend team sometime before S3 is implemented.
