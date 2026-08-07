# Migration

Where everything from before v2 went. Four layers, in increasing distance from
the working tree.

Unknowns are marked **TBD** rather than guessed. A `TBD` here means nobody has
decided yet — it is a question for whoever implements that stage, not a
suggestion that the answer is obvious.

**Nothing is deleted by the v2 scaffold pass.** Legacy crates still compile and
still pass their tests. Each is retired only when its replacement actually
lands.

---

## 1. Crates in this working tree

| Crate | Fate | Maps to | Notes |
|---|---|---|---|
| `course-seed` | **retained unchanged** | all stages | The determinism kit. Stage-agnostic and correct. Its reroll machinery (`MAX_ATTEMPTS`, `Scope::Attempt`) is **vestigial** under v2 — no v2 stage raises an attempt — but is kept because removing it would rekey every stream for no gain. |
| `course-world` | **retained unchanged** | all stages | `Grid`/`GridSpec`, CGRID1, world constants, `Spine`/Catmull-Rom, noise, `Vec2`/libm math, `Profile`, `ease`. This is why `course-contracts` does **not** restate grid or seed types. |
| `course-viz` | **retained unchanged** | dev tooling | Grid→PNG renderers. |
| `course-spec` | **slot retained, contents replaced** | [S0](docs/stages/stage-00-archetype-draw.md) | Right crate, pre-v2 contents. Its five archetypes (`archetype.rs`) and the fitted 88-knob `data/archetype_priors.json` are keyed to the retired archetype set and do not carry over. Its **`FINGERPRINT_GOLDEN` interlock is the pattern to keep** — see [envelope certification](docs/calibration/envelope-certification.md). `HOLE_COUNT`/`PAR_SEQUENCE` stay as-is. |
| `course-framing` | **retained until superseded** | [S1](docs/stages/stage-01-macro-primitives.md), [S2](docs/stages/stage-02-skeleton-kernel.md) | v1 stage 01. Its structural skeleton — trunk corridor, tributaries, ridge axes, terrace step lines — is **direct prior art for the fluvial engine**, particularly the order-sensitive draw transcript and the family-isolation tests. Read `src/generate.rs` before implementing S2. Retire when S2 lands. |
| `stage-lab` | **repurposed** | dev viewer | egui per-stage viewer shell with a seed-sweep gallery and headless snapshots. The shell is sound; its one tab (framing) goes when `course-framing` does. New stages add tabs. |
| `tile-lab` | **retained** | [calibration](docs/calibration/lidar-pipeline.md) | Campaign-data QA viewer: real corpus tiles, extractor classifications, the exclusion cull. Belongs to the corpus loop, not the generator. |

### New in v2

All 14 are empty scaffolds — doc comments and module skeletons only.

`course-contracts` (C1/C2/C3, units, metadata, plasticity, biome) ·
`course-primitives` (S1) · `course-skeleton` (S2) · `course-amplify` (S3) ·
`course-transforms` (S4) · `course-substrate` (S5) · `course-routing` (S6) ·
`course-earthmoving` (S7) · `course-layout` (S8) · `course-micro` (S9) ·
`course-zoning` (S10) · `course-validate` (S11) · `course-calibration`
(offline) · `course-cli` (headless driver).

---

## 2. Python `tools/` in this working tree

~18 300 lines across 7 packages, all built for the retired step-03 era.

