# Project Plan

Goal: a deterministic generator that turns a seed into a complete, playable
9-hole (par-36) golf-course terrain — heightfield, water, cover, routing, and
finished hole earthworks — in one of five named archetypes, with the archetype
driving both terrain character and (later) art direction.

Cross-cutting rules and the repo map: [ARCHITECTURE.md](ARCHITECTURE.md).
Per-step contracts: [steps/](steps/).

## The pipeline

| # | Step | Doc | Output (one line) |
|---|------|-----|-------------------|
| 1 | Seed | [steps/01-seed.md](steps/01-seed.md) | the master seed + run identity |
| 2 | Archetype selection | [steps/02-archetype.md](steps/02-archetype.md) | archetype + fully sampled parameter pack (`CourseSpec`) |
| 3 | Macro landform | [steps/03-macro-landform.md](steps/03-macro-landform.md) | base heightfield + conditioning fields + structure graph |
| 4 | Conditioned noise | [steps/04-conditioned-noise.md](steps/04-conditioned-noise.md) | composed heightfield (skeleton + modulated noise) |
| 5 | Erosion & hydrology | [steps/05-erosion-hydrology.md](steps/05-erosion-hydrology.md) | eroded heightfield + flow model + streams/lakes/wetlands |
| 6 | Cover assignment | [steps/06-cover.md](steps/06-cover.md) | per-cell cover class + canopy density |
| 7 | Routability gate | [steps/07-routability-gate.md](steps/07-routability-gate.md) | pass/fail report with published metrics (reroll on fail) |
| 8 | Routing | [steps/08-routing.md](steps/08-routing.md) | 9-hole returning-nine routing plan |
| 9 | Hole earthmoving | [steps/09-earthmoving.md](steps/09-earthmoving.md) | local 0.5 m height patches (tees/greens/bunkers) |
| 10 | Drainage validation | [steps/10-drainage.md](steps/10-drainage.md) | drainage audit + repair patches + final course bundle |

Data flows strictly forward; the only loop is the gate (step 7 fail ⇒ reroll
from step 2 with a derived sub-seed, bounded attempts).

## Phases

- **P0 — docs hardening (now).** Refine ARCHITECTURE.md + the step docs with
  the user until each step doc is strong enough to hand to an agent
  unsupervised. Exit: user sign-off per doc.
- **P1 — foundations.** The shared substrate the steps sit on: artifact
  store + grid format, determinism kit (seeded domain-split RNG), fixture
  set, and a minimal per-step viewer/harness. Exit: a fixture-backed
  end-to-end skeleton runs and renders.
- **P2 — parallel step build-out.** One agent per step doc, all steps in
  parallel against fixtures. Critical path: step 05 (erosion/hydrology) —
  start it first. Exit per step: its doc's Hard Requirements all tested, a
  determinism double-run, and its viewer tab showing real output.
- **P3 — composition.** Replace fixtures with real steps one seam at a time;
  wire the gate/reroll loop; first archetype end-to-end is `piedmont` (the
  fluvial general case — the other four are specializations).
- **P4 — calibration & validation.** The archetype data campaign (below)
  replaces placeholder parameter priors with fitted ones; batch harness
  measures gate pass-rates per archetype; generated terrain is scored in the
  same metric space as the real-course atlas.

## Parallelization protocol

- Each step doc has a **Status** line: `unclaimed` → `claimed (<agent>, <date>)`
  → `built` → `verified`. Claim before starting; keep the doc current — the
  doc IS the step's source of truth, the code is its implementation.
- An agent owns exactly: its step's doc, its step's module, its fixtures
  usage. It never edits another step's module.
- Contract changes are cross-step events: propose in the step doc's Open
  Questions, resolve with the user, then update every affected doc +
  ARCHITECTURE.md before code changes.
- Never weaken a fixture or a requirement to make a step pass.

## Archetype data campaign (parallel to everything)

Parameters per archetype must come from real courses, not intuition:

1. **Label** the existing 200-course atlas (135 with 1 m lidar,
   `terrain-v2:tools/dtm_atlas`) into the five archetypes — geography (course
   bbox) + relief + the 49-scalar morphology vectors + existing soft
   clustering (`terrain-v2:tools/tile_scout/out/archetypes.json`); manual
   override list; `none` is a valid label.
2. **Collect gap courses** for thin archetypes — sandhills (today only
   pinehurst2, streamsong) and glacial_moraine (erin, moraine) — via the
   existing `dtm_atlas` fetch machinery.
3. **Fit per-archetype priors** (quantile tables per parameter, later a
   copula) consumed by step 02, plus per-archetype calibration targets
   (drainage density, lake %, depression density, canopy %) used as
   acceptance checks by steps 05–06.

## Current status

P0. All docs seeded from the M0 design on `archetype-pipeline`; refinement
with the user in progress. No steps claimed.
