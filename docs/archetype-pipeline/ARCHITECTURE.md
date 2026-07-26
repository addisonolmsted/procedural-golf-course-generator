# Archetype Pipeline — Architecture & Stage Contracts

This is the working contract for the archetype-pipeline rebuild (branch
`archetype-pipeline`). It exists so that **each pipeline stage can be built by a
different agent in parallel**: if you are implementing a stage, this document
plus the `course-contracts` crate is your interface — you should not need to
read (or wait for) any other stage's crate.

Current stage status lives in [STATUS.md](STATUS.md). Read that first to see
what is claimed/done.

## 1. What we are building

A deterministic 10-stage generator for 9-hole (par-36) golf-course terrain:

| # | Stage | Crate | Output artifact |
|---|-------|-------|-----------------|
| 1 | Seed | `course-seed` | (folded into 2) |
| 2 | Archetype select + param sampling | `course-seed` | `CourseSpec` |
| 3 | Macro landform | `course-macro` | `MacroSkeleton` |
| 4 | Conditioned noise | `course-noise` | `ComposedHeight` |
| 5 | Erosion + hydrographic extraction | `course-hydro` | `HydroBundle` |
| 6 | Cover assignment | `course-cover` | `CoverMap` |
| 7 | Routability gate | `course-gate` | `GateReport` |
| 8 | Routing | `course-route` | `RoutingPlan` |
| 9 | Hole earthmoving | `course-earthworks` | `EarthworksDelta` |
| 10 | Drainage validation/repair | `course-drainage` | `DrainageReport` |

Plus: `course-pipeline` (orchestrator CLI, later milestone), `course-lab`
(egui GUI, one tab per stage), and `tools/archetype_atlas/` (Python labeling +
prior-fitting campaign).

**World geometry** (constants in `course-contracts/src/lib.rs`):
3 km × 3 km box, origin SW, y up. Routable core = central 1.5 km × 1.5 km
(`[750, 2250]²` m). All 9 holes must fit in the core; the outer ring is
scenery and hydrological context. Canonical grid resolution 2 m
(`RES_FULL_M`, 1501×1501 nodes), preview 8 m; every stage takes `res_m` and
must be resolution-consistent (same code path at any resolution — no
baked-in cell counts). Stage 9/10 patches are local grids at 0.5 m.

## 2. The five archetypes

`ArchetypeId` (in `course-contracts/src/archetype.rs`) is carried end to end
and is the art layer's lookup key. Archetypes differ by **parameters** (the
prior) and one **mode switch** (`HydrologyMode`), never by per-archetype code
branches in stage crates — if you find yourself writing
`if archetype == Sandhills`, you are missing a parameter.

| Archetype | key | HydrologyMode | Terrain identity |
|---|---|---|---|
| Sandhills | `sandhills` | `Infiltrated` | aeolian dune trains, high infiltration ⇒ **empty hydro graph** |
| Piedmont | `piedmont` | `Fluvial` | rolling + creek corridor, clay, high canopy |
| Florida lowland | `florida_lowland` | `WaterTable` | near-zero relief, water table at surface, lakes primary |
| Glacial moraine | `glacial_moraine` | `Kettle` | closed depressions preserved, mid relief, open canopy |
| Mountain bench | `mountain_bench` | `Alpine` | high relief, benches/scarps, hardest gate |

They deliberately span no-water → all-water, flat → steep, closed → open
canopy: if a contract survives all five, it survives.

## 3. Non-negotiable invariants

1. **Determinism.** One master `seed: u64`. Every random draw goes through
   `course_contracts::rng::channel(seed, CH_*)` with a channel registered in
   `rng.rs` (`cp/<stage>/<purpose>/v1`). Never share a stream across
   purposes; never draw in a data-dependent order that can change under
   refactoring. Same seed + params + res ⇒ byte-identical output, verified by
   a double-run test in every stage crate and the `course-fixtures` golden
   (`cargo run -p xtask -- golden`).
2. **Pure stages.** A stage is `run(inputs, spec, res_m) -> Output` — no
   global state, no file IO inside the stage (the store handles IO), no
   wall-clock, no threads with nondeterministic reduction order.
3. **Contract types only at boundaries.** Anything crossing a stage boundary
   is defined in `course-contracts` (`spec.rs`, `stages.rs`). Add private
   types freely inside your crate.
4. **The empty case is legal.** `HydroGraph` with zero streams/lakes/wetlands
   is a *valid, expected* stage-5 output (sandhills). Every consumer must
   handle it.
