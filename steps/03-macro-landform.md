# Step 03 — Macro Landform

**Status:** built (claude, 2026-07-26)

## Purpose

Author the large-scale terrain skeleton the whole course is organized around:
the base heightfield (before noise/erosion), the conditioning fields that
steer downstream steps, and the structural graph (drainage spines, ridge
lines, bench edges). This is where the routable core is won or lost — macro
structure is placed so the core stays playable per archetype.

## Position

- Upstream: step 02 (`CourseSpec`, reads `params.landform`).
- Downstream: step 04 (conditions noise on the fields), step 05 (conditions
  erosion on drain spines), steps 07/08 (read the structure graph for
  corridor hints).

## Contract

Implementation: [`crates/course-macro/`](../crates/course-macro/) (on
[`crates/course-world/`](../crates/course-world/), the shared grid/geometry
substrate). Entry point: `generate_skeleton(&CourseSpec, res_m) ->
MacroResult { plan, config, skeleton }`.

Input: `CourseSpec`, `res_m`.

Output — `MacroSkeleton`:

```
MacroSkeleton {
  base_height: grid<f64>,          // metres, full 3 km box at res_m
  fields: {                        // all same grid spec, all [0,1] unless noted
    floor_damp:    grid<f64>,      // 1 on drainage floors → 0 by ~150 m past halfwidth
    slope_gain:    grid<f64>,      // structurally steep ground (analytic slope 0.05→0.50)
    grain_dir_rad: grid<f64>,      // local structural grain (radians mod π); global
                                   // azimuth bending to drain tangents near corridors
    valley_dist_m: grid<f64>,      // metres to nearest drain spine (10_000 when none)
    core_protect:  grid<f64>,      // 1 in routable core, 150 m smooth shoulder out
  },
  structure: {
    spines: [ { kind: drain | ridge_line | bench_edge,
                pts: [[x,y], …] } ],   // world coords; drains ordered downstream
  },
}
```

Drain spines are the RESOLVED valley floor centerlines (junction-snapped,
~10 m spacing, tributaries truncated at their junction); trunk endpoints
extend ~120 m past the box edge so the exit is explicit. Fields are computed
ANALYTICALLY from the resolved geometry (never from the raster), which is
what makes hard requirement 4 hold bit-exactly.

Artifact directory `macro_skeleton/` (`course-macro/src/artifact.rs`):
`structure.json` + six `.cgrid` (CGRID1, f64→f32) + `meta.json` (versions,
seed, prior fingerprint, in-memory-f64 FNV hashes) are the contract;
**`plan.json` (the staged `MacroPlan` IR) and `config.json` (the analytic
`MacroConfig`) are advisory sidecars** — lab/tooling only, downstream steps
MUST NOT read them. (Resolves the former open question: yes they ship, but
the rasterized output + structure graph remain the whole contract.)

Primitive vocabulary (`course-macro/src/prims/`, ported verbatim from
`terrain-v2:golf-landform`): regional tilt (+ low-order curvature), ridges
(smooth-max swept crests with emphasis profiles), bluffs/scarps (eased
steps), closed basins (bowls with mandatory Lake|Spillway outlet), valleys
(swept monotone floors, accordant `join_trunk` tributary junctions, carve
LAST via smooth-min), meander centerlines (curvature-clamped). Dune trains:
wavelength + grain only (carrier waves belong to step 04).

Randomness: `macro/place/v1`, drawn ONLY in `planner::plan()` in one
documented fixed order (see the module header). The budget solve draws
nothing. `MACRO_VERSION = 1`.

## The planner (how placement stays routable AND interesting)

Five staged passes, each recorded in `MacroPlan` for the viewer:

1. **Frame** — grain azimuth, tilt direction ⟂ grain ± jitter, curvature,
   valley-count draw, exit edge (most-downhill; `None` iff closed-basin-only).
2. **Drainage** — trunk chord along the downhill direction, offset from the
   box center. *Routing rule:* a creek's in-core drop ≈ `fall × 1500 m`; if
   that fits `0.6 × core_relief_cap_m` the chord clips/crosses one side of
   the core (gentle water through the course — the interest); otherwise it
   is **tangent-routed** just outside the core ring (steep archetypes), and
   tributaries are forced to the outer side. Tributaries join with
   30–70° angles at stratified stations; accordance is `join_trunk`'s.
3. **Uplands** — the **bench cascade**: excess regional drop
   `(tilt_grade − 0.4·cap/1500)·3000 m` is converted into contour-parallel
   scarp steps with flat treads; the core-center band (≥ max(250, tread/2) m)
   stays scarp-free — the through-core tread is the steep-archetype
   routability lever. Ridges use the interfluve rule (max distance to
   drains, 4-candidate argmax), aligned to grain.
4. **Basins** — cluster-biased darts with min-spacing, rejected within
   60 m + radius of drain floors; blowouts elongate along grain
   (`blowout_eccentricity`); outlet = Lake when closed-basin-only, else
   Lake/Spillway draw with the spillway on the downhill rim.
5. **Core budget** (`corecheck`) — fixed 16 m preview (independent of
   res_m), fixed 6 iterations: first passes scale per-feature **core dips**
   (height eased down over the normalized-arc spans where a feature's
   *footprint* — spine within height/gradient reach — touches the core);
   later passes scale whole-feature amplitudes + effective tilt/curve with
   a squared factor (converges even when the unscalable valley floor
   dominates). Valleys are never scaled — their incision is
   placement-capped at ≤ 0.6·cap. Every action is recorded in
   `CoreBudget.notes`.

## Per-archetype behavior (via params, never branches)

