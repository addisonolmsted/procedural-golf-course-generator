# Architecture

The map of this repo. **This file is updated every time a folder or file is
added to this branch.** Deep documentation lives in [docs/](docs/); the project
plan and milestones live in [PLAN.md](PLAN.md); where the pre-v2 code went is
in [MIGRATION.md](MIGRATION.md).

## The pipeline in one paragraph

A deterministic **12-stage** generator (S0–S11). A seed goes in; a complete,
playable 9-hole par-36 golf course comes out in under ten seconds,
byte-identically on every platform. S0 draws the biome, a categorical
structural class, and site descriptors from an offline-certified envelope; S1
lays down the site's predisposition and its structural discontinuities; S2 — a
fluvial skeleton engine — authors the drainage network and a base surface; **S3
reconstructs terrain texture from a dictionary of real lidar patches**; S4
re-derives hydrology from the finished surface and places water; S5 **selects
the 600 m play window** and compiles everything into the routing substrate; S6
routes nine holes; S7–S10 grade, lay out, detail, and dress them one hole at a
time so the first tee is playable while the rest finish; S11 measures and
reports. **Nothing rejects and nothing retries** — the rejecting happened
offline, in calibration, so runtime samples only from parameter space already
known to work.

Where realism comes from is the architecture's central claim:
`dist_to_channel_p50` measures 104–120 m in **every** real archetype, so
drainage spacing is a law of landscapes and not a signature. The skeleton
therefore cannot carry archetype identity — **texture does**, reconstructed
from real terrain at S3.

Full treatment, including the six decisions this architecture deliberately
reverses from its predecessor: [docs/00-architecture.md](docs/00-architecture.md).

## Repo layout (current)

```
ARCHITECTURE.md        — this file: repo map + cross-cutting invariants
PLAN.md                — milestones, claim protocol, status
MIGRATION.md           — where every pre-v2 module went
Cargo.toml             — Rust workspace (resolver 2, edition 2024)

docs/                  — the authoritative documentation
  00-architecture.md   — the 12 stages, the three ideas, where realism
                         comes from, plasticity, the six reversals
  01-conventions.md    — units, world geometry, resolution ladder,
                         determinism, artifacts, naming
  02-performance-budget.md — per-stage ms budgets, streaming order
  03-success-indicators.md — per-stage gates: plausibility, variety
                         (dispersion ratio), qualitative protocols P1-P5,
                         the golfability proxy, blocking-gate discipline
  04-terrain-workplan.md — the complete work breakdown to terrain-complete:
                         phases A-H, three parallel tracks, stage-lab tabs,
                         the dependency spine
  contracts/           — C0 (delivery to the frontend team), C1
                         (primitives→kernel), C2 (routing substrate),
                         C3 (routing→realization); the four guarded seams
  stages/              — stage-00…11; one standalone doc per stage
  biomes/              — the six Heartland records, config-not-code,
                         and the future-pack index
  calibration/         — lidar pipeline, 22-metric battery, target
                         vectors, envelope certification (all OFFLINE)

crates/
  course-seed/         — determinism kit: DetRng, stream registry,
                         RunIdentity. Shared substrate, reused unchanged
  course-world/        — Grid/GridSpec + CGRID1, world constants,
                         Vec2/libm math, ease, Profile, Spine, noise
  course-viz/          — Grid→PNG renderers (hillshade, hypsometric,
                         scalar/direction fields, overlays)
  course-contracts/    — C0/C1/C2/C3 + units, metadata, plasticity, biome.
                         The v2 vocabulary; deliberately does NOT restate
                         grid or seed types
  course-spec/         — S0: biome + structural class + descriptor draw
                         → spec.json (contents pre-v2; see MIGRATION.md)
  course-primitives/   — S1: macro structure + discontinuities → C1
  course-skeleton/     — S2: the fluvial engine + five modules; Horton
                         hierarchy; the SkeletonKernel future-biome seam
  course-amplify/      — S3: the patch dictionary — texture and archetype
                         identity reconstructed from real lidar residuals,
                         conditioned on skeleton position
  course-transforms/   — S4: flow re-derivation, datum ops, water, basins;
                         glacial + boundary-retreat seams (empty)
  course-substrate/    — S5: site selection (600 m play window) + assembles C2
  course-routing/      — S6: beam + anneal → C3
  course-earthmoving/  — S7: grading, borrow, mass balance, bounded repair
  course-layout/       — S8: the ten-pass plan/surface hole layout
  course-micro/        — S9: oriented band-limited detail + surface fields
  course-zoning/       — S10: cover mosaic, materials, palette, placements
  course-validate/     — S11: playability, sightlines, difficulty, perf, goldens
  course-calibration/  — OFFLINE: ingest, battery, gates, fitting, envelopes
  course-cli/          — headless dev driver
  course-framing/      — v1 stage 01; prior art for S1/S2, retired when
                         the skeleton kernel lands (MIGRATION.md)
  stage-lab/           — per-stage viewer shell (egui) + headless snapshots
  tile-lab/            — campaign-data QA viewer

tools/                 — Python: the data campaign + terrain analysis stack
  spike/               — M3.5 dictionary-spike workspace: smoke.py, spike.py
                         (the G-SPIKE experiment), textures.py, measure.py
                         (the tile-battery runner: any CGRID → battery +
                         residual battery + proxy, JSONL)
  golf_proxy/          — the golfability proxy: thresholds measured from the
                         64 real course grids + the window scorer
  metrics/             — the pure f(height, cell, mask) battery kernels
  dtm_metrics/         — metric admission gates (G1–G6) + the Sobol/HistGBT
                         calibration sandbox
  dtm_atlas/           — USGS 3DEP DTM store, OSM/NHD masks, QA
  macro_campaign/      — corpus fetch/screen/compare/report + CGRID1 mirror
  tile_scout/          — F0/F1/F2 tile selection cascade
  dtm_primitives/      — primitive extraction; promoted to a dependency
                         (skeletonizes real tiles for the dictionary)
  parkland_atlas/      — the v1 200-course collector (recovered from main;
                         cache regeneration optional — see MIGRATION.md)
  landform_prior/      — retired fitting pattern (reference only)

assets/
  atlas.bin            — packed 200-course parkland atlas (from main;
                         golf-atlas/src/binfmt.rs is the format)
```

