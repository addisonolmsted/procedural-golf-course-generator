# Terrain Workplan — everything up to routing

The complete work breakdown from today's state (scaffold + docs + verified
measurement chain) to **terrain-complete** — G-TERRAIN + G-SITE passing on
piedmont, heathland, and sandhills
([03-success-indicators.md](03-success-indicators.md)). Routing (S6+) is
explicitly out of scope; C2 is the finish line.

Three tracks run in parallel: **Rust pipeline**, **Python calibration/data**,
and **viewer/verification**. Phases are ordered by dependency, not strictly by
time — B and E overlap A; E runs underneath everything.

Sizes: S = days, M = ~1–2 weeks, L = weeks+.

---

## Phase A — The spike (M3.5) · gate G-SPIKE · blocking

The go/no-go on the central bet, using only data already on disk.
Workspace: [`tools/spike/`](../tools/spike/README.md).

| # | Item | Size | Verification |
|---|---|---|---|
| A1 | Patch + conditioning extraction from 5 of the 6 clean piedmont tiles (`surfaces.build` + skeleton fields; hold out `t03550_08002`) | S | patch count, per-bucket tile diversity report |
| A2 | Coarse bucketing (slope × TPI × hillslope-position, ~3×3×3) + PCA basis (~16 comp) + coefficient distributions per bucket | S | reconstruction error on *training* tiles first |
| A3 | Reconstruct residual over the held-out tile's own lowpass; overlap-add blend | S | β, variogram range/sill, curvature pair vs the held-out tile's own values |
| A4 | Seam check: radial PSD, look for a peak at patch pitch/harmonics | S | no peak above noise |
| A5 | Hillshade crop pairs (real vs reconstructed) for the blind A/B | S | **P2 relaxed: reviewer ≤ 75%** |
| **GATE** | **G-SPIKE report**: metric table + crops, committed | — | go ⇒ Phase E scales up · no-go ⇒ fallback conversation NOW |

## Phase B — Foundations (parallel with A)

| # | Item | Size | Verification |
|---|---|---|---|
| B1 | **M1: contracts.** `course-contracts`: C1/C2/C3 (+C0 manifest types) as real Rust types, construction-time invariant enforcement, serde with loud load errors, **fixture builders** for each | M | unit tests per invariant; a hand-built C2 fixture round-trips byte-identically |
| B2 | **Stream registry rekey** to v2 names (`primitives/v1`, `skeleton/*`, `amplify/v1`, `hydro/v1`, `substrate/v1`), one `PIPELINE_VERSION` bump, goldens re-blessed once | S | registry mirror test; double-run test |
| B3 | **M2: `course-cli`.** Subcommands: `run` (pipeline through stage N, write artifacts), `batch` (the `forward-grid` equivalent: θ-list × seeds → surfaces, rayon), `time` (per-stage ms vs the budget table) | M | 100-run batch deterministic (hash-identical across two invocations); timing report emits |
| B4 | **Battery additions** in `tools/metrics/metrics/core.py`: `network_connectivity`, `hypsometric_bimodality`, `slope_bimodality`, `horton_ratios` (wrapper over `netstats._strahler`) + analytic-surface tests for each | M | tests: connectivity=1 on a cone with full network, ≈0 on disconnected segments; bimodality fires on a two-plane surface, not on fBm |
| B5 | **Golfability proxy**: measure the 64 real course grids → `proxy_thresholds.json` (slope-cap fraction, contiguous sub-cap area, relief band, ponding coverage); scorer function `f(surface, window) -> score` | S | thresholds file committed with provenance; real courses score ≥ threshold on themselves (sanity) |
| B6 | **Tile-battery runner**: promote `spike/smoke.py` into a proper CLI (`tools/spike/measure.py` or `metrics` entry) — any CGRID → full battery + residual battery → JSON row | S | runs over all 38 clean tiles without error |

## Phase C — Upstream stages S0 + S1 (needs B1)

| # | Item | Size | Verification |
|---|---|---|---|
| C1 | **S0 in `course-spec`**: biome + structural-class + descriptor + exemplar draw against a **provisional hand-authored envelope** with the final schema (GMM JSON, fingerprint interlock). Real fitted data swaps in at Phase H with no code change | M | stage-00 acceptance criteria; 10k-seed mode coverage; zero rejection sampling (draw-count assert) |
| C2 | **S1 in `course-primitives`**: window-class realization, tilt/relief/hardness/accommodation, discontinuities, C1 emission | M–L | stage-01 acceptance criteria: class-distinct fields, discontinuity steps, band-limit PSD assert, mean-zero relief |
| C3 | **Stage-lab: Spec tab (S0)** — drawn spec as a readable card (class, descriptors, exemplars) + a 20-seed spec table view | S | eyeball: 20 specs describe different sites |
| C4 | **Stage-lab: C1 tab (S1)** — the four fields, discontinuity overlay, **implied-terrain hillshade** (port the v1 framing-tab pattern), seed-sweep gallery **grouped by window class** | M | supports the class-legibility test |
| C5 | **Class-legibility session** (S1 exit test): blind rounds on the gallery, ≥ 80% class named from hillshade alone | S | recorded in the gate report format |
| C6 | Golden fixtures: `spec.json` + C1 for seed 1 × {piedmont, heathland, sandhills} | S | double-run + golden tests green |

