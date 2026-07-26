# Archetype Pipeline — Status

Stage tracker for the archetype-pipeline rebuild. Claim a stage here (agent +
date) before starting work; keep the table honest. Contracts and requirements:
[ARCHITECTURE.md](ARCHITECTURE.md).

Convention (mirrors `docs/terrain-v2-status.md`): a stage is **green** only
with tests + determinism double-run + (once composed) golden coverage.

| Stage | Crate | State | Claimed by | Notes |
|---|---|---|---|---|
| contracts | `course-contracts` | **green (M0)** | — | types, store, gridio, priors (placeholder), fixtures; 20 tests |
| 1–2 seed/spec | `course-seed` | not started | — | fixture: `fixture_spec` (prior medians) |
| 3 macro | `course-macro` | not started | — | fixture: `fixture_skeleton` |
| 4 noise | `course-noise` | not started | — | fixture: `fixture_height` |
| 5 hydro | `course-hydro` | not started | — | **critical path — start first**; fixture: `fixture_hydro` |
| 6 cover | `course-cover` | not started | — | fixture: `fixture_cover` |
| 7 gate | `course-gate` | not started | — | fixture: `fixture_gate` (area proxy, no corridor placement) |
| 8 route | `course-route` | not started | — | second wave; fixture: `fixture_routing` (geometric loop) |
| 9 earthworks | `course-earthworks` | not started | — | second wave; fixture: `fixture_earthworks` |
| 10 drainage | `course-drainage` | not started | — | second wave; fixture: `fixture_drainage` (audit only) |
| orchestrator | `course-pipeline` | not started | — | second wave; store wiring exists in contracts |
| GUI | `course-lab` | **shell green (M0)** | — | all tabs fixture-backed; swap per-stage as crates land |
| data campaign | `tools/archetype_atlas` | not started | — | fully parallel; delivers fitted `archetype_priors.json` |

## M0 record (2026-07-26)

- Branch `archetype-pipeline` off `terrain-v2`.
- `course-contracts`: `ArchetypeId`/`HydrologyMode`, `CourseSpec`/`ParamPack`
  (+ placeholder `data/archetype_priors.json`, schema-final), stage artifact
  types 3–10, `Artifact` store (JSON + CGRID1 sidecars + blake3 manifest),
  DetRng channel registry, fixtures for all stages. 20 tests incl. store
  round-trip, determinism, sandhills-empty-hydro, florida-lakes,
  monotone-drain-spine, feathered-earthworks.
- `course-lab`: egui shell, 9 tabs, seed scrub, archetype combo, fast/full
  res, contours, per-tab overlays (spines, flow tint, streams/lakes, cover
  palette, gate metrics, corridors, features, drainage report).
- xtask: `course-dump` headless renders; `course-fixtures` golden wired into
  `golden`/`bless` (hash folds PIPELINE_VERSION + prior fingerprint + all five
  archetypes' composed/flow/cover/gate/routing at seed 7, 24 m).
- All five archetype fixtures gate-PASS at seed 7 / 8 m.