## The separation principle

Each stage is an **isolated module behind a versioned artifact contract**:

1. A stage's only interface is: read its input artifacts → write its output
   artifact. Data shapes live in the stage docs; nothing crosses a stage
   boundary that is not in a contract.
2. A stage may be overhauled or replaced without touching any other stage, so
   long as its output contract is honored. Contract changes are cross-stage
   events: update the affected docs and this file, bump the artifact version
   and `PIPELINE_VERSION`, re-bless goldens.
3. Stages never import each other's internals and are testable in isolation
   against fixture artifacts before their upstream neighbours exist.
4. Every stage doc has the same skeleton, so an agent can own one doc and one
   module without reading the others.

The four seams in [docs/contracts/](docs/contracts/) are the *guarded* ones —
golden-tested and stable while everything between them churns. **C2 is the
waist**: S6 onward may see nothing except C2, and `course-routing` depending on
anything upstream of it is a build failure, not a style problem. **C0 is the
product**: the delivery bundle for the frontend team — plain axis-aligned
grids, self-describing manifest, one prevailing-wind vector shared between
terrain sculpting and gameplay.

## World geometry (fixed, cross-stage)

Defined in [`crates/course-world/src/world.rs`](crates/course-world/src/world.rs)
— import these, never redefine them.

- **3 km × 3 km**, origin SW, x east, y north, metres (`EXTENT_M`). No CRS: a
  local Cartesian frame, with elevations in a local datum where 0 is a nominal
  core reference, not sea level.
- **Core**: the central 1.5 km × 1.5 km window `[750, 2250]²` (`CORE_MIN_M`,
  `CORE_MAX_M`, `in_core`). The **search region** for site selection; the
  margin exists so landforms enter and leave the frame instead of terminating
  at it.
