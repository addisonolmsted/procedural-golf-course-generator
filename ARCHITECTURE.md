# Architecture

The general architecture of the golf-terrain pipeline project. **This file is
updated every time a folder or file is added to this branch** — it is the map
of the repo. Contracts and requirements for individual pipeline steps live in
their own docs under `steps/`; the project plan and phase overview live in
[PLAN.md](PLAN.md).

## Repo layout (current)

```
ARCHITECTURE.md      — this file: repo map + cross-cutting invariants
PLAN.md              — project goal, phases, milestones, parallelization protocol
steps/01-seed.md     — one contract doc per pipeline step (01…10);
…                      each step's owning agent builds out that step and
steps/10-drainage.md   keeps its doc current
Cargo.toml           — Rust workspace root (resolver 2, edition 2024;
                       dev/test profiles build at opt-level 2 — the macro
                       raster is ~30x slower unoptimized)
crates/course-seed/  — step 01: DetRng determinism kit, stream registry,
                       RunIdentity, reroll rule — the determinism substrate
                       every step crate depends on
crates/course-spec/  — step 02: archetype selection + prior sampling →
                       spec.json (CourseSpec); data/archetype_priors.json is
                       the committed, fingerprinted prior (the knob registry
                       — adding a knob is a data edit + golden re-bless;
                       examples/bless_golden.rs regenerates the golden)
crates/course-world/ — P1 substrate: Grid/GridSpec (+bilinear), Vec2 + libm
                       math, ease/noise, Profile, arc-length Spine, the
                       CGRID1 binary grid format, world constants
                       (EXTENT/CORE/RES, world_spec) — ported from
                       terrain-v2:golf-core + golf-landform geometry +
                       archetype-pipeline:course-contracts. No RNG here.
crates/course-macro/ — step 03: macro landform. prims/ (Valley/Ridge/Bluff/
                       Bowl/Meander analytic engine, ported verbatim from
                       terrain-v2:golf-landform), planner (params →
                       MacroPlan staged IR → MacroConfig; all macro/place/v1
                       draws), budget (core-relief cap solve: staged
                       bisections on core-localized knobs — never the box),
                       raster+fields (analytic,
                       resolution-consistent), artifact (macro_skeleton/
                       dir: contract grids+structure + advisory plan/config
                       sidecars)
crates/course-viz/   — shared render helpers (hypsometric×hillshade,
                       scalar/direction fields, NaN-tolerant terrain render,
                       alpha-blended class-mask + axis-band overlays) for
                       step viewers; from terrain-v2:golf-viz, no pipeline deps
crates/macro-lab/    — the STEP-03-ONLY viewer (per-construction-stage tabs,
                       seed scrub, knob overrides); later steps build their
                       own viewers on course-viz
crates/tile-lab/     — the CAMPAIGN-DATA viewer: real DTM tiles with the
                       extractor's landform classifications overlaid, knobs
                       vs the committed quantiles, and the QA cull that
                       writes out/exclude.json (read-only over campaign
                       artifacts — it measures nothing itself)
tools/               — the data-campaign Python stack: dtm_atlas,
                       dtm_primitives, landform_prior, tile_scout,
                       dtm_metrics, metrics (ported from terrain-v2) +
                       macro_campaign (step-03 archetype exemplar landscape
                       tiles — 3DEP fetch → OSM development screen → knob
                       extraction → shape-tier transect fits → prior quantile
                       fit → generated-vs-real compare; out/ is local-only.
                       Exemplar centers must be protected/unpopulated land;
                       `develop` measures and auto-culls the rest)
```

The determinism substrate and pipeline steps are Rust; the data campaign is
Python — the contracts themselves stay language-agnostic (data shapes,
requirements, file formats). CGRID1 (`crates/course-world/src/gridio.rs`,
mirrored by `tools/macro_campaign/macro_campaign/cgrid.py`) is now the fixed
grid sidecar format of invariant 3.

## The separation principle (the reason this branch exists)

Each of the 10 pipeline steps is an **isolated module behind a versioned
artifact contract**:

1. A step's only interface is: read its input artifacts → write its output
   artifact. Data shapes are defined in the step docs; nothing crosses a step
   boundary that is not in a contract.
2. A step may be overhauled or fully replaced without touching any other
   step, so long as its output contract is honored. If the contract itself
   must change, that is a cross-step event: update the affected step docs +
   this file, and bump the artifact's version.
3. Steps therefore never import each other's internals, never share hidden
   state, and are testable in isolation against **fixture artifacts**
   (cheap stand-ins for their inputs) before their upstream neighbors exist.
4. Every step doc has the same skeleton (Purpose / Position / Contract /
   Per-archetype behavior / Hard requirements / References / Open questions /
   Status), so an agent can own one doc and one module without reading the
   others.

## World geometry (fixed, cross-step)

