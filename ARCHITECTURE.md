# Architecture

The general architecture of the golf-terrain pipeline. **This file is updated
every time a folder or file is added to this branch** — it is the map of the
repo. Per-stage contracts live under `stages/`; the project plan and
milestones live in [PLAN.md](PLAN.md).

## The pipeline in one paragraph

A deterministic 14-stage generator (stages 0–13). Authored **envelopes** in,
physics out: Stage 0 draws the archetype and the parameter vector θ; stages
1–3 author the site frame, the routability mask (the guarantee, authored
BEFORE terrain exists), and a few shallow guidance strokes; Stage 4 compiles
them into forcing fields; Stage 5 — the co-author — runs a landscape
evolution model (stream power + diffusion + thermal) that earns terrain
plausibility instead of imitating it; stages 6–8 extract water, assign
cover, and cheaply verify the mask survived; stages 9–13 route the holes,
build them, validate drainage, blend the seams, and emit the object
manifest. Rerolls are cheap by construction: a gate fail resamples stages
2–3 only — archetype, θ, and framing survive.

Why this shape: the previous authored-primitive generator was measured
against a 90-tile corpus of real landscapes and failed structurally (22/71
knobs within the real IQR; archive commit `0313432`,
`tools/macro_campaign/out/report/compare.md`). Realism is now delegated to
the sim; authorship is confined to envelopes the sim cannot break.

## Repo layout (current)

```
ARCHITECTURE.md        — this file: repo map + cross-cutting invariants
PLAN.md                — goal, milestones N0-N8, campaign v2, status
stages/00…13-*.md      — one contract doc per stage; each stage's owning
                         agent builds that stage and keeps its doc current
Cargo.toml             — Rust workspace (resolver 2, edition 2024)
crates/course-seed/    — stage 00a: DetRng determinism kit, stream registry,
                         RunIdentity, reroll rule
crates/course-spec/    — stage 00b: archetype selection + θ sampling →
                         spec.json; data/archetype_priors.json is the
                         committed, fingerprinted prior (knob registry)
crates/course-world/   — shared substrate: Grid/GridSpec + CGRID1 store,
                         world constants, Vec2/libm math, ease, Profile,
                         Spine/catmull-rom splines, perlin noise
crates/course-viz/     — shared substrate: Grid→PNG renderers (hillshade,
                         hypsometric, scalar/direction fields, overlays)
crates/tile-lab/       — campaign-data QA viewer: real corpus tiles, the
                         extractor's classifications, knobs-vs-prior, the
                         exclusion cull; optional compare pane accepts any
                         CGRID1 heightfield
tools/                 — Python: the data campaign + terrain analysis stack
  macro_campaign/      — corpus fetch/screen/report + fitting (see PLAN.md
                         campaign v2 for what is retained vs retired)
  dtm_atlas/ dtm_metrics/ dtm_primitives/ metrics/ tile_scout/
                       — ported terrain-v2 analysis tools (DTM store,
                         metric batteries, primitive extraction, tile scout)
  landform_prior/      — retired fitting pattern (reference only)
```

Retired at commit `0313432` (archived, reachable, not in the workspace):
`crates/course-macro` (authored macro primitives), `crates/macro-lab`.

## The separation principle

Each stage is an **isolated module behind a versioned artifact contract**:

1. A stage's only interface is: read its input artifacts → write its output
   artifact. Data shapes live in the stage docs; nothing crosses a stage
   boundary that is not in a contract.
2. A stage may be overhauled or replaced without touching any other stage,
   so long as its output contract is honored. Contract changes are
   cross-stage events: update the affected docs + this file, bump the
   artifact version and `PIPELINE_VERSION`.
3. Stages never import each other's internals and are testable in isolation
   against fixture artifacts before their upstream neighbors exist.
4. Every stage doc has the same skeleton (Purpose / Position / Contract /
   Per-archetype behavior / Hard requirements / References / Open questions
   / Status), so an agent can own one doc and one module without reading
   the others.

## World geometry (fixed, cross-stage)

- Generated terrain: **3 km × 3 km**, origin SW, x east, y north, metres.
  (`course-world::world`: `EXTENT_M`, `world_spec`, `in_core`.)
- **Core**: central 1.5 km × 1.5 km `[750, 2250]²` m. The routability mask
  (stage 02) lives mostly inside it; holes are routed inside the mask.