## Phase D — S2 + gate G-SKELETON (needs C, B4–B6)

| # | Item | Size | Verification |
|---|---|---|---|
| D1 | **S2 in `course-skeleton`**: trunk growth, Horton-hierarchical tributaries (top-down construction so ratios hold), discontinuity deflection, flow-distance transform (+norm, hillslope-position), derived divides, parametric catena base, the five structural modules | **L** — the largest single Rust item before S3 | stage-02 acceptance criteria |
| D2 | **Rust flow kernel with Barnes flat resolution** (port of `macro_campaign/flow.py`; shared by S2 verify + S4) — lives in `course-world` or `course-skeleton::fluvial` | M | port-parity test vs flow.py outputs on a real tile; the no-straight-channels regression |
| D3 | **Conditioning-overlap tool** (Python): joint conditioning distribution of generated cells vs the 38 clean real tiles; overlap ≥ ~90% | S | the G-SKELETON leading indicator |
| D4 | **Stage-lab: Skeleton tab (S2)** — base hillshade + **network/divide overlay** (P1, the v1 lesson), Strahler-order coloring, flow-distance + conditioning field views, seed gallery per biome | M | P1 review session |
| D5 | Invariant cross-check harness: `dist_to_channel_p50`, density, θ, Horton ratios via `course-cli batch` + tile-battery over 1 000 seeds × 6 biomes; assert in band AND not biome-separated | S | part of the gate report |
| D6 | Golfability proxy on base surfaces (windows scored, in-regime check) | S | part of the gate report |
| **GATE** | **G-SKELETON report**: invariants, conditioning overlap, IoU variety, proxy, P1 gallery | — | blocking; reviewer must know candle-wax is expected |

## Phase E — Corpus campaign (M4) · the long pole · starts day one

| # | Item | Size | Verification |
|---|---|---|---|
| E1 | Region definitions for the six Heartland biomes in `tools/tile_scout` (protected/near-unpopulated land only) | S | region maps reviewed |
| E2 | Fetch ~50/biome for US biomes via `fetch.py` (3DEP → CGRID1); resumable, network-heavy, unattended | M (wall-clock L) | tile counts; valid_frac |
| E3 | **Heathland non-US ingestion** (EA LIDAR / AHN): new fetch path + conditioning parity checks vs 3DEP | **M–L, highest-risk data item** | cross-source comparability test on overlap metrics |
| E4 | OSM develop screen + **tile-lab cull** (human review) → `exclude.json` v2 with digest | M (human time) | ~30 clean/biome; per-tile reasons |
| E5 | Skeleton + residual extraction per clean tile (`network.py`, `ridgepipe.py`, `geomorphons.py`, `surfaces.build`) → per-tile conditioning + patch source data | M | spot-check overlays in tile-lab (P1 on real tiles) |
| E6 | **Empirical invariant/discriminant confirmation**: between-biome / within-biome variance ratio for all 22 metrics across the corpora; reassign roles if the data disagrees | S | the split table updated with measured F-ratios |
| E7 | **targets.md filled**: shared-invariant block + per-biome discriminant vectors, despeckle sensitivity bands, DR denominators (real within-biome std) | M | bands committed with exclusion digest + battery version |
| E8 | G1–G6 metric admission over the corpus (`gates.py`; needs the dtm_atlas store refetch or adaptation to run over campaign tiles — decide then) | M | every fit-target metric classified |

## Phase F — Dictionary production + C0 settlement (needs A go, E5)

| # | Item | Size | Verification |
|---|---|---|---|
| F1 | **M4.5: settle C0 with the frontend team** — format, quantization, `height_play` extent, beyond-window LOD | S (calendar risk) | C0 doc TBD rows resolved; C0 types updated in B1's crate |
| F2 | **Dictionary fitter** in `course-calibration` (or Python baking to a Rust-readable asset): bucketing tuned by E5 coverage, PCA/sparse basis, coefficient distributions, course-grid source weighted separately, i16+zstd bake, fingerprint | **L** | per-bucket tile diversity ≥ 5; asset < 15 MB; round-trip reconstruction error report |
| F3 | Dictionary QA: reconstruct held-out real tiles per biome, full discriminant scoring | M | energy distance beats 2.35 on held-out; per-biome bands |

## Phase G — S3 + S4 + gate G-TERRAIN (needs D, F)