- **Play window**: a **600 m axis-aligned square** (`PLAY_M`) chosen inside the
  core, centre free ±450 m. Holes route inside *this*. The arithmetic closes
  exactly — 600 ± 450 spans the core — so no world constant moves and the
  **750 m terrain margin** holds by construction.
- **Resolution ladder**: 8 m preview/search (`RES_PREVIEW_M`, 376²), **2 m
  canonical** (`RES_FULL_M`, 1501²), 0.5 m corridor-local
  (`RES_EARTHWORKS_M`). Node convention `nx = extent/res + 1`, so coarse node
  sets **nest** inside fine ones and coarsening is subsampling, not resampling.

Details and the three ladder rules: [docs/01-conventions.md](docs/01-conventions.md).

## Cross-cutting invariants

1. **Determinism.** One master `seed: u64`. Same seed + params + resolution ⇒
   byte-identical artifacts on every platform. All randomness through
   `RunIdentity::stream(name)` with a name registered in
   [`crates/course-seed/src/streams.rs`](crates/course-seed/src/streams.rs)
   (test-enforced). No wall clock, no `HashMap` iteration order, no platform
   intrinsics, no nondeterministic parallel float reduction; `libm` for
   transcendentals.

   **No retry-forked RNG.** v2 never retries, so no stream may advance
   conditionally on a rejection: draw counts are a function of the parameters
   alone, never of the data. "Keep drawing until the point lands outside water"
   is forbidden — draw a fixed count and resolve collisions deterministically.
   This is the rule most likely to be violated by accident.

   **Registry v3** (`PIPELINE_VERSION` 4, the v2-pipeline rekey, 2026-08-07).
   v2 never retries, so **every stream is `stable`** (master-seed keyed); the
   scope machinery is vestigial-but-retained — see invariant 2. The retired
   v1 streams (`mask/v1`, `strokes/v1`, `forcing/v1`) had no v2 owner;
   retained names kept their exact spellings, so attempt-0 draws are
   bit-identical across the bump and only artifact version headers moved.

   | Stream | Scope | Owner |
   |---|---|---|
   | `arch/select/v1` | stable | S0 biome draw |
   | `arch/params/v1` | stable | S0 descriptor sampling |
   | `framing/v1` | stable | v1 stage 01 (retained with `course-framing`; retires with it) |
   | `primitives/v1` | stable | S1 macro structure |
   | `skeleton/carve/v2` | stable | S2 erosion roughness seed |
   | `skeleton/module/v1` | stable | S2 structural modules |
   | `amplify/v1` | stable | S3 amplification |
   | `hydro/v1` | stable | S4 hydrology & transforms |
   | `substrate/v1` | stable | S5 siting + substrate |
   | `route/v1` | stable | S6 routing search |
   | `earthworks/v1` | stable | S7 earthworks jitter |
   | `layout/v1` | stable | S8 hole layout |
   | `micro/v1` | stable | S9 micro noise |
   | `cover/v1` | stable | S10 cover clumping |
   | `zoning/v1` | stable | S10 palette/exaggeration draws |
   | `placement/v1` | stable | S10 placement jitter |
   | `validate/v1` | stable | S11 playability-sim dispersion |
   | `reroll/v1` | stable | INTERNAL reroll sub-seeds (vestigial) |
   | `fixture/v1` | stable | tests and fixtures only |

   This table is **test-enforced** to mirror
   [`crates/course-seed/src/streams.rs`](crates/course-seed/src/streams.rs).

2. **No runtime rejection.** No gates, no reroll, no unbounded loops, no
   convergence criteria. Every iterative stage takes a fixed count from config.
   Safe only because
   [offline calibration](docs/calibration/envelope-certification.md) certifies
   the parameter envelope first. **A bad course is a calibration bug, not a
   seed to discard.**

   `course-seed`'s `MAX_ATTEMPTS` and `Scope::Attempt` machinery is therefore
   **vestigial** — attempt is pinned at 0. It is retained deliberately;
   removing it would rekey every stream for no behavioural gain.

