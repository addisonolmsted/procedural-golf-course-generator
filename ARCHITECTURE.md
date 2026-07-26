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
```

No code yet. Language/stack for each step is decided when the step is built;
nothing in this branch's contracts assumes a specific language — contracts are
data shapes, requirements, and file formats.

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
   (`<step>/<purpose>/v1`); a registry of stream names will live here once
   code exists. No wall-clock, no unordered parallel reductions, no
   data-dependent draw order.
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
