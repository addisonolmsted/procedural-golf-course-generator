# Project Plan

Goal: a deterministic generator that turns a seed into a complete, playable
9-hole (par-36) golf course — final heightfield, water, cover, routing,
finished hole construction, and the object manifest — in one of five named
archetypes, on terrain that is **varied, interesting, plausible, and golf
routable**:

- *Varied* — stage 01's site framing (window position in an implied larger
  landscape × structural grain × provinces) multiplies between-seed macro
  diversity beyond the archetype choice.
- *Interesting* — stage 03's hero features and province-boundary landforms,
  plus sim-emergent drainage texture.
- *Plausible* — stage 05's landscape evolution model earns real-terrain
  statistics (the retired authored generator measured 22/71 knobs inside the
  real-tile IQR — commit `0313432`); stage 00's covariance-intact θ keeps
  parameter combinations coherent; the campaign's real-vs-generated loop is
  the referee.
- *Routable* — stage 02 authors the guarantee before terrain exists and it
  propagates as physics; the stage-08 gate rejects rarely and rerolls
  resample only stages 2–3.

Cross-cutting rules and the repo map: [ARCHITECTURE.md](ARCHITECTURE.md).
Per-stage contracts: [stages/](stages/).

## The pipeline

| # | Stage | Doc | Output (one line) |
|---|-------|-----|-------------------|
| 0 | Seed & archetype | [stages/00-seed-archetype.md](stages/00-seed-archetype.md) | `RunIdentity` + `CourseSpec` (archetype, θ with covariance) |
| 1 | Site framing | [stages/01-site-framing.md](stages/01-site-framing.md) | window position, base level, tilt, grain, provinces |
| 2 | Routability mask | [stages/02-routability-mask.md](stages/02-routability-mask.md) | the authored guarantee: connected mask + corridors + allowance |
| 3 | Guidance strokes | [stages/03-guidance-strokes.md](stages/03-guidance-strokes.md) | trunk spline, boundary landform, hero, flatten datums |
| 4 | Forcing fields | [stages/04-forcing-fields.md](stages/04-forcing-fields.md) | initial surface, uplift, erodibility, diffusivity |
| 5 | Landscape evolution | [stages/05-landscape-evolution.md](stages/05-landscape-evolution.md) | the macro+meso heightfield (the co-author) |
| 6 | Hydrography | [stages/06-hydrography.md](stages/06-hydrography.md) | creek splines, lakes, wetlands, water table |
| 7 | Cover | [stages/07-cover.md](stages/07-cover.md) | class + canopy + substrate maps |
| 8 | Routability gate | [stages/08-routability-gate.md](stages/08-routability-gate.md) | cheap pass/fail vs the mask invariant |
| 9 | Routing | [stages/09-routing.md](stages/09-routing.md) | 9-hole returning loop; earthmoving allowance as soft cost |
| 10 | Earthmoving | [stages/10-earthmoving.md](stages/10-earthmoving.md) | per-hole displacement layers (plan/surface split) |
| 11 | Drainage validation | [stages/11-drainage-validation.md](stages/11-drainage-validation.md) | flow audit + masked grading repairs |
| 12 | Micro re-pass | [stages/12-micro-pass.md](stages/12-micro-pass.md) | seam blending + calibrated micro noise → final heightfield |
| 13 | Placement | [stages/13-placement.md](stages/13-placement.md) | seed-deterministic object manifest |

Data flows strictly forward. Loops: stage 08 fail ⇒ attempt++ resamples
stages 2–3 (archetype/θ/framing are stable-scoped and survive); stage 09 may
fail the same way; stages 10↔11 iterate within a bounded budget.

## Milestones

- **N0 — reset + docs (done 2026-08-02).** Step-03 era archived
  (`0313432`), workspace reduced to the keepers, `stages/00…13` written.
- **N1 — Stage 00 rev.** Two-tier stream registry v2 (stable/attempt
  scopes) + θ from Gaussian components (provisional diagonal build from the
  existing tables). `PIPELINE_VERSION` 2, goldens re-blessed once.
- **N2 — Stages 1–4 + stage-lab.** Framing, mask, strokes, forcing fields —
  all cheap authored fields — plus the per-stage viewer shell (course-viz).
- **N3 — Stages 5–6 (critical path).** Port the erosion core, build the LEM
  and hydrographic extraction. Start stage 05 first; everything else in N2
  can proceed in parallel against fixtures.
- **N4 — Stages 7–8 + the attempt loop.** First full terrain end-to-end per
  archetype; measure gate pass rates (target ≥ 90% with the mask upstream).
- **N5 — Campaign v2.** Refit θ as per-archetype Gaussian components from
  the corpus; calibrate LEM forcing against `structure.py` metrics
  (drainage density, local relief, dist-to-channel); `netstats` families
  (slope-area θ, width scaling) become VALIDATION targets. Corpus scale-up
  (glacial_moraine is thin).
- **N6 — Stage 9.** Port the beam/anneal router onto the new contracts.
- **N7 — Stages 10–11.** Import the ten-pass earthmoving design (open
  question in its doc), build construction + drainage repair.
- **N8 — Stages 12–13.** Micro finish + placement manifest = shippable
  course bundle.

## Parallelization protocol

- Each stage doc has a **Status** line: `unclaimed → claimed (<agent>, <date>)
  → built → verified`. Claim before starting; the doc IS the stage's source
  of truth.
- An agent owns exactly its stage's doc + module + fixture usage; it never
  edits another stage's module.
- Contract changes: propose in Open Questions, resolve with the user, update
  every affected doc + ARCHITECTURE.md before code.
- Never weaken a fixture or requirement to make a stage pass.
- Dependency note for claiming: stages 1–4 are independent of each other
  given fixtures for their inputs; 5 needs 4's artifact shape (fixture
  early); 6–8 fixture-drivable; 9+ second wave.

## The data campaign (v2)

Everything empirical routes through `tools/`:

- **Retained as-is**: corpus infrastructure (`macro_campaign`
  fetch/develop/courses/regions/character/structure/report/cgrid), the
  `dtm_*` analysis stack, the 90-tile exemplar corpus + course corpus
  (local, refetchable; indices committed), the exclusion-cull QA loop and
  `tile-lab`.
- **Repurposed**: `netstats`/`structure` metric families → LEM calibration
  and validation targets (no longer generator inputs);
  the course-corpus playable-fraction method → stage-02 mask sizing.
- **Rewritten**: `fit_knobs.py` → a GMM fitter (transformed space, gating
  policy carried over); `extract.py` → the new θ knob set per stage docs.
- **Retired**: `shape.py` transect fitting, `landform_prior` (pattern
  reference only).

## Current status

N0 complete (2026-08-02): archive + reset committed, 14 stage docs written,
kept workspace (course-seed, course-spec, course-world, course-viz,
tile-lab) builds with all tests green. Stage 00 is built at v1 (rev
pending); stages 01–13 unclaimed. Next: N1 (Stage 00 rev), then N2/N3 in
parallel with stage 05 first.