- Resolutions: **2 m** canonical deliverable (`RES_FULL_M`, 1501² nodes),
  8 m preview, **sim resolution ~4 m** for stage 05 (its doc owns the final
  choice), **0.5 m** local patches for stages 10–12
  (`RES_EARTHWORKS_M`). Every stage is resolution-consistent: same code
  path at any cell size.

## Cross-cutting invariants

1. **Determinism.** One master `seed: u64`. Same seed + params + resolution
   ⇒ byte-identical artifacts on every platform. All randomness through
   `RunIdentity::stream(name)` with a name registered in
   `crates/course-seed/src/streams.rs` (this table is test-enforced to
   mirror it). Current (v1) registry:

   | Stream | Purpose |
   |---|---|
   | `arch/select/v1` | archetype draw (when not forced) |
   | `arch/params/v1` | θ sampling |
   | `macro/place/v1` | RETIRED with step 03 — removed at the v2 rekey |
   | `noise/field/v1` | retired name, replaced at v2 |
   | `hydro/v1` | retired name, replaced at v2 |
   | `cover/clump/v1` | retired name, replaced at v2 |
   | `route/v1` | routing search (name survives into v2) |
   | `earthworks/v1` | earthworks jitter (name survives into v2) |
   | `reroll/v1` | INTERNAL: gate-fail sub-seeds (master-seed keyed) |
   | `fixture/v1` | tests/fixtures only |

   **Registry v2 (planned, lands with the Stage-00 rev — see
   `stages/00-seed-archetype.md`):** every stream gains a SCOPE — `stable`
   (keyed off the master seed; stages 0–1: `arch/*`, `framing/v1` — survives
   rerolls) or `attempt` (keyed off the attempt seed; stages 2+: `mask/v1`,
   `strokes/v1`, `forcing/v1`, `cover/v1`, `route/v1`, `earthworks/v1`,
   `micro/v1`, `placement/v1`). The rekey is one `PIPELINE_VERSION` bump +
   golden re-bless.
2. **Versioned artifacts.** Every artifact carries a version; shape changes
   bump it + `PIPELINE_VERSION`, update the stage doc, re-bless goldens —
   explicit, reviewed events. `serde_json` runs with `float_roundtrip` so
   f64 JSON round-trips byte-identically.
3. **Artifacts are files.** JSON for structure, CGRID1 (`course-world::gridio`,
   mirrored in Python by `macro_campaign/cgrid.py`) for per-cell fields;
   per-course manifest with blake3 content hashes; `RunIdentity` embedded in
   every manifest.
4. **The empty case is legal.** Sandhills has zero surface water; every
   consumer must be correct on empty collections.
5. **The archetype is data, not code.** Stages read θ knobs and at most one
   mode switch (`hydrology_mode`); no stage branches on the archetype id.
   If you need `if sandhills`, you are missing a parameter.
6. **The mask is law.** From stage 02 onward, the routability mask's
   invariant (slope cap, no off-corridor channels, connectivity, corridor
   width) may not be broken by any stage — stage 04 encodes it as physics,
   stage 08 verifies it, stages 9–12 preserve it.

## The five archetypes

| Key | Identity | Water | Relief | Canopy |
|---|---|---|---|---|
| `sandhills` | aeolian dune trains, high infiltration | none (surface) | low-mid | open |
| `piedmont` | rolling fluvial + creek corridor, clay | streams | mid | closed |
| `florida_lowland` | water table at surface, lakes primary | dominant | ~zero | mid |
| `glacial_moraine` | kettled moraine / heathland | ponds/bogs | mid | open |
| `mountain_bench` | benches, scarps, terraces | mountain streams | high | mid |

Selected in stage 00, carried to stage 13 (the art-kit key). They span
no-water→all-water, flat→steep, open→closed canopy: a contract that
survives all five survives.

## Reference material

- **This repo's history**: commit `0313432` — the complete step-03 era
  (authored primitives, its planner/network/budget, the 67 KB working
  journal at `steps/03-macro-landform.md`, and the compare verdict). The
  best record of what was tried and measured.
- `main` branch: `golf-terrain/src/erosion.rs` (the stream-power/
  priority-flood core stage 05 ports), `golf-terrain/src/water.rs`
  (hydrographic extraction for stage 06), `golf-routing/` (the beam/anneal
  router stage 09 ports), `golf-holes/` (build-out prior art for stage 10).
- `terrain-v2` branch: the original Python tool sources; `golf-landform`
  noise/skeleton experiments.
- `archetype-pipeline` branch: the M0 typed-contract scaffold (fixture
  patterns, gate/report shapes).