| Package | LOC | Fate | Maps to |
|---|---:|---|---|
| `metrics` | 1 912 | **retained, core of the battery** | [metric-battery.md](docs/calibration/metric-battery.md). `core.py`'s pure `f(height, cell, mask)` kernels are the reason real and synthetic terrain share a feature space. **Reuse; do not rewrite.** The v2 battery of 22 is a subset of its 49 plus 4 new metrics. Note `spectral_slope_beta`, the curvature family, and `anisotropy` are already here — they were simply not being used as identity metrics. |
| `dtm_metrics` | 2 356 | **retained, three parts** | `gates.py` (G1–G6 metric admission) → the battery's admission layer. `sandbox/` (Sobol → HistGBT → SALib, with the inert `dummy` noise floor and the θ-vs-seed variance split) → the structural template for `course-calibration`. **`surfaces.py` → the dictionary's decomposition**: it already computes `s2 = s1 − lowpass` plus the `lp_slope`/`lp_aspect`/`tpi`/`relief_pos` conditioning fields, with a *pluggable* macro provider. That pluggability is the seam S3's fitter attaches to. |
| `dtm_atlas` | 3 003 | **retained** | [lidar-pipeline.md](docs/calibration/lidar-pipeline.md). `dep3.py` (3DEP fetch), `grids.py` (1 m→2 m block-mean), `nhd.py`, `osm.py`, `masks.py`. |
| `macro_campaign` | 4 463 | **partly retained** | `fetch.py`, `cgrid.py`, `develop/regions/report` → corpus infrastructure, retained. `compare.py` → the real-vs-generated gate bands, **including `--baseline`**, which splits the real corpus in half to measure each gate's false-positive rate. Any new harness needs this or it will optimize into smooth blobs. `fit_knobs.py` → superseded by envelope fitting, but its **gating policy and exclusion-digest provenance carry forward**. `shape.py` → retired. |
| `tile_scout` | 2 219 | **retained** | The F0/F1/F2 tile-selection cascade. |
| `dtm_primitives` | 3 599 | **promoted to a dependency** | Geomorphons, ridge/valley pipes, network extraction, transects. Built to extract primitives for the retired generator, but v2 needs exactly this to **skeletonize real tiles** so their residuals can be conditioned the same way the generator conditions its own. `network.py`, `ridgepipe.py`, `geomorphons.py` are now on the critical path for S3's dictionary. |
| `landform_prior` | 731 | **reference only** | Marked retired already. But `fit.py` — weighted quantile tables, log-log hydraulic regressions, a **Gaussian copula over reach residuals**, Kish-ESS shrinkage — is the **closest prior art for the envelope fitter**. See [envelope certification, open question 1](docs/calibration/envelope-certification.md). |

### Data

`tools/macro_campaign/out/` — 2.3 GB, gitignored, refetchable. 180 real 3DEP
tiles across the **retired** five archetypes, plus extracted knobs, generated
comparisons, and the `report/` dashboards.

**Tiles are salvageable for three v2 biomes; fits are not.** And the corpus is
thinner than the headline: **38 of 90 tiles are clean** (52 excluded). See
[targets.md](docs/calibration/targets.md) for the crosswalk and the collection
order.

The **64 real golf-course grids** in `out/courses/` are promoted from a
one-knob input to two roles: ground truth for the
[S5](docs/stages/stage-05-siting-substrate.md) siting scorer, and a
separately-weighted exemplar source for the dictionary.

**Resolved (2026-08-07):** `tools/parkland_atlas` and `assets/atlas.bin` are
recovered from `main`, and the "non-runnable" diagnosis turned out to be
narrower than feared. The parkland cache (`out/cache/*.json`) is needed only
for **atlas-corpus runs** of `tools/metrics` — a v1 workflow. The battery
itself (`metrics.features.compute`) is a pure function and runs on CGRID1
tiles directly; `dtm_metrics` degrades gracefully (one context column NaN).
Verified end-to-end by `tools/spike/smoke.py`: tile → `surfaces.build` →
49-scalar battery, on a real clean piedmont tile. **v2 never needs the cache**
— its corpus is the 3 km @ 2 m tiles, better data than the atlas's 256²
clips. If an atlas run is ever wanted, the regeneration path is
`assets/atlas.bin` + `golf-atlas/src/binfmt.rs` (main), or re-collection via
the parkland_atlas scripts.

**Still absent (M4 concern, not spike):** the 135-course DTM store
(`tools/dtm_atlas/out/`) that `dtm_metrics/gates.py` runs over. Refetchable
from 3DEP via `dep3.py`; only the indices are committed.

---

## 3. The `main` branch

The full pre-v2 generator, 37 Rust files. Not in this tree; recoverable with
`git checkout main -- <path>`.