| # | Item | Size | Verification |
|---|---|---|---|
| G1 | **S3 in `course-amplify`**: asset loader (fingerprint check), conditioning, position-seeded reconstruction, overlap-add blend, near-channel taper, sub-patch spectral fill, relief-budget closer, polish (embryo-aware pit removal) | **L** | stage-03 acceptance criteria incl. permuted-order bit-identity |
| G2 | **S4 in `course-transforms`**: flow re-derivation (D2's kernel), water-table + floodplain datums, trunk carve modifier, basin inventory, `skeleton_agreement`, water provenance | M | stage-04 acceptance criteria incl. the flat-channels regression |
| G3 | **Stage-lab: Amplify tab (S3)** — before/after hillshade, residual-only view, bucket-usage heat, PSD panel (seam check), **A/B crop exporter** for P2 | M | feeds P2/P3 sessions |
| G4 | **Stage-lab: Hydro tab (S4)** — water polygons + flow-accum overlay on hillshade, basin list, agreement score | S | P1 water-in-low-places review |
| G5 | Gate runs: 1 000 seeds × 3 gate biomes through `course-cli batch` + battery; DR table; blind A/B + name-the-biome sessions | M | **G-TERRAIN**: discriminants in band, energy distance > v1's 2.35 baseline, P2 ≤ 65%, P3 ≥ 80%, DR 0.7–1.3, agreement ≥ 0.9 |
| G6 | Goldens for S3/S4 artifacts, 3 biomes, seed 1 | S | green |

## Phase H — S5 + envelopes + gate G-SITE ⇒ terrain-complete

| # | Item | Size | Verification |
|---|---|---|---|
| H1 | **Siting scorer fit** (Python): chosen-ground vs random-window contrast on the 64 course grids (the one-class fix) → fitted weights | M | scorer separates real course sites from random windows on held-out grids |
| H2 | **S5 in `course-substrate`**: siting scan (summed-area exhaustive, stable tie-break), masks, cost fields, metadata pass-through, C2 assembly | M | stage-05 acceptance criteria incl. the siting-arithmetic test (600 m, ±450 m, ≥750 m margin) |
| H3 | **Stage-lab: Siting tab (S5)** — score heatmap over the core, top-3 window panel, chosen-window overlay, mask/cost field views | M | P5 golf-eye sessions |
| H4 | **Envelope certification** (M7): recover `tools/calibration` from `main`; rebind `param_schema`/`forward` (→ `course-cli batch`)/`export_sampler`; GMM fit per gate biome; certify the five hard guarantees (incl. S3 amplitude ceiling, siting viability); bake + fingerprint; swap into S0 | **L** | coverage gate run **separately on invariants and discriminants**; ESS healthy; S0 goldens re-blessed once |
| H5 | **C0 exporter** in `course-cli`: bundle per the settled contract (manifest, layers, wind, hashes) | M | a bundle validates against C0 invariants; frontend smoke-loads one |
| **GATE** | **G-SITE**, then **terrain-complete**: G-TERRAIN + G-SITE on piedmont, heathland, sandhills, gate reports committed | — | the declared precondition for any routing work |

---

## Cross-cutting verification infrastructure (built once, used everywhere)

| Item | When | Note |
|---|---|---|
| Per-stage determinism double-run + golden tests | with each stage | pattern exists (`course-seed`, `course-framing`) |
| **Gate-report generator**: headless stage-lab snapshots + metric JSON + DR table into `reports/<gate>/` | Phase D (first needed) | extends the existing `framing_snapshots.rs` pattern |
| Cross-platform determinism check (macOS + Linux CI on goldens) | Phase C onward | S3/S4 iterative float code is the risk; a GitHub Actions runner suffices |
| Budget timing in CI (`course-cli time` vs the table) | Phase D onward | S3's 900 ms is flagged unmeasured — verify at G1 start, not end |

## Explicitly deferred (not terrain)

S6 routing, C3 consumers, S7 earthmoving, S8 layout, S9 micro, S10 zoning,
S11's playability/sightline halves (its **metric-vector half** is effectively
built in B6/D5). `course-framing` retirement happens after D; the stage-lab
framing tab goes with it.

## The dependency spine

```
A (spike) ──go──┐
B1 contracts ───┼─→ C (S0,S1) ─→ D (S2, G-SKELETON) ─→ G (S3,S4, G-TERRAIN) ─→ H (S5, G-SITE)
B3 cli ─────────┘                        ↑                    ↑                     ↑
E (corpus, runs underneath) ── E5 ─→ F (dictionary) ──────────┘                     │
                               E7 targets ──────────────────────────────────────────┤
B5 proxy ───────────────────────────────→ D6, H1 ───────────────────────────────────┘
F1 (C0 with frontend) ─────────────────────→ G1 (quantization bounds texture depth)
```

Longest path: **E → F → G → H** — the corpus feeds everything. Start E2's
fetching the moment A confirms the method; everything Rust-side (B, C, D) is
off the critical path and parallelizes.