- Generated terrain: **3 km × 3 km**, origin at the SW corner, x east, y
  north, elevations in metres.
- **Routable core**: the central **1.5 km × 1.5 km** window `[750, 2250]²` m.
  All 9 holes are routed inside the core; the outer ring is scenery and
  hydrological context. Upstream steps are designed to keep the core routable
  (see steps 03, 06, 07).
- Working heightfield resolution: **2 m** canonical (1501×1501 node grid),
  with a coarser preview mode (8 m) — every step must be
  resolution-consistent: same code path, any cell size, no baked-in counts.
- Hole-construction edits (steps 09–10) are local patches at **0.5 m**.

## Cross-cutting invariants

1. **Determinism.** One master `seed: u64`. Same seed + same params + same
   resolution ⇒ byte-identical artifacts, on every platform. All randomness
   is drawn from domain-split streams named per step and purpose
   (`<step>/<purpose>/v1`), opened only via `RunIdentity::stream(name)`.
   No wall-clock, no unordered parallel reductions, no data-dependent draw
   order. The stream registry (source of truth:
   `crates/course-seed/src/streams.rs`; this table is test-enforced to
   mirror it — adding a stream is a doc+code+review event):

   | Stream | Owner | Purpose |
   |---|---|---|
   | `arch/select/v1` | step 02 | archetype draw (when not forced) |
   | `arch/params/v1` | step 02 | prior quantile sampling |
   | `macro/place/v1` | step 03 | primitive placement (counts, frames, phases) |
   | `noise/field/v1` | step 04 | noise-field lattice offsets |
   | `hydro/v1` | step 05 | stochastic erosion/hydrology components (if any) |
   | `cover/clump/v1` | step 06 | cover clumping |
   | `route/v1` | step 08 | routing search |
   | `earthworks/v1` | step 09 | earthworks jitter (bunker shapes etc.) |
   | `reroll/v1` | step 01 | INTERNAL: gate-fail reroll sub-seeds (master-seed keyed) |
   | `fixture/v1` | fixtures | tests/fixtures only, never real steps |

   Steps 07 and 10 are RNG-free by contract.
2. **Versioned artifacts.** Every artifact carries a version; any shape
   change bumps it and the global pipeline version, updates the step doc, and
   is an explicit reviewed event.
3. **Artifacts are files.** Each step's output is serializable: structured
   data as JSON, per-cell fields as flat binary grids with a small header
   (spec to be fixed when the store is built; prior art:
   `archetype-pipeline:course-contracts/src/gridio.rs`). A per-course
   manifest with content hashes makes staleness and provenance checkable.
4. **The empty case is legal.** Collections in artifacts may be empty where
   the archetype implies it (a sandhills course has zero surface water);
   every consumer must be correct on empty inputs.
5. **The archetype is data, not code.** Steps read archetype-dependent
   *parameters* (sampled in step 02) and at most one *mode switch*; a step
   implementation must not branch on the archetype id. If you need
   `if sandhills`, you are missing a parameter.

## The five archetypes

Selected in step 02, carried end-to-end, and the key the art layer uses:

| Key | Identity | Water | Relief | Canopy |
|---|---|---|---|---|
| `sandhills` | aeolian dune trains, high infiltration | none (surface) | low-mid | open |
| `piedmont` | rolling fluvial + creek corridor, clay | streams | mid | closed |
| `florida_lowland` | water table at surface, lakes primary | dominant | ~zero | mid |
| `glacial_moraine` | kettled moraine / heathland | ponds/bogs | mid | open |
| `mountain_bench` | benches, scarps, terraces | mountain streams | high | mid |

They deliberately span no-water→all-water, flat→steep, open→closed canopy: a
contract that survives all five survives.

## Reference material on other branches

This branch starts blank, but the repo's other branches hold data and prior
art (read-only from here; reference with branch-qualified paths):

- `terrain-v2` — Python data tools: `tools/dtm_atlas` (1 m lidar DTMs for 135
  US courses), `tools/dtm_metrics`, `tools/dtm_primitives` (landform
  extraction), `tools/tile_scout`, `tools/landform_prior`; plus
  `tools/metrics` (49-scalar morphology space over 200 courses) and
  `assets/atlas.bin` (200-course atlas).
- `archetype-pipeline` — the M0 scaffold of this same 10-step design:
  `course-contracts` (typed contracts, artifact store, fixtures, placeholder
  archetype prior), `course-lab` (per-stage egui viewer), and
  `docs/archetype-pipeline/ARCHITECTURE.md`. Useful as a concrete first
  draft of every contract in `steps/`.
- `main` — the v1 generator; best single references:
  `golf-terrain/src/erosion.rs` (deterministic erosion),
  `golf-terrain/src/water.rs` (hydrographic extraction),
  `golf-routing/` (9-hole routing search).