3. **Versioned artifacts.** Every artifact carries a version; shape changes bump
   it plus `PIPELINE_VERSION`, update the stage doc, and re-bless goldens — an
   explicit reviewed event. `serde_json` runs with `float_roundtrip` so f64 JSON
   round-trips byte-identically.

4. **Artifacts are files.** JSON for structure, CGRID1
   (`course-world::gridio`, mirrored in Python by
   `tools/macro_campaign/macro_campaign/cgrid.py`) for per-cell fields; a
   per-course manifest with blake3 hashes and the embedded `RunIdentity`. Loud
   load validation — a stale or malformed artifact fails with a specific error,
   never a silent default.

5. **The empty case is legal.** Sandhills has no surface water and no channels;
   Great Plains often has no water bodies; any biome may run any module at zero.
   Every consumer must be correct on empty collections. This is a hard
   requirement and the failure mode most likely to survive testing on piedmont
   and die on sandhills.

6. **The biome is data, not code.** Stages never branch per biome — there is no
   `if piedmont` anywhere in the pipeline, and adding a biome adds no stage
   code. A biome that does not use a module **runs it at zero intensity**
   rather than skipping it. Dials are named for what they do, never for the
   biome that uses them: `drainage_integration`, not `heathland_derangement`.
   See [docs/biomes/README.md](docs/biomes/README.md).

7. **Amplification decorates the skeleton; it does not replace it.** S2 authors
   the drainage network; S3 reconstructs texture over it with residual
   amplitude **tapering to zero inside channel corridors**. Because S4 then
   re-derives hydrology from the finished surface, small movements are absorbed
   rather than forbidden — the constraint is constructive, not policed. See
   [docs/stages/stage-03-amplification.md](docs/stages/stage-03-amplification.md).

8. **The course is sited, not centred.** The core is the *search region*; the
   routable area is a **600 m axis-aligned play window** chosen inside it, its
   centre free ±450 m. Guarantees a **750 m minimum terrain margin** beyond any
   played point — a hole at the edge of play must still have ground running out
   past it. Translation only; deliverable heightmaps stay axis-aligned with no
   transform attached.

## The six biomes ("Heartland")

| Key | Identity | Water | Relief | Canopy | Plasticity |
|---|---|---|---|---|---|
| [`piedmont`](docs/biomes/piedmont.md) | rolling fluvial, creek corridors | streams | mid | closed | mid |
| [`great_plains`](docs/biomes/great-plains.md) | flat, caprock-held, big sky | little/none | low | open | mid-high |
| [`river_valley`](docs/biomes/river-valley.md) | floodplain + terrace flight | river | low-mid | mixed | mid |
| [`sandhills`](docs/biomes/sandhills.md) | aeolian dune trains | **none** | mid | open | low |
| [`heathland`](docs/biomes/heathland.md) | deranged drainage, kettles | ponds/bogs | low-mid | open | low-mid |
| [`hill_country`](docs/biomes/hill-country.md) | benched, stratified, steep | streams | high | mixed | low |

All six are served by one fluvial kernel plus five modules. They span
no-water → water-dominant, flat → steep, open → closed canopy, and — the axis
that matters most — **integrated → deranged drainage**: sandhills (no network)
and heathland (a disconnected one) break the engine's central assumption from
opposite directions.

**Golden archetypes:** piedmont (primary), heathland (adversarial). Sandhills is
a strong third candidate. **Lowcountry Marsh** (future, Coastal pack) resumes
the adversarial contract-tester role for C2 when it lands.

## Reference material

- **[MIGRATION.md](MIGRATION.md)** — the authoritative map of where every pre-v2
  module went, and the retirement schedule.
- **Commit `0313432`** — the step-03-era authored-primitive generator and its
  measured failure (22/71 knobs inside the real-tile IQR). Read before
  implementing S2; it is the record of what authoring-in-landform-space costs.
- **`main` branch** — `tools/calibration/` (a complete working harness for the
  v2 calibration loop), `golf-terrain/src/erosion.rs`, `golf-routing/`,
  `golf-holes/`, `xtask`.
- **`pipeline` branch** — the immediate predecessor: the v1 14-stage
  sim-co-authored architecture and its stage docs.