| Path | Maps to | Notes |
|---|---|---|
| `tools/calibration/` (11 modules, ~54 KB) | [envelope-certification.md](docs/calibration/envelope-certification.md) | **The single most valuable thing on any other branch.** A complete working harness for exactly the v2 loop: Sobol design → forward → coverage gate → HistGBT emulator + SALib → density-ratio inverse → held-out validation → bake to Rust. Read `CALIBRATION.md` first. Rebinding needed: `param_schema.py`, `forward.py`, `export_sampler.py`. |
| `golf-terrain/src/erosion.rs` | [S3](docs/stages/stage-03-amplification.md) polish, [S4](docs/stages/stage-04-hydrology.md) flow | **Mostly retired.** v2 does not simulate erosion for texture — texture comes from the patch dictionary. What survives is the priority-flood core (useful to S4's flow routing, though `macro_campaign/flow.py` with Barnes flat resolution is the better reference) and, at most, S3's short consistency polish. Port neither the authority nor the stream-power model. |
| `golf-terrain/src/water.rs` | [S4](docs/stages/stage-04-hydrology.md) | Hydrographic extraction. |
| `golf-terrain/src/sampler/ptheta.rs` (163 KB, generated) | [S0](docs/stages/stage-00-archetype-draw.md) | The precedent for baking a fitted ensemble into deterministic Rust: unit-space design points, weight CDF, `(lo, hi, log, int)` decode spec. |
| `golf-routing/` (7 files) | [S6](docs/stages/stage-06-routing.md) | The beam/anneal router. Closest prior art for S6, but it read terrain directly — S6 may see only [C2](docs/contracts/C2-routing-substrate.md). |
| `golf-holes/` (6 files) | [S7](docs/stages/stage-07-earthmoving.md), [S8](docs/stages/stage-08-hole-layout.md) | Hole build-out. Prior art for S7's template library and S8's surface passes 7–9, both of which are open questions blocking implementation. |
| `xtask/` (136 KB) incl. `xtask/golden/*.hash` | [S11](docs/stages/stage-11-validation.md), calibration | Golden-hash regression driver. `forward-grid` is the batch entrypoint v1's `forward.py` calls — **`course-cli` must provide an equivalent** before the calibration loop can run. Also `dump-heightgrid`, `calibrate`, `sampler-check`. |
| `tools/parkland_atlas/` + `assets/atlas.bin` | calibration | The 200-course atlas. Needed to make `tools/metrics` runnable again. |
| `golf-core/`, `golf-viewer/`, `golf-atlas/`, `golf-viz/`, `golf-match/` | — | **TBD.** `golf-viz` is already ported as `course-viz`. The rest have no assigned v2 home. |

### Other branches

- **`terrain-v2`** — everything on `main` plus `golf-landform` (16 files,
  noise/skeleton experiments) and `docs/`. Source of the Python tools and of
  `course-world`/`course-viz`.
- **`archetype-pipeline`** — superset of `terrain-v2` plus `course-contracts`
  (9 files, the M0 typed-contract scaffold) and `course-lab`. The v2
  `course-contracts` is a **new crate with the same name and a different
  scope** — that one held pipeline-wide types; this one holds only C1/C2/C3 and
  the shared vocabulary. Worth reading for fixture and gate/report shapes.
- **`pipeline`** — the immediate predecessor. The v1 14-stage sim-co-authored
  architecture, its `stages/00…13` docs, and stage 01 built. The `heartland`
  branch is cut from its tip, so it is reachable as ordinary history.

---

## 4. Archived in-repo history

| Commit | What |
|---|---|
| `1ad56c7` | **Stage 01 + stage-lab + N1 Rev A.** The last v1 work, committed to `pipeline` immediately before the v2 branch so it could not be stranded. `course-framing` and `stage-lab` come forward into v2; the rest is reference. |
| `2fcb0ee` | The v1 reset: 14-stage sim-co-authored pipeline, docs-first. The architecture v2 replaces. Its `stages/` docs are the best record of the reasoning v2 reverses — see [the four reversals](docs/00-architecture.md). |
| `0313432` | **The step-03 era archive.** `crates/course-macro` + `crates/macro-lab` (29 files, 290 KB) — the authored macro-primitive generator, with its planner, network, and budget systems, and a 67 KB working journal at `steps/03-macro-landform.md`. Retired because it measured **22 of 71 knobs inside the real-tile IQR** (`tools/macro_campaign/out/report/compare.md`). **Read this before implementing S2.** It is the record of what authoring-in-landform-space costs, and S2's entire justification is that it authors in a different space. |
| `62da080` | Steps 01+02: the determinism kit and the archetype spec. |
| `b2b1e26` | Docs-first blank slate. |

---

## Retirement schedule

Nothing is removed until its replacement is verified.

| Retire | When |
|---|---|
| `course-framing` | [S1](docs/stages/stage-01-macro-primitives.md) and [S2](docs/stages/stage-02-skeleton-kernel.md) both verified |
| `course-spec`'s `archetype.rs` + `archetype_priors.json` | [S0](docs/stages/stage-00-archetype-draw.md) verified against a certified envelope |
| `stage-lab`'s framing tab | with `course-framing` |
| `tools/macro_campaign/macro_campaign/fit_knobs.py` | envelope fitting produces a certified envelope |
| The v1 tile corpus | the v2 corpus covers all six biomes ([targets.md](docs/calibration/targets.md)) |