5. **No dams.** Stage 4 must damp noise by `SkeletonFields::floor_damp`
   (terrain-v2's hard-won lesson: un-damped noise dams drainage floors).
   Stage 3's `Drain` spines must fall monotonically. Stage 9 must not raise
   ground across a `HydroGraph` stream.
6. **Core protection.** Stage 3 caps relief inside the core
   (`landform.core_relief_cap_m`); stage 6 keeps the core more open than the
   ring. The gate (stage 7) is the enforcement point, but upstream stages are
   the reason it passes.
7. **Versioning.** Each artifact has `Artifact::VERSION`; the pipeline has
   `PIPELINE_VERSION`. Any shape change bumps both the artifact version and
   the pipeline version, updates this doc, and re-blesses goldens — as an
   explicit, reviewed step.

## 4. Artifact store

Implemented in `course-contracts/src/artifact.rs` + `gridio.rs`; you get it
for free by implementing nothing — all contract types already have `Artifact`
impls.

- Directory: `output/courses/<slug>/` where `slug = format!("{seed:016x}-{archetype}")`.
- Each artifact = `stage{NN}_{kind}.json` + zero or more
  `stage{NN}_{kind}.{field}.cgrid` binary sidecars (`CGRID1` format: magic,
  dtype f32/u32/u8, GridSpec header, row-major LE payload).
- `manifest.json` records `{stage, version, files, blake3}` per artifact —
  the staleness/provenance check used by the lab and orchestrator.
- f32 sidecar quantization is below any pipeline sensitivity; golden hashes
  are taken over in-memory f64 fields, never after a store round-trip.

## 5. Stage contracts

Type definitions are authoritative in `course-contracts` — this section adds
the semantic requirements that types can't express. "Reference:" lines point
at legacy code to READ for algorithms (do not import those crates).

### Stage 1–2 — `course-seed` → `CourseSpec`
- In: master seed, optional forced archetype + param overrides (lab sliders).
- Draw archetype from `CH_ARCHETYPE` (equal weights for now), then sample
  every knob of `ParamPack` from `ArchetypePriors` quantile tables via
  `CH_PARAMS` (independent draws now; copula when the fitted prior lands).
  `Quantiles::sample(u)` is already implemented.
- The committed prior `course-contracts/data/archetype_priors.json` is a
  **placeholder** (schema-final, values hand-authored). The Python campaign
  replaces the data file only. Fold `ArchetypePriors::fingerprint()` into any
  golden you add.
- `route` and `gate` sections are fixed switches, not sampled.

### Stage 3 — `course-macro` → `MacroSkeleton`
- In: `CourseSpec`, `res_m`.
- Author archetype-vocabulary primitives from `landform` params: ridges,
  drainage spines, benches/scarps (`bench_count`), closed basins
  (`basin_count`), dune trains (`dune_wavelength_m`), regional tilt. Analytic
  or rasterized internally — but output `base_height` + the five
  `SkeletonFields` + `StructureGraph`.
- Hard requirements:
  - `Drain` spines: monotone falling `base_height` along the polyline,
    accordant junctions (tributary floor meets mainstem floor at equal
    elevation), and at least one spine must exit the box (except
    closed-basin-only configs, e.g. florida/moraine where `valley_count = 0`).
  - Core cap: relief inside the core ≤ `core_relief_cap_m`; benches must be
    oriented so at least one bench system passes THROUGH the core
    (mountain's gate-pass lever).
  - `floor_damp` = 1 on drainage floors → 0 by ~150 m out; `core_protect`
    = 1 in core with a smooth shoulder; `grain_dir_rad` is the local
    structural grain (undirected, mod π).
- Reference: `golf-landform/src/valley.rs` (monotone floors, junctions),
  `golf-landform/src/presets.rs` (primitive vocabulary).

### Stage 4 — `course-noise` → `ComposedHeight`
- In: `MacroSkeleton`, `CourseSpec`, `res_m`. RNG: `CH_NOISE_FIELD`.
- fBm/ridged stack per `noise` params, modulated by the skeleton fields:
  amplitude × `(1 − floor_damp·p.floor_damp)` × `(1 + slope_gain·p.slope_gain)`,
  anisotropy `aniso_ratio` aligned to `grain_dir_rad` by `grain_align`.
  Sandhills dune trains = high aniso + `dune_wavelength_m` carrier along the
  grain.
- Must add zero mean drift (no net raising of drainage floors) and preserve
  spine monotonicity within a stated tolerance (test it).
- Reference: `golf-landform/src/noiselab.rs` (the KEEP-7 parameter set and
  skeleton modulation), `golf-terrain/src/noise.rs`.

### Stage 5 — `course-hydro` → `HydroBundle`
- In: `ComposedHeight`, `CourseSpec`, `res_m`. RNG: `CH_HYDRO` (prefer none —
  legacy erosion is RNG-free).
- Erode: priority-flood depression routing (ε-variant), stream-power incision,
  graded deposition with an exact mass ledger, hillslope diffusion, talus
  guard. Scale forcing by `runoff = rainfall × (1 − infiltration)`;
  erodibility from `hydro.erodibility`.
- Mode semantics (parameters, not branches):
  - `Infiltrated`: runoff ≈ 0 ⇒ negligible incision, **empty graph**.
  - `WaterTable`: build a water-table surface at `water_table_depth_m` below
    the terrain reference; cells below it are lakes/wetlands; do NOT breach
    closed basins.
  - `Kettle`: preserve depressions per `depression_keep` (do not fill/breach
    below the retention threshold); mark retained pits in `FlowModel` with
    `rcv[i] == i`.
  - `Fluvial`/`Alpine`: full erosion; alpine = high erodibility + talus.
- Extract: streams above `channel_threshold_ha` (effective area), Strahler
  order, hydraulic-geometry widths; lakes with surface elevation; HAND-style
  wetlands. Populate `FlowModel { rcv, order, zfill, flow_area_m2 }` exactly
  as documented in `stages.rs` (fixed NBR scan order, documented tie-breaks).
- Reference: `golf-terrain/src/erosion.rs` (READ THIS FIRST — routing
  epochs, fkey heap ordering, mass ledger), `golf-terrain/src/water.rs`
  (entity extraction, HAND wetlands).

### Stage 6 — `course-cover` → `CoverMap`
- In: `HydroBundle`, `CourseSpec`, `res_m`. RNG: `CH_COVER_CLUMP`.
- Classes from moisture proxies (HAND, slope, curvature, water adjacency) +
  `cover` params, clumped at `clump_scale_m`. Canopy density independent of
  class. Keep the core more open than the ring (reduced forest/wetland
  propensity under `core_protect`-like weighting — recompute from
  `in_core`, or accept the skeleton as an optional input later).
- `CoverClass::routable()` is the gate's definition of routable ground — if
  you think it's wrong, change it HERE (contract change), not in the gate.

### Stage 7 — `course-gate` → `GateReport`
- In: `HydroBundle`, `CoverMap`, `CourseSpec`. Pure analysis; MUST NOT mutate
  terrain. Deterministic, no RNG.
- Core-window metrics (published names, keep stable):
  `core_slope_lt8_frac`, `core_routable_patch_ha`,
  `core_water_blockage_frac`, `core_stream_crossings`, `corridor_need_ha`,
  plus a real corridor-feasibility check (greedy placement of 9 corridors of
  par-36 lengths ≈ 2×470 m, 5×360 m, 2×155 m × ~55 m wide inside the largest
  routable patch graph — the fixture only approximates this by area).
- Thresholds from `gate` params (per-archetype). `reasons` non-empty iff
  fail. The orchestrator rerolls on fail via `CH_REROLL` (bounded attempts,
  attrition reported).

### Stage 8 — `course-route` → `RoutingPlan`
- In: gate-passed `HydroBundle` + `CoverMap` + `CourseSpec`. RNG: `CH_ROUTE`.
- 9 holes, pars = `route.par_sequence` (sum 36), returning nine: hole 1 tee
  and hole 9 green adjacent to a shared clubhouse site. All corridors inside
  the core; minimize forced crossings (≤ gate's `crossings_max`); respect
  `CoverClass::routable` + slope; greens not in water/wetland.
- Suggested shape: site scoring → sequence construction with backtracking →
  simulated-annealing polish.
- Reference: `golf-routing/src/{fields,sequence,construct,anneal}.rs`
  (algorithm ideas only — types are v1).

### Stage 9 — `course-earthworks` → `EarthworksDelta`
- In: `RoutingPlan`, `HydroBundle`, `CourseSpec`. RNG: `CH_EARTHWORKS`.
- Tee pads, green complexes + surrounds, bunkers, fairway softening as local
  0.5 m patches. Patch edges MUST feather to exactly 0 (tested). Sum of
  |cut|+|fill| ≤ `cut_fill_budget_m3`. Never raise ground across a stream
  polyline.
- `EarthworksDelta::height_at(base, p)` is the composition rule (base +
  overlapping patches) — final terrain everywhere.

### Stage 10 — `course-drainage` → `DrainageReport`
- In: composed final height (stage 4/5 base + stage 9 patches),
  `RoutingPlan`, `HydroGraph`.
- Re-run flow on the composed terrain; detect NEW ponding on playing
  surfaces (corridors/greens/tees) and water impounded against earthworks;
  repair by carving bounded swale patches (`repair_patches`, same feather
  rules); re-validate; bounded iterations; report metrics either way.

## 6. Fixtures — how to build your stage before its inputs exist

`course_contracts::fixtures` provides deterministic stand-ins for every
artifact (`fixture_spec`, `fixture_skeleton`, `fixture_height`,
`fixture_hydro`, `fixture_cover`, `fixture_gate`, `fixture_routing`,
`fixture_earthworks`, `fixture_drainage`, and `run_fixture_pipeline`).

Workflow for a stage agent:
1. Take your inputs from fixtures (e.g. course-cover consumes
   `fixture_hydro(...)`).
2. Build + test your crate against them (all five archetypes, ≥2 seeds,
   preview + a coarse res).
3. When the real upstream crate lands, switch the lab/orchestrator wiring —
   your crate does not change.

Fixtures are contract-faithful but crude; never import them from a real
stage crate, and never "fix" a fixture to make your stage pass — fix the
stage or escalate the contract question.

## 7. course-lab

`cargo run -p course-lab --release`. One tab per stage, seed scrub, archetype
selector, fast/full res toggle, contours; side panel shows the stage's
JSON/metrics. Currently 100% fixture-backed; as each real stage crate lands,
replace its `fixture_*` call in `course-lab/src/main.rs` (one line each) —
keep the fixture path available as a fallback toggle until M3.

Headless twin: `cargo run -p xtask --release -- course-dump [seed] [res_m]`
renders terrain+hydro / cover / routing PNGs per archetype into
`output/course_fixtures/`.

## 8. Verification requirements (every stage crate)

- Unit tests for the stage's hard requirements (listed per stage above).
- A byte-identical double-run test.
- A golden field hash pinned in the crate (and wired into `xtask golden` /
  `bless` when the stage joins the composed pipeline).
- All five archetypes exercised; the sandhills empty-hydro case explicitly.
- `cargo test -p <crate>` green and `cargo run -p xtask -- golden` green
  before merging.

## 9. Parallel work assignment

Ready NOW (independent, in any order):
- `course-seed` (small), `course-macro`, `course-noise` (against
  `fixture_skeleton`), `course-hydro` (against `fixture_height` — the
  critical path; start first), `course-cover`, `course-gate`,
  `tools/archetype_atlas` (fully independent Python).

Second wave (needs stable gate/cover semantics, still fixture-drivable):
- `course-route`, then `course-earthworks`, `course-drainage`,
  `course-pipeline` orchestrator.

Coordination rules: claim a stage in STATUS.md before starting; contract
changes go through `course-contracts` + this doc + `PIPELINE_VERSION` bump +
golden re-bless, never through silent divergence.

## 10. Data campaign (`tools/archetype_atlas`, Python)

1. **Label** the 200 atlas courses (135 with 1 m lidar in
   `tools/dtm_atlas/out/store`) into the five archetypes: `bbox` centroid
   geography + relief band + `tools/metrics` 49-scalar vectors +
   `tools/tile_scout/out/archetypes.json` GMM responsibilities; YAML manual
   overrides; `none` is a valid label. Output `out/labels.parquet` + review
   HTML.
2. **Gap collection** for thin archetypes (sandhills ≈ pinehurst2/streamsong;
   moraine ≈ erin/moraine today): curated new course keys through the
   existing `dtm_atlas` fetch machinery.
3. **Fit** per-archetype quantile tables (+ later copula) for every
   `ParamPack` knob from `dtm_primitives`/`dtm_metrics` extractions →
   emit `archetype_priors.json` (same schema as the placeholder), plus
   per-archetype hydro/cover calibration targets (drainage density, lake %,
   depression density, canopy %) as stage-5/6 acceptance checks.
   Reference: `tools/landform_prior/landform_prior/fit.py`.

## 11. Milestones

- **M0 (done)** — branch, contracts + fixtures + placeholder prior, lab
  shell, goldens, this doc.
- **M1** — Piedmont end-to-end real through stage 7 (fluvial general case);
  8–10 fixture-backed.
- **M2** — all five archetypes gate-passing ≥ ~70% (mountain may be lower);
  fitted prior v1 replaces the placeholder.
- **M3** — routing/earthworks/drainage real; full 10-stage end-to-end.
- **M4** — batch attrition harness + metric-space validation of generated
  terrain vs the labeled atlas cohort (`tools/dtm_metrics`).