- `sandhills`: dune-train grain + blowout basins; drains usually absent
  (`valley_count` median 0 ⇒ closed-basin-only; basins become Lakes = no
  outlet — water presence is step 05's infiltration call).
- `piedmont`: 1–3 drain spines (creek corridor crossing/clipping the core),
  rolling interfluve ridges.
- `florida_lowland`: near-flat plane, big shallow basin field (lakes-to-be),
  tilt ~0.
- `glacial_moraine`: many kettles + short ridges; internal drainage legal.
- `mountain_bench`: bench cascade absorbs the regional grade with ≥1 tread
  through the core; the mountain stream is tangent-routed outside the core.

## Hard requirements (all tested — `crates/course-macro/tests/`)

1. **Monotone drains:** composed height along every drain floor falls
   monotonically (≤ 2 m smooth-min scour-dip tolerance, the terrain-v2
   gate's bound); junctions accordant (Δfloor < 1e-6); unless
   closed-basin-only, the trunk exits the box.
   (`contract.rs::drains_monotone_accordant_and_exiting`)
2. **Core cap:** post-solve core relief ≤ `core_relief_cap_m` (×1.05
   preview tolerance), incl. an adversarial forced-override case.
   (`core_relief_capped`, `core_relief_capped_adversarial`)
3. **Field consistency:** floor_damp ≈ 1 on drain centerlines;
   valley_dist_m matches brute-force distance to published spines;
   core_protect window exact. (`fields_match_geometry`)
4. **Resolution-consistent:** 48 m grid is a bit-exact node subset of the
   16 m grid; structure graph identical. (`coarse_is_downsample_of_fine`)
5. **Deterministic double-run:** byte-identical artifact dirs; plan
   round-trips. (`deterministic_artifact_double_run`)

Plus: mountain always threads a scarp-free core tread
(`mountain_bench_through_core_with_clear_tread`), sandhills closed-basin
case (`sandhills_grain_and_blowouts`), the ported golf-landform property
suite (`property.rs`: preset floor monotonicity, C1 transects, 1k meander
fuzz, engine golden), per-archetype base-height goldens (`goldens.rs`),
and a headless render smoke (`viewer_smoke.rs`).

## Viewer

[`crates/macro-lab/`](../crates/macro-lab/) — the **step-03-only** viewer
(steps 04+ build their own on [`crates/course-viz/`](../crates/course-viz/)):
tabs per construction stage (Frame / Drainage / Uplands / Basins / Budget /
Terrain / Fields), seed scrub, forced archetype, 16 m/4 m preview toggle,
contours, and per-knob `landform.*` override drags (via step-02
`SpecOverrides`). `cargo run -p macro-lab --release`. Static per-archetype
renders: `cargo run -p macro-lab --example snapshots --release -- <dir> [seed]`.

## Tile QA viewer

[`crates/tile-lab/`](../crates/tile-lab/) answers "are we fitting the prior
to good data?" — it shows the REAL campaign tiles with the extractor's own
classifications drawn on top, read back from `out/extract/**` exactly as the
fit consumed them (the viewer measures nothing). Layers: channels (+ the
component bounding boxes that ARE `valley_count`), geomorphon vs accepted
ridges, accepted vs rejected basins, depression-fill flats, bench scarp
bands, valley transects, fitted centerlines, nodata, contours. Per-tile knob
values are colored against that archetype's committed quantiles (green
inside q10–q90, yellow in the tails, red outside or null), with the
`extras` gates (anisotropy, dune directionality) alongside. Bad tiles are
culled into `out/exclude.json`, which `fit_knobs.collect()` honors and whose
digest is stamped into the prior's `fit_provenance`. Optional second pane
compares a generated `macro_skeleton/` at a shared elevation scale.
`cargo run -p tile-lab --release [-- <campaign_out> [<skeleton_dir>]]`;
batch PNGs via `cargo run -p tile-lab --example tile_snapshots --release --
<dir>`.

**Second QA pass (2026-07-28) — the viewer paid for itself.** Reviewing the
overlays against the hillshade found four defects that all the scalar
sanity-checking had missed, because a number in range says nothing about
whether it was measured in the right place:

- **Mirrored geometry.** `dtm_primitives/frame.py::rc_to_local` computes
  `y = (nrows−1−row+0.5)·cell` — a row 0 = NORTH convention inherited from
  the atlas GeoTIFF store. Campaign tiles are row 0 = SOUTH (`fetch.py`
  flips the 3DEP export to match the pipeline's y-up world), so every
  polyline the primitives exported was mirrored `y → H − y`: traced ridge
  centerlines landed on the geomorphon ridge mask 39 % of the time against
  a 35 % background — chance. The transect sampler round-trips through the
  same inverse, so **every scalar fit was still valid**; only the geometry
  was wrong. Fixed by adapting at the campaign boundary
  (`extract.primitives_frame` hands dtm_primitives a `np.flipud` view, which
  collapses its formula onto our convention) rather than editing shared
  code. Post-fix the same measurement is 70–99 % on-mask.
- **Straight-line channels.** `metrics.core.d8_accumulation` has no flat
  resolution: ties break to the first cardinal `_D8` direction and diagonals
  lose by the √2 divisor, so every depression-filled lake, pond and graded
  flat drained along a dead-straight axis-aligned ray. New
  `macro_campaign/flow.py` keeps the epsilon fill but resolves flats
  Barnes-style — a BFS inward from the flat's spill cells — so flow
  converges on the real outlet. Lake bodies are also no longer painted as
  channel (only the thalweg crossing them).
- **Bench "bands".** The old detector projected the whole tile onto ONE
  global downhill axis and binned it into 50 slabs, so it could only ever
  emit straight strips and had no way to follow a curved terrace. Replaced
  by `dtm_primitives.blufffit` in the shape pass, which traces real scarp
  **centerline splines** with fitted height and face gradient; valley
  corridors from the same pass suppress plain valley walls.
- **Basin gate too harsh** — accepted depth 1.5 m → 1.0 m
  (`BASIN_MIN_DEPTH_M`); kettles and blowouts that read clearly on the
  hillshade were being rejected.

- **Development contamination — the corpus, not the code.** The QA pass
  found subdivisions, section-line agriculture and a surface mine in the
  exemplar tiles. A new OSM screen (`python3 -m macro_campaign develop`)
  measured it: of the original 27 pilot tiles, the piedmont set ran 5–15 %
  built, florida 8–29 %, moraine 3–64 %. Only sandhills was clean. The
  centers had been chosen for landform character with no populated-land
  test at all. Two fixes: the screen now rasterizes OSM roads/buildings/
  quarries per tile, auto-culls anything over 2 % into `out/exclude.json`,
  and hands the mask to `extract` so graded ground stays out of the
  detectors; and `regions.py` was re-picked onto protected land — Uwharrie
  NF (piedmont), Ocala NF + Three Lakes WMA (florida), Big South Fork +
  Obed (mountain), Chequamegon and Huron-Manistee pitted outwash (moraine),
  Samuel R. McKelvie NF + Valentine NWR (sandhills). Result: all 6 new
  Uwharrie tiles at 0.0–1.8 % against 5–15 % for the 6 they replace, 7 of 8
  new Big South Fork tiles at 0.0–1.9 %, sandhills 10/10 clean. **Selection
  rule going forward: protected or near-unpopulated land only** — the
  screen is a verifier, not a license to keep a built-up tile and mask it.
- **Detector sensitivity (2026-07-31).** Review judged ridges and terrace
  steps under-detected. Instrumenting the funnels showed two unrelated
  causes, neither of which was the acceptance gate people would reach for
  first:
  - *Ridges* — `ridgepipe` was only offering 0.1–4.2 crests/tile, so the
    gate was irrelevant. Branches were arriving with **zero usable reaches**
    (five of seven on one mountain tile), leaving nothing to fit. Transects
    default to a 300 m half-length, which suits a valley with a floodplain;
    interfluves sit 200–600 m apart, so a 300 m ridge transect crosses into
    the neighbouring corridor and is discarded. Shortening to 120 m with
    proportionally shorter reaches (scoped to the ridge call — valleys keep
    the wide defaults, since a floodplain genuinely is that wide) took
    mountain from 1.0 to **3.4 accepted ridges/tile**, and placement quality
    went UP, not down: 91.5 % on-mask against 42.3 % chance, worst tile 80 %.
  - *Terrace steps* — here the gate WAS the limiter, decisively: 98 of 138
    mountain candidates were rejected on length alone and **none** on
    height, against a candidate median length of 236 m versus a 300 m cut.
    A riser is naturally broken into segments by the gullies cutting back
    into it, so a long-unbroken-scarp requirement measures continuity rather
    than the presence of a bench. Cut to 150 m, with `blufffit`'s slope
    threshold relaxed 0.25 → 0.18 to admit gentler risers.

  Campaign-side overrides live in `extract.CFG_OVERRIDES` /
  `RIDGE_CFG_OVERRIDES` rather than in `dtm_primitives/config.yaml`, which
  is the atlas pipeline's tuning too.

  Raising scarp sensitivity then exposed a **semantic** bug that had been
  hidden by the strict gate: `bench_count` was taking blufffit's
  scarp-SEGMENT count, which jumped to ~16/tile. But the knob drives the
  planner's cascade — how many times the regional grade steps down — and 16
  steps across 142 m of relief is a staircase of toy risers, not a bench
  landscape. Two corrections: one terrace edge breaks into several segments
  wherever a gully cuts back into it, so segments are clustered into
  **levels**; and, more importantly, most detected scarps are not cascade
  members at all — in dissected ground they are valley walls and spur noses
  pointing every which way. A cascade riser follows the CONTOUR, so only
  scarps running within 35° of perpendicular-to-downhill are counted
  (`shape._cascade_positions`). Counting levels without the orientation
  filter just re-created the old axis-band detector's 200 m binning. With
  both, mountain measures a median of **3 levels (range 2–6)** — its
  archetype anchor is 2–5 — while florida is 0 and moraine 1 (kettled, not
  terraced). Segment counts remain in `extras.scarp_candidates`.

  End-to-end the fitted counts do reach the generator: mean placed ridges
  went from ~1 to 2.25–4.0 per course over 8 seeds (the planner places what
  it samples). **But they are not yet VISIBLE in the routable core**, and
  the seed-3 mountain render shows why: the ridges survive as isolated
  lumps in the outer ring while the core is nearly featureless, because the
  core-cap budget solve's global fallback scales every amplitude across the
  whole box. Placing more ridges cannot help until that is fixed — which
  makes the core-cap item below the blocker for landform density, not just
  for the relief numbers in the compare report.
- **Two archetype-identity findings fell out of the re-pick.** (1) Ocala
  National Forest looked like the obvious protected stand-in for the Florida
  lake district, but it measures 18–31 m of relief on a sandy karst ridge —
  it is an upland, and mixing it in pulled the fitted `relief_amp_m` median
  to 18 m and broke the "flattest archetype" anchor. The corpus is now flat
  wet protected land (Three Lakes WMA, Kissimmee Prairie, Green Swamp,
  Myakka: 1.1–6.3 m relief); the Ocala tiles are excluded on *character*
  rather than development, which is a legitimate cull reason worth keeping
  distinct in `exclude.json`. (2) Those flat tiles carry almost no closed
  basins (0–1 per tile at the 1 m gate) — real flatwoods water is broad and
  shallow, not a depression field. That is consistent with the pipeline's
  own division of labour: florida's water is a **step-05 water-table**
  phenomenon, not a step-03 macro-basin one, so a near-zero fitted
  `basin_count` here is the correct measurement rather than a detector
  failure. Step 05 should not expect step 03 to hand it lake basins for
  this archetype.

First QA pass findings (2026-07-28): the basin detector counts lakes and
wet interdune flats as basins (correct-by-design for florida, inflating for
sandhills); the bench-band detector fires on dune fields (harmless — the
identity-zero gate already keeps sandhills benches provisional, and this
confirms that gate was right).

## Data campaign

[`tools/macro_campaign/`](../tools/macro_campaign/) (see its README):
archetype exemplar **landscape tiles** (Nebraska Sandhills, NC piedmont,
central-FL lake district, WI Kettle Moraine, Cumberland Plateau margins) are
fetched from USGS 3DEP at 2 m as CGRID1, measured with the ported
`tools/metrics` + `tools/dtm_primitives` stack (fetch → extract → fit), and
fitted into the `landform.*` quantile tables of
`crates/course-spec/data/archetype_priors.json` (values only; every apply is
a course-spec golden re-bless). The course atlas (`tools/dtm_atlas`) is the
validation cross-check.

**Pilot run (2026-07-27): complete.** 27/30 tiles fetched (3 permanent 3DEP
export failures, ≥5 per archetype), all extracted, 37 knob tables fitted and
applied (`prior_version: campaign-pilot-1`), 63 kept provisional under the
gating policy in the campaign README (design knobs, quarantined estimators,
identity-zero families, small-n winsorize). Highlights: mountain benches
measured beautifully on-design (count 2.5, scarp 10.3 m, tread 678 m);
sandhills blowout density confirmed ~10× provisional (47/tile, spacing
184 m); relief medians florida 13.6 / moraine 46 / piedmont 52 / sandhills
78 / mountain 112 m. Applying the fitted relief exposed a real planner bug —
the budget solve scaled the tilt but bowl rims / ridge crests / trunk floors
kept authored-tilt absolute elevations (stranded relief the solve could
never remove); fixed by re-anchoring all absolute elevations to the
effective tilt at `lower()` time (`MACRO_VERSION 2`, goldens re-blessed;
`debug_core` example is the per-component attribution tool).

**Estimator repair + QA pass (2026-07-28), `prior_version: campaign-pilot-2`,
`MACRO_VERSION 3`.** Same 27-tile corpus, extract v3, 50 tables fitted:

- `valley_fall_grad` left quarantine — it was measured on the
  depression-FILLED surface, where lake and pit flats are dead level and
  dragged the median to ~1e-4 (below the Valley primitive's 5e-4 monotone
  floor). Now routed on the filled surface for connectivity but MEASURED on
  the raw one with fill-raised cells excluded: 0.008 (florida) → 0.097
  (mountain).
- `ridge_count` / `ridge_len_m` left quarantine — geomorphon components
  counted every ≥300 m spur (~25/tile even in Florida). Now
  `dtm_primitives.ridgepipe` traced crests gated to *major* (prominence ≥
  max(5 m, 10% of tile relief), length ≥ 600 m — a ridge a hole could be
  routed along): 0–3 per tile, Florida lowest.
- `dune_wavelength_m` stays quarantined, now with a proof rather than a
  suspicion. v2's uniform 384 m had two causes: `mcore.radial_psd` bins
  linearly in WAVENUMBER, leaving only two bins (128/384 m) across the whole
  100–600 m dune band, and red-noise power rises monotonically so the argmax
  is always the band edge. `extract._dune_probe` fixes the instrumentation
  (log-spaced bins along the strongest direction, peak against a fitted
  background) — but a falsification test kills it as a *dune* statistic:
  orientation coherence over the 5 Nebraska Sandhills tiles (R=0.41) is
  LOWER than over the 6 Cumberland Plateau tiles (R=0.75). A real dune field
  shares one wind orientation, so a dune-sensitive statistic would rank
  sandhills highest; this one ranks structural grain highest. Kept in extras.
- **Shape tier** (`python3 -m macro_campaign shape`) runs the full
  dtm_primitives transect pipeline and fills `valley_centerlines` for QA.
  Both meander knobs are now quarantined *with cause*: `meander_intensity`
  is defined against the generator's own curvature clamp and returns exactly
  1.0 on 26 of 27 tiles (a statistic pinned at its ceiling carries no
  information), and `meander_wavelength_mult` inherits a top-width fit that
  swings 0.29–12.19. **Sinuosity measures cleanly and discriminates** (1.18
  confined mountain/piedmont → 1.36 sandhills) and is recorded as
  `meander_sinuosity` — see Open questions for the design change it implies.
- `landform.ridge_crest_hw_m` promoted from extra to knob (measured 7.8 m
  piedmont → 92.5 m sandhills) and the ridge **de-mesa** shaping now reads
  it: crest halfwidth comes from the measurement instead of `0.05·length`
  clamped to 45 m, and `crest_round_m` scales with the halfwidth (0.9×)
  instead of a fixed 12 m that left a 90 m flat top on every ridge.

**Raster performance at 2 m** (`cargo run -p course-macro --example
perf_raster --release`). Bowls dominated: 11 s of a 12 s sandhills raster,
because `ResolvedBowl`'s bbox pad is relief-scaled (~1 km on a 40 m kettle —
correct, since a bowl carves into terrain rising faster than its outer
gradient) so the early-out never fired. Added an exact terrain-aware cull:
`smin` saturates to `z` once the surface clears it by a blend band, and a
cheap centroid-distance lower bound on the surface proves that without the
96-segment projection. Full skeleton: sandhills 58 s → 4.1 s, florida
29 s → 1.0 s, moraine 33 s → 8.6 s. Not byte-identical (the saturated
`smin` branch computes `s + (z - s)`, which drifts by ≤4.3e-14 m; returning
`z` verbatim is the more accurate of the two) — goldens re-blessed, and
piedmont/mountain, which place no bowls, are unchanged, which is the
expected signature. **Mountain is now the worst case at 42 s**: bluffs
deliberately have NO bbox (a step's plateau extends arbitrarily far, so a
cut would leave a hard seam), leaving ~250-segment projections per cell.
Next perf target is a spatial index in `Spine::project`, which would speed
every primitive.

**Generated-vs-real validation.** `cargo run -p course-macro --example
export_tiles --release` writes generated `base_height` as CGRID1 tiles;
`python3 -m macro_campaign compare` measures them with the SAME extractor
and writes `out/report/compare.md`. Against the rebuilt corpus
(`campaign-pilot-3`, 6 seeds × 5 archetypes): **17/60 knobs land inside the
real interquartile range**, and the shortfall is now unambiguous rather than
noisy — generated relief sits far below real in every archetype (florida
0.49 m against a real 2.34 m median). That is the core-cap budget solve
flattening the whole 3 km box; see Open questions.

**Placement verification** (the check the mirror bug should have had from
the start): sampling every accepted-ridge centerline point against the
geomorphon ridge mask over the usable corpus gives **82.4% on-mask against
36.4% chance** (per-tile median 82%, max 100%). Before the fix it was 39%
against a 35% background — i.e. indistinguishable from random placement,
while every scalar stayed in range.

## Network-driven redesign (in progress, 2026-08-01)

Measurement showed the generated surface is *a tilted plane with independent
objects composited on it*, and that this is not fixable by tuning: **94 % of
a generated piedmont tile is within ±10 % of relief of its best-fit plane**
(real 58 %); **local relief at 100 m — hole scale — is 4–11 % of real** while
whole-tile relief is 0.6–1.7×; **median distance to the nearest channel is
104–123 m in every real archetype but 134–590 m generated**; and the drainage
density gap *widens* as the accumulation threshold drops, the signature of a
network with no low-order tributaries. Matching real ridge coverage (39 %
piedmont / 47 % mountain of the tile on a convex ridge) needs ~20–25
interfluves — the deficit is spatial COVERAGE, not amplitude, so no
`ridge_count` value can reach it.

`terrain-v2:golf-landform/src/sampler.rs` already tried trunk+fan, and its
own closure loop recorded **"Network family 95.6x [fail] … noise does not
dissect interfluves"** while its cross-section geometry closed. So the
authored cross-sections are right and deferring dissection to step-04 noise
is empirically known to fail. **Step 03 therefore owns dissection** (a
cross-step contract change; steps 04/05 modulate and erode an
already-dissected surface).

The key structural insight: `valley.rs` surfaces already rise without bound
and compose by `smin`, so the drainage layer *is* the lower envelope of all
valley surfaces — a distance-to-channel hillslope field with rounded
divides. It produces nothing today only because the base sits a few metres
above the floor, clipping every wall into a plateau. **Raising the base by a
hillslope lift makes interfluves emergent**, and the numbers cohere: the
fitted `valley_wall_grade` (piedmont 0.15) *is* the measured hillslope
(0.10–0.14), and `0.0144×3000 + 17 + 25 ≈ 85 m` = the fitted
`relief_amp_m`. `relief_amp_m` becomes a budget identity, incision becomes
an output rather than an input, and `valley_count` is retired as an input
(it counts connected components, so a perfectly dendritic tile scores 1).

**M0 (measurement) complete.** New `macro_campaign/structure.py` (organization
metrics), `netstats.py` (branching topology), `courses.py` (empirical core
cap), and a reworked `compare.py` that finally iterates metrics as well as
knobs — drainage density had never been gateable because `compare` only
walked `fit_knobs.KNOBS`. Raising the network extraction caps
(`max_depth` 2→5, `max_tributaries` 6→40) plus a ridge-style transect fix
(300 m half-length on channels 170 m apart was discarding cross-sections)
unlocked real topology: **18–35 branches/tile, Strahler 3, depth 4–5**,
where the old caps could only ever report a depth-1 star.

Two bugs fell out. `branch.parent` indexes the ORIGINAL network while
`extract_valleys` drops branches that fail to fit, so a 13-result tile
carried parent indices 15 and 19 — every topology statistic was linking the
wrong reaches (`netstats._resolve_parents` now walks to the nearest
surviving ancestor). And the `compare` IQR test has a **60–100 % false-flag
rate** at this sample size, so the gates use ratio bands instead, calibrated
against a real-vs-real split-half spread of 1.02–1.17× (max 1.56×).

Measured targets (medians; the first four are near-constant across
archetypes, which is what a space-filling network looks like):

| target | florida | moraine | mountain | piedmont | sandhills |
|---|---|---|---|---|---|
| dist to channel p50 (m) | 123 | 119 | 111 | 110 | 113 |
| frac within 150 m | 0.60 | 0.62 | 0.64 | 0.66 | 0.62 |
| drainage density 1× | 2.34 | 2.30 | 2.29 | 2.21 | 2.45 |
| Strahler max | 3 | 3 | 3 | 3 | 2.5 |
| junction angle (°) | 44 | 54 | 62 | 61 | 53 |
| slope–area θ | 0.40 | 0.30 | 0.28 | 0.35 | 0.41 |
| local relief 100 m (m) | 0.70 | 6.1 | 16.4 | 12.4 | 10.1 |
| ridge-mask area frac | 0.03 | 0.33 | 0.47 | 0.41 | 0.25 |

**The core cap was too tight, empirically.** `courses.py` measured real OSM
golf courses over the same 1.5 km window (a built course is routable by
definition, so the relief its architects accepted IS the constraint):
florida **10.5 m** vs an authored cap of 3, sandhills **32.9** vs 15,
mountain **72.5** vs 40, moraine **33.2** vs 22, piedmont **30.5** vs 25 —
tight by 1.2× to 3.5×. Also notable: courses do NOT sit on median landscape.
In flat archetypes they pick *more* relief than typical ground (florida
10.5 vs a 1.9 m landscape median) and in steep ones *less* (piedmont 30.5 vs
56.3). Report: `out/report/course_relief.md`.

Baseline before the redesign: **27 structural gate failures**, worst being
`dist_to_channel` 4.64× and `local_relief_100m` 0.03×.

**M1 (perf) complete.** Exact early-outs on Valley and Ridge — the mirror of
the Bowl cull: `smin` saturates to `z` once the surface clears it by a blend
band (and `smax` symmetrically), and a one-distance bound against the
un-padded spine bbox proves that without the projection. The padded bboxes
stay relief-scaled (1.3–6.1 km on a 3 km world, so they never cull); this is
what actually culls. Plus a `SegIndex` uniform-grid segment index in
`course-world`, used by the per-cell nearest-drain query, which is the one
field cost that scales with valley count.

Result: raster **2.7–2.9× faster** (sandhills 5.1→1.9 s, piedmont 8.8→3.0 s,
moraine 13.0→4.6 s) and — the point of the milestone — **30 valleys rasterize
in 10.4 s with sub-linear scaling** (15× the valleys for 2.5× the time), so
a real network is affordable.

Two findings worth keeping. The bounds were wrong twice before they were
right, both times by using the `.max(0.05)`-clamped gradient that the *pad*
uses: `ramp` grows with gradient, so clamping it up inverts the bound and
culls cells the primitive still shapes. And a ridge crest may RISE with arc
(`fall_gradient` may be negative), so both ends must be considered. Both are
now pinned by property tests that sweep the bound against the surface.

The goldens moved, and the justification is proven rather than asserted: the
bounds are property-tested sound, and `valley_early_out_is_ulp_scale`
measures the residual where the early-out fires at <1e-9 m — arithmetic
association (`s + (z - s)` vs `z`), not geometry. Florida is unchanged, which
is the expected signature since it places neither valleys nor ridges.

**A spine index does NOT help bluffs** — it measured 41 s → 172 s. A bluff
has no bbox by design (its terrace extends arbitrarily), so its spine is
queried from everywhere, the candidate set is never small, and the per-query
allocation beats a 300-segment scan. Mountain stays the worst case at ~34 s.
The lever there is an along-spine band bound, since the end taper zeroes the
step past the spine ends — recorded as future work, not attempted.

**M2 (primitive plumbing) complete** — all of it back-compatible via serde
defaults, so the goldens did not move:

- `apply(p, z, w)` on Valley/Ridge/Bluff/Bowl, with `Resolved::core_weight`
  hoisted out of the primitive loops in `height_at`. This is the world-space
  handle the budget solve will turn, replacing `CoreDip`'s arc-space spans;
  it is C1 because `core_protect_at` is a product of smoothsteps. Valleys
  are never weighted — their incision stays placement-capped.
- `Tilt::core_tread`: the core band keeps `core_tread` of the regional
  grade and the outer bands take the rest, chosen so the **end-to-end drop
  is conserved**. That is the whole point — scaling the grade destroys
  `relief_amp_m` (a fitted knob) to satisfy `core_relief_cap_m` (a design
  one), whereas a tread redistributes it. Geometrically it is a bench tread.
  Pinned by three tests: unit tread is the plain plane bit-for-bit, total
  drop is conserved across grades and tread values, and the core is
  genuinely flatter while the profile stays monotone (drains keep falling).
- `Valley::fall_profile` + a 128-station cumulative-drop table, so one
  Valley per reach can carry a concave slope–area long profile instead of a
  single scalar. Monotone by construction (knots clamp at
  `MIN_FALL_GRADIENT`). Accordance now rises by the tributary's own
  cumulative drop rather than `grad × arc`.
- **Depth-4 accordance property test.** `join_trunk` indexes any earlier
  valley so nesting was always legal, but the planner only ever emitted
  `Some(0)` — every production junction is depth 1, and M5 will emit depth
  3–5. The test chains trunk → trib → sub-trib → sub-sub and asserts exact
  junction floors plus monotone reaches. It **passed first try**, so the
  corridor trim and overshoot really are depth-agnostic; that materially
  de-risks M5.

`height_at_5` (batching the 5-point slope stencil) was considered and
**deliberately not done**: the early-outs test against the running composed
height, which differs per stencil point and evolves differently through the
primitive chain, so a shared cull cannot be bit-identical without bounding
all five running values. The M1 work already leaves 30 valleys at ~10.5 s.

**Progress page** (`python3 -m macro_campaign report` →
`out/report/progress.html`). The redesign's evidence lived in a markdown
table, which cannot show that a generated tile is a plane with objects on
it. The page draws the gap two ways: side-by-side hillshades of a real
exemplar and a generated course per archetype, and the four diagnostic
charts (distance to channel, relief vs window, drainage density vs
threshold, plane-residual and ridge coverage) with the real IQR against the
generated median and the gate's accepted zone. It measures nothing — every
value is read from `out/extract/**` and `out/generated_measured/**`, and the
gate bands and noise floor come from `compare.GATES`/`compare.baseline`, so
the page and `compare --gate` cannot disagree (both report 27 failures
today). Two decisions matter for it to be honest: **both image columns go
through one renderer** (`dtm_atlas.qa.hillshade_rgb` on the raw `.cgrid`,
not the two Rust snapshot examples, which draw overlays and normalise
elevation differently), and the **representative tile is picked, not
chosen** — the tile whose 200 m relief is nearest its own side's median,
ties to the lowest name. The shared-elevation-scale toggle re-stretches both
images to the same span (each re-centred on its own median, since a literal
union of absolute elevations would paint a 900 m mountain tile and a 40 m
generated one flat); that is the only view in which the relief deficit reads
as a picture. Re-run after each milestone — the same charts, moving.

Two things the page made visible that the table had not. The generated
sandhills tile is a near-flat plane with a scatter of isolated blobs where
the real one is a fully dissected dune field — the "objects on a plane"
diagnosis, as an image. And **piedmont seed3 carries a 79 m flat-topped cone
with ~0.65 flank gradient** rising off an otherwise 105 m plane (peak
residual +25 m against a 61-cell mean; the same signature appears at +17.6 m
on seed5 and +18.7 m on sandhills seed3). A piedmont interfluve is not a
cliff — ridge crest elevation is being anchored far above local terrain.
Recorded here rather than chased: it is placement, which is what M5/M6
replace.

**M3 (budget solve) complete.** New `budget.rs` replaces `corecheck`'s global
fallback with staged bisections on three knobs, cheapest lever first, over a
fixed two rounds: `Tilt::core_tread` (redistributes the regional grade out of
the core, conserving the box drop exactly), `core_relax` (the M2 world-space
core weight), and a new `incision_scale` (uniform on the drain system). Two
rounds because the knobs interact — piedmont seed 7's tread could not bite
until the incision stage had shallowed the drain its floor is anchored to.
Each stage keeps its knob only if it measurably helped, reverting to the
knob's PRIOR value rather than to 1.0 (reverting to 1.0 threw away an earlier
round's gain — an 11.8 m regression the guard itself introduced).

Result: **200/200 seeds across all five archetypes now meet the cap**, where
the plan as authored exceeded it on 41/200 and the old solve only "met" it by
shrinking the entire 3 km box. Golden movement is the diagnostic: **only
MountainBench moves**, because at seed 1 the other four archetypes are
already under cap and the solve returns with every knob at exactly 1.0,
lowering a bit-identical config.

Chasing the residual failures turned up four real defects, none of which the
scalar checks had caught:

- **`Bluff::apply` shadowed its core-weight parameter** with the eased-step
  width, so bench scarps ignored the budget solve entirely and only ever
  responded to the `w <= 0.0` early-out. Invisible while the solve was global
  (nothing ever passed a fractional weight); the moment M3 bisected, mountain
  core relief was IDENTICAL at every relax from 0.5 down to 0.05 and then
  collapsed at exactly 0.0 — a switch, not a blend. Five 20-25 m scarps are
  most of a mountain core's relief, so the solve had no lever at all.
- **`Tilt::tread_profile` reversed slope near the knee.** Blending two
  elevation branches puts a `(z_out - z_in)·w'` term in the derivative that
  is negative just inside the knee; at `core_tread` 0.1 the outer band runs
  at 1.9× the regional grade and it dominates, scalloping a dip into the core
  edge. The flattening lever made piedmont seed 7 LESS flat (58.5 → 59.2 m).
  Fixed by integrating the blended gradient instead, which cannot reverse
  (the integrand never drops below `core_tread·g`) and conserves the drop
  exactly, since a smoothstep integrates over its knee to exactly the knee
  half-width. M2's monotonicity test passed straight through the bug because
  it only swept a mild tread; `a_strong_tread_never_reverses_slope` now
  sweeps to the solve's floor and fails on the old formulation.
- **The trunk's incision and its through-core drop were budgeted separately**
  against the same cap — each capped at 0.6·cap — so the planner could author
  a core at 1.2·cap by construction. The old solve hid this because its
  global pass scaled `tilt_grade_eff`, which scaled every valley's fall
  through `t_ratio`. Budgeting them together cleared the cap failures in
  florida, sandhills and moraine outright.
- **Tributary depth was budgeted not at all.** A tributary's depth below the
  terrain is emergent — its floor is accordance-snapped to the trunk and
  rises upstream by its own cumulative drop, with fall drawn at 1.4-2.6× the
  trunk's. This was the last mechanism standing, and `incision_scale` now
  reaches it.

`core_relax` may weight VALLEYS only when no drain centerline anywhere feels
the core weight (`DrainagePlan::floors_clear_of_core`), because easing a
carve where the floor is inside the core lifts that reach relative to its
neighbours and ponds the drain. The precondition is measured on the RESOLVED
spines, not inferred from `tangent_routed`: junction snapping trims and
overshoots a tributary's tail, and that put a moraine trib on (672.5, 2398.9)
— inside the shoulder corner by a hair — while every authored point was
clear. Piedmont seed 7 ponded 2.53 m under the inferred version. Where the
precondition fails, `incision_scale` is the lever instead, and it is provably
safe: scaling incision shifts the whole floor by one constant, so every
longitudinal slope is unchanged.

Worth recording as a falsified hypothesis: **raising the cap makes cap
compliance WORSE, not better.** Incision is a share OF the cap, so a bigger
cap digs a deeper valley and flips `tangent_routed` off for more seeds —
mountain goes from 17/40 to 28/40 over-cap seeds as the cap is raised 1.6×.
M4's empirically re-derived (larger) caps therefore need the M3 solve in
place first, not the other way round.

**M4 (prior) complete** — `campaign-m4`, fingerprint
`87ed843b69735cfad41ec460f6bbf820f2e476fda6a07f8012ff66e5c92dff27`.

**`core_relief_cap_m` is now sourced from real golf courses.** It had been in
`DESIGN_KNOBS` — never fitted, on the correct reasoning that real landscapes
are not golf-routable so a landscape corpus cannot say what a routable core
may contain. But a *built course* is routable by definition, which makes
`courses.py` a legitimate second corpus, and it is a different corpus rather
than a different estimator. New `COURSE_SOURCED` gating fits the cap from
`out/courses/*.json` with the same winsorized quantile policy. The authored
caps were tight by 1.2× (piedmont) to 2.2× (sandhills): florida 3 → 10.5,
sandhills 15 → 32.9, piedmont 25 → 35.1, moraine 22 → 33.2, mountain 40 →
72.5 m.

**A corpus defect found first.** The piedmont and sandhills course search
boxes overlapped around Pinehurst, and since courses are ranked by area, all
SIX "piedmont" courses came back as courses sandhills had already claimed —
piedmont's empirical cap was measuring the Sandhills of North Carolina. With
a disjoint box over the Uwharrie/Yadkin piedmont it is 12 genuine piedmont
courses and the median moves 30.5 → 35.1 m. `courses.run` now asserts the
boxes are disjoint and dedupes across archetypes as a backstop; the assertion
caught my own first replacement box, which still clipped the corner.

**The M3 finding no longer holds, which is why M4 had to follow M3.** Before
M3, raising the cap made cap compliance *worse* (mountain 17/40 → 28/40
over-cap seeds at 1.6×), because incision is a share OF the cap. With the
core-localized solve it is 0/30 over cap at 1.0×, 1.5× and 2.2×, and core
relief tracks the cap instead: mountain's mean rises 38.7 → 82.5 m against a
real landscape median of 73.7. The cap is now the lever that closes the
core-relief gap rather than the thing fighting it.

**Ten network knobs added** for M5 to grow the network from — junction
angle/spacing, branch length, junction area log-ratio, Strahler max,
slope–area θ and its pivot intercept, channel half-width at the pivot, and
`meander_sinuosity` (the measurable replacement for the quarantined
`meander_intensity`). They are inert until M5 reads them; fitting them now
means the corpus, the cull and the fingerprint move once instead of twice.
Provisionals are the POOLED corpus — real measurement, just not
archetype-specific — replaced per archetype wherever that archetype has
≥ MIN_N of its own. `collect()` now reads `extras` as well as `knobs`,
because which block a measured quantity landed in is shape-pass bookkeeping,
not a claim about whether it can inform the prior; promoting one no longer
costs an hour-long re-run of the transect pass.

**`chan_hw_area_exp` is quarantined on a sign error.** Hydraulic geometry has
width growing with drainage area (w ~ A^b, b ≈ 0.3–0.5). Fitted: 0.40
piedmont, 0.27 mountain, 0.20 moraine — but **−0.34 sandhills, with 4 of its
6 tiles negative**. A negative exponent says the channel narrows downstream,
which no real network does; it is the log-log fit collapsing on the 3–6
reaches one tile offers across a narrow area range. The pivot-centred
intercept `chan_hw_at_a0_m` survives it (that is what the pivot is for) and
is fitted. Repair is to pool reaches ACROSS tiles within an archetype so the
regression sees two decades of area instead of one.

The network family joins `valley_count`'s identity-zero family, so florida
and sandhills — whose archetype identity is "no through drainage" — keep
pooled provisionals rather than being fitted from whatever the detectors
found. Goldens: all five moved, the expected signature, since every
archetype's cap changed and the cap feeds trunk incision, the tangent-routing
decision and the budget solve alike.

## The primitives question, settled by measurement (2026-08-02)

After M4 the pictures still looked wrong — placed ridges in particular read as
random, **because they are**: the planner draws four candidate centres
uniformly and keeps whichever is farthest from a drain
([`planner.rs`](../crates/course-macro/src/planner.rs) stage 3b). Nothing ties
a ridge to the drainage. That raised a fair question: are swept spline
primitives the wrong tool, and should macro character instead be synthesized
from geomorphon-ridge / flat-fill structure?

**Two measurements answered it.**

*First, on real tiles.* Fit `z = z(nearest channel) + g·dist_to_channel` — a
hillslope field hung on a channel network, which is exactly what an `smin`
envelope over valley surfaces computes — and compare against the tile's own
best-fit plane, which is the shape the generator currently emits:

| archetype | R² hillslope field | R² best-fit plane |
|---|---|---|
| glacial_moraine | **0.874** | 0.325 |
| piedmont | **0.784** | 0.564 |
| sandhills | **0.650** | 0.035 |
| florida_lowland | 0.418 | 0.028 |
| mountain_bench | 0.356 | 0.145 |

So the fluvial archetypes ARE a distance-to-channel hillslope field, and the
representation was never the problem. Where the model is weak — mountain and
florida — the dominant control is structural benches and closed karst basins,
which have their own primitives. The three primitive families map onto the
three real macro controls.

*Second, on a spike.* `examples/spike_network.rs` builds ONE hand-wired config
— the grown 48-reach network, slope-area long profiles, hillslope lift, **no
placed ridges, no bowls, no bluffs** — and the real extractor measures it:

| metric | real piedmont | pre-M5 generator | spike |
|---|---|---|---|
| dist_to_channel_p50 | 110 m | 414 m (3.78×) | **126 m (1.15×)** |
| local_relief_100m | 12.4 m | 1.8 m (0.15×) | **16.4 m (1.33×)** |
| plane_residual_frac | 0.52 | 0.88 (1.69×) | **0.32 (0.62×)** |
| ridge_mask_area_frac | 0.41 | 0.11 (0.26×) | 0.26 (0.63×) |
| drainage density (3 thresholds) | — | 1 of 3 pass | **3 of 3 pass** |

**6 of 7 structural gates pass, untuned, with every ridge deleted.** The
metric that has been the project's headline failure since M0 —
`local_relief_100m` at 0.03–0.18× of real — lands at 1.33×. Raster cost is
2.7 s at 2 m for 48 valleys, so the primitive count is affordable.

**Verdict: keep the primitive engine; the fault was usage, not vocabulary.**
Two to four valleys plus randomly-placed ridge objects, where the correct
construction is a space-filling network whose lower envelope IS the hillslope
field and whose interfluves emerge as the network's complement. `smin`
composition over a dense network is a legitimate analytic representation of
that field — and it keeps hard requirement 4 (a coarse grid is a bit-exact
node subset of a fine one), which a raster-derived synthesis would forfeit.

**But the one failing gate is a real finding, and so is what happened when I
chased it.** The divides are CREASES where two planar walls intersect; real
interfluves are broad convex crests, which is what the geomorphon ridge class
needs area of. Widening the shoulder smooth-min band rounds them, and sweeping
it 3 → 45 m raises `ridge_mask_area_frac` 0.258 → 0.330 while pulling excess
relief down (16.4 → 13.4 m) — **7 of 7 gates pass at k=45**. Then the picture:
at k=45 the terrain is a smooth blobby surface with knife-slash channels,
visibly WORSE than k=3, because a band that wide swallows the valley floor
everywhere except the centerline. *The gates can be satisfied by a surface
that looks wrong* — the same lesson as the detector-placement work, in the
other direction. **Do not tune k to pass the gate.**

The right fix is geomorphic rather than numerical: `Valley`'s wall is a
constant-gradient ramp, where a real hillslope is CONCAVE near the channel
and CONVEX near the divide (creep-dominated). A convex-upward wall profile
produces a broad crest AND keeps the channel defined, which the smooth-min
band cannot do simultaneously. That is the M5b shaping change.

### The gate set is under-constraining — measured twice

`Valley` gained `crest_k_m` / `crest_rise_m`: a smooth-min band that GROWS
with height up the wall, so the channel seam keeps a small band while the
divide seam gets a large one. `None` reproduces the constant band exactly,
so no golden moved. Sweeping the crest band on the spike:

| crest_k | dist_chan | density 1x | local_relief_100m | ridge_frac | gates |
|---|---|---|---|---|---|
| real | 110 m | 2.21 | 12.4 m | 0.408 | — |
| off | 126 (1.15×) | 1.92 | 16.4 (1.33×) | 0.258 | 6/7 |
| 50 | 137 | 2.11 | 9.4 | 0.235 | 6/7 |
| **90** | **111 (1.01×)** | **2.27 (1.03×)** | 9.8 (0.80×) | 0.242 | 6/7 |

At crest_k 90 the network metrics are essentially EXACT — distance to
channel 1.01×, drainage density 1.03×, all three thresholds inside band.
Then look at it: smooth blobby worms with hairline slashes, visibly worse
than the crisp version. **Second time in one session that the numbers
improved while the terrain got worse** (the first was the constant-band
sweep to k=45).

That is not a nuisance, it is the finding: **every gated metric except
`ridge_mask_area_frac` is satisfiable by SMOOTHING.** Distance-to-channel,
drainage density, local relief and plane residual all improve as the surface
blurs, because a blurred surface still has channels in the right places.
Only the geomorphon ridge fraction resists, which reframes it — it is not the
annoying last gate, it is the only one carrying shape information, and it has
been flat at 0.24-0.27 (real 0.408) through every tuning attempt.

Two consequences for the remaining work:

1. **Stop tuning composition parameters.** The residual is that a
   constant-gradient wall classifies as planar SLOPE, not ridge; real
   hillslopes are convex over their whole upper half (creep), which is why
   41% of a real piedmont tile is geomorphon-ridge. Only a convex wall
   profile moves that number honestly.
2. **The gate set needs a shape-sensitive member before it can be trusted to
   accept M5.** Candidates already measured by the extractor: `slope_median`
   (the c=90 blur would fail it), the curvature/aniso family, or a
   hypsometric-integral comparison. Add one to `compare.GATES` in M7 and
   re-check every "pass" recorded so far against it.

### Both landed, and the shape gate immediately earned its place

`slope_median` is now an eighth gate at (0.6, 1.6) — its real-vs-real noise
floor is 1.10× p50 / 1.48× max, comfortably inside the band. `Valley` gained
`wall_convexity`: the wall gradient decays to `1 - convexity` of its channel
value over `crest_rise_m` of rise, integrating a smoothstep in closed form.
It is deliberately barred from reaching zero gradient (clamped at 0.85) — a
wall that goes flat never clears the surrounding terrain, so `smin` would
carve to the bbox edge and seam there. Convexity is a shape, not a ceiling.
`0.0` is the default and reproduces the straight ramp bit-for-bit, so again
**no golden moved**. The M1 lower bound had to be shaped identically (it only
ever LOWERS the surface), and `valley_lower_bound_never_exceeds_surface` now
sweeps convexity alongside the wall gradients rather than trusting that.

Sweeping convexity on the spike, against the eight gates:

| convexity | ridge_frac | slope_median | local_relief_100m | dist_chan | gates |
|---|---|---|---|---|---|
| real | 0.408 | 0.112 | 12.4 m | 110 m | — |
| 0.00 | 0.258 | 0.151 | 16.4 | 126 | 7/8 |
| **0.40** | **0.293** | **0.093** | **11.4 (0.92×)** | 132 | **8/8** |
| 0.70 | 0.364 | 0.046 | 6.3 | 144 | 6/8 |
| 0.85 | 0.364 | 0.024 | 3.2 | 171 | 4/8 |

**The new gate did exactly the job it was added for.** Convexity 0.70 and
0.85 push `ridge_mask_area_frac` to 0.364 — the best value any lever has
produced, 0.89× of real — and under the old seven-gate set that would have
read as progress. `slope_median` catches them at 0.41× and 0.21×: those
surfaces reach the ridge target by going flat, which is the same degenerate
the blend-band sweeps found from a different direction. Only 0.40 improves
ridge coverage while keeping the slopes real.

### Sinuosity: consumed, and it trades against ridge coverage

`network::wander` bends a grown reach to a target sinuosity by bisecting the
amplitude of a half-wave sine in normalized arc. The half-wave matters: the
offset vanishes at BOTH ends, so the head stays put and the mouth stays
exactly on the parent centerline, which is what the accordant junction snap
assumes (pinned by `wander_pins_both_endpoints`). Lobe count and phase come
from the reach index, so it is deterministic without drawing RNG and reaches
do not all wiggle in step. Sinuosity ≤ 1 is an exact no-op.

Swept on the spike at convexity 0.40:

| sinuosity | ridge_frac | slope_med | local_relief_100m | density 1x | plane_resid | gates |
|---|---|---|---|---|---|---|
| real | 0.408 | 0.112 | 12.4 m | 2.21 | 0.521 | — |
| 1.00 | **0.293** | 0.093 | 11.4 | 1.85 | 0.325 | **8/8** |
| 1.26 | 0.236 | 0.092 | 12.0 | 1.70 | 0.400 | 7/8 |
| 1.42 | 0.248 | 0.092 | **12.2 (0.99×)** | **2.05 (0.93×)** | **0.423 (0.81×)** | 7/8 |

Wandering channels improve nearly everything — local relief lands at 0.99× of
real, drainage density at 0.93×, plane residual climbs from 0.325 toward the
real 0.521 — and cost ridge coverage, which drops out of band. **Both are
area claims on the same tile**: a sinuous channel corridor occupies ground
that a straight one leaves as interfluve. Do not split the difference by
tuning; the likely real cause is that the reaches are too WIDE (the
`chan_hw_at_a0_m` intercept plus the wander envelope) rather than that
sinuosity is wrong, since the fitted 1.26 is a measurement and the width
intercept rests on 5 usable piedmont tiles with a quarantined exponent.
Check reach width against real channel corridors before touching either.

### A fitted mean gradient is not a profile's gradient parameter

The ridge/sinuosity trade resolves, and the cause is a measurement-reading
error rather than a shape one. `valley_wall_grade` (piedmont 0.150) is a
whole-hillslope AVERAGE. Feeding it in as the NEAR-CHANNEL gradient of a
convex profile makes the whole hillslope too gentle, because convexity then
decays 0.150 downward from there — so every channel sits in a ~270 m wide
corridor of gentle ground that classifies as SLOPE, not ridge, and ridge
coverage is capped no matter what else is tuned. The near-channel value has
to exceed the mean for the mean to come out right: 0.25 decaying to ~0.10
over the hillslope averages ≈ 0.15, which is the fitted number.

Sweeping it at convexity 0.6, sinuosity 1.42:

| wall @channel | ridge_frac | slope_med | local_relief | density | gates |
|---|---|---|---|---|---|
| real | 0.408 | 0.112 | 12.4 | 2.21 | — |
| 0.150 | 0.266 | 0.061 | 8.4 | 2.03 | 6/8 |
| **0.250** | **0.308** | **0.104 (0.93×)** | 14.7 | **2.27 (1.03×)** | **8/8** |
| 0.350 | 0.332 | 0.152 | 21.2 | 2.38 | 6/8 |

### Stop tuning: three passes, three wrong pictures

That 8/8 configuration still renders wrong — a huge smooth dome over one
corner and channels that ZIGZAG rather than meander. **This is the third
time in the session that a lever drove the gates to a pass while the terrain
got worse** (constant blend band → k=45; crest band → c=90; now wall gradient
→ 0.25). Three different levers, same outcome. The conclusion is not "tune
more carefully", it is that the gate set still does not constrain shape
enough to be used as the optimisation target, even with `slope_median` added.

Two concrete structural defects are visible in that render and should be
fixed before any further measurement:

1. **`network::wander` makes zigzags, not meanders.** It resamples to 12
   stations and offsets by a sine; at sinuosity 1.4 the amplitude is large
   enough that the Catmull-Rom through those stations turns sharply. The
   crate ALREADY has a proper curvature-limited meander generator
   (`prims/meander.rs`, `MeanderSpec`) that the trunk uses — `wander` should
   solve for a `MeanderSpec` that achieves the target sinuosity rather than
   rolling its own offsets.
2. **The hillslope lift keys off the worst corner.** `lift = wall_grade *
   d_max` uses the single farthest point from the network (402 m), so at
   wall 0.25 the plane sits 100 m up and every sparsely-served region becomes
   an enormous smooth dome. It should key off a TYPICAL divide distance, and
   the growth should be made more uniform so the max is not an outlier —
   `MIN_HEAD_DIST_M` currently lets growth stop while one corner is still
   400 m from a channel.

Neither is a tuning constant; both are construction bugs the pictures found
and the metrics did not.

### Both fixed — and the residual is a knob INCONSISTENCY, not a bug

`wander` now resamples to 40 stations and caps amplitude by LOBE aspect ratio
(0.30 of a lobe's half-wavelength) rather than by total length. A sine lobe's
peak curvature goes as `A(pi/L)^2`, so bounding `A/L` bounds how sharply the
channel turns; without it the bisection was free to reach a high sinuosity
with a few enormous lobes. The test now asserts it never OVERSHOOTS and
always wanders, since the curvature cap may legitimately leave a short reach
below target — `sinuosity_of` is exported so callers measure rather than
assume. (The earlier note here said `wander` should solve for a `MeanderSpec`
instead. That is wrong and is why `meander_intensity` is quarantined: the
generator's curvature clamp cannot reach real channel sinuosity, which is the
whole reason `intensity` pins at 1.0 on real data. Bounding curvature
explicitly is the honest version of the same idea.)

The lift now keys off the **p90** divide distance (233 m) instead of the max
(402 m), so one badly-served corner no longer sets the height of the whole
regional plane. 8/8 gates hold, with `slope_median` at 0.91× and
`drainage_density_4x` at 1.01×.

The render still shows one large smooth arc, and the cause is neither of the
above — it is that **two fitted knobs disagree**:

| quantity | value | drop across the box |
|---|---|---|
| `tilt_grade` (regional) | 0.0144 | 43.2 m |
| slope-area gradient at the trunk's area (9 km²) | 0.0092 | 30.1 m |

The regional plane falls 43 m while the trunk it is lifted above falls only
30 m, so a CONSTANT lift cannot hold at both ends: downstream the plane sinks
below the valley envelope and shows through as un-dissected ground. At the
largest scale the regional slope IS the trunk's slope — a landscape's overall
grade is set by its master stream — so `tilt_grade` and
`net_fall_at_a0`/`slope_area_theta` are measuring the same thing twice and
currently disagree by 1.6x.

**CORRECTION — that diagnosis was wrong, and testing it produced a more
useful fact.** Deriving the regional grade from the slope-area law (0.0144 →
0.0092) changes the output NOT AT ALL: identical relief to 0.1 m, identical
metrics to three decimals, identical render. The tilted plane is not
participating in the composed surface anywhere in the box. Once the hillslope
lift puts it above the valley envelope, `smin` picks the envelope at every
cell, so the plane's grade is unobservable.

The two knobs therefore do not conflict — the tilt simply has no say. The
real consequence is larger than the supposed blocker:

**In the network-envelope construction, `tilt_grade` stops being a shape
input.** The regional grade is an OUTPUT of the trunk's long profile, exactly
as geomorphology says it should be (a landscape's overall grade is its master
stream's grade). That is the same move `relief_amp_m` made when it became a
budget identity, and it means the planner's frame stage — which currently
authors `tilt_grade`, `tilt_grade_eff` and a curvature term, and hangs every
absolute anchor off them — is doing work the network will overrule. Settling
what the frame stage is FOR is the first question of the integration, not a
side issue.

The residual smooth arc is consequently not the plane showing through. It is
inside the envelope itself, and binning local relief by distance-to-channel
says exactly what kind of problem it is:

| local relief by distance-to-channel quartile | near → far |
|---|---|
| spike | 18.6 → 17.3 → 13.3 → **12.5** m |
| real | 5.2 → 5.6 → 6.2 → **8.9** m |

(Ad-hoc square-window measure, so absolute values are not comparable with
the extractor's `local_relief_100m`; the TREND within each tile is.)

**The sign is inverted.** In the spike, ground gets SMOOTHER the farther it
is from a channel. In a real tile it gets ROUGHER. That is not a coverage
hole and not a wall-shape defect — it is intrinsic to a distance-function
surface: an `smin` envelope is smoothest exactly where it is farthest from
its generators, so divides come out as the flattest ground on the tile, while
real divides are among the roughest.

This is the same wall terrain-v2 hit from the other side when its closure
loop recorded "noise does not dissect interfluves". The remedy is not more
noise and not a distance-weighted noise field: **it is more NETWORK at finer
scale** — low-order channels reaching up into the interfluves, which is what
makes a real divide rough. The corroborating number is already in the gate
table: `drainage_density_0p25x` is the spike's weakest passing metric at
0.75x, and that threshold is precisely the one that counts fine tributaries.

Concretely, for the next pass: `MIN_HEAD_DIST_M` (140 m) and the
`target_len_m` derived from drainage density together stop growth while the
divides are still bare. Growing to the FINE-threshold density rather than the
nominal one, and letting heads insert closer, is the experiment — not another
shaping parameter.

**Run, and it is the answer for ridge coverage.** Growing to the fine
threshold (4.30 km/km², real `drainage_density_0p25x`) instead of the nominal
2.25 gives 125 reaches and Strahler 4, at the same 3.7 s raster:

| growth | near→far local relief | ridge_frac | gates |
|---|---|---|---|
| real | 5.2 → 5.6 → 6.2 → 8.9 | 0.408 | — |
| nominal 2.25 | 18.6 → 17.3 → 13.3 → 12.5 | 0.297 | 8/8 |
| **fine 4.30** | 18.0 → 17.4 → 13.0 → 10.9 | **0.412 (1.01×)** | **8/8** |
| 6.00, heads 70 m | 16.9 → 14.8 → 9.8 → 0.9 | 0.541 (over) | 8/8 |

`ridge_mask_area_frac` — the metric that resisted the blend band, the crest
band, convexity and wall gradient alike — lands at 0.412 against a real 0.408
purely by growing the network to the density the real corpus has. It was
never a shaping problem. Note also that heads-closer (70 m) does nothing at
4.30 and only bites at 6.00: the LENGTH target is the binding constraint, not
the spacing floor. `GrowthSpec.min_head_dist_m` is now overridable and
`MAX_REACHES` raised to 240 to allow it.

**The relief-vs-distance trend is still inverted, and no growth setting fixes
it** — at 6.00 the far quartile collapses to 0.9 m, i.e. denser growth makes
divides SMOOTHER still. That is the distance-function property again and it
will not yield to the grower. It is the honest hand-off to step 04: with a
real network the interfluves are now small, which is the condition under
which noise can plausibly dissect them where terrain-v2's could not.

Remaining: M5 proper (move the spike's construction into the planner, replacing
the trunk+star; the network grower is built and tested in
[`network.rs`](../crates/course-macro/src/network.rs)), M5b the convex
hillslope profile, M6 retire placed ridges for fluvial archetypes (keep them
for dune crests and bench risers), M7 close. Note `budget.rs` runs up to 96
preview evaluations and M5 makes each ~20× dearer, so trimming `BISECT_ITERS`
moves from M7 into M5.

## References

- `terrain-v2:golf-landform/src/valley.rs` — monotone swept-profile valleys,
  accordant junctions (ported verbatim to `course-macro/src/prims/`).
- `archetype-pipeline:course-contracts/src/{stages,gridio}.rs` — the
  MacroSkeleton types + CGRID1 (ported to `course-macro`/`course-world`).
- `archetype-pipeline:course-lab` — the viewer interaction pattern.
- `terrain-v2:tools/{dtm_atlas,dtm_primitives,landform_prior,metrics}` —
  campaign machinery (ported to `tools/` on this branch).

## Open questions

- ~~Should the analytic primitive config be part of the artifact?~~
  Resolved 2026-07-26: shipped as ADVISORY sidecars (`plan.json`,
  `config.json`); the contract stays raster + structure.
- Bench geometry: uniform tread grade vs terraced quantization — current
  build uses flat treads + eased scarps; revisit after mountain tiles from
  the campaign land (fit both models, keep the residual winner).
- ~~Real sandhills basin density ~10× provisional~~ Resolved 2026-07-27:
  fitted (`basin_count` med 47, winsorized hi ~78).
- ~~Profile the per-cell bowl cost at 2 m~~ Resolved 2026-07-28: exact
  early-out, sandhills 58 s → 4.1 s. Next perf target is a `Spine::project`
  spatial index for the unbounded bluffs (mountain 42 s).
- ~~Quarantined estimators (valley fall, ridge counting)~~ Resolved
  2026-07-28. `dune_wavelength_m` is now quarantined with a falsification
  result rather than a to-do; the provisional 150–400 m table (from dune
  geomorphology) is better than anything measurable here. Revisiting it
  needs dune-only crops or a mapped-dune inventory, not a better spectrum.
- **`meander_intensity` should be replaced by sinuosity — a design change,
  not an estimator fix.** `intensity` is `A / A_max` against the
  generator's own curvature clamp, so real channels (which bend tighter)
  measure at exactly 1.0 and the knob can never be informed by data. The
  planner should take a measured `meander_sinuosity` (arc / trend length,
  which measures cleanly: 1.18 confined → 1.36 wandering) and solve for the
  `MeanderSpec` amplitude achieving it. Same for `meander_wavelength_mult`:
  fit an absolute wavelength (or regress on drainage area, as terrain-v2's
  `landform_prior` does) rather than a ratio to a fragile top-width.
- **The core-cap budget solve flattens the whole 3 km box, not just the
  core** — the dominant cause of the 22/73 validation result. `corecheck`'s
  global fallback scales every feature amplitude AND the tilt, so a florida
  seed whose 1.5 km core must fit a 3.5 m cap ends up with 2.4 m of relief
  across the entire tile against a real median of 13.6 m. The cap is a
  *core* contract; the outer ring is scenery and should keep its fitted
  relief. Fix: make the global pass core-localized (scale through the
  `core_protect` shoulder, as the targeted pass already does) so the
  outer ring is untouched. Re-run `compare` after — most of the relief,
  valley-count and basin-count FLAGs should clear together.
- Sandhills fitted `relief_amp_m` (med 78 m, Nebraska Sandhills exemplars)
  is dramatic vs golf-sandhills (Pinehurst ~15–30 m). If gate pass-rates or
  looks suffer, re-pick exemplar tiles toward mixed-grass dune fields or
  add golf-sandhills course tiles to the corpus.
