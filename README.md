# Procedural Golf Course Generator

Seed-driven terrain generator for golf courses. Deterministic in `(seed, params)` —
bit-identical across hardware, so results can be cached.

**Current phase: noise + erosion calibrated against real courses.** One `u64` seed
drives the whole terrain: parameters are drawn from a **calibrated `p(θ)`** — a
density fit (in `tools/calibration`) so the generator's *terrain-metric distribution*
matches the 200-course *Parkland Atlas*, scored by the same metric code
(`tools/metrics`) — the field is sculpted by a deterministic geomorphic erosion pass,
and a seed-search finds the seeds that reproduce each real course most closely.

## Workspace

| Crate | Role |
|-------|------|
| `golf-core` | Foundation: deterministic RNG (`det`), `Grid`/`GridSpec`, `Vec2` + libm math |
| `golf-terrain` | Hash-based Perlin/fBm noise + domain warp + redistribution, **erosion pipeline**, seed→params sampler, slope, stats |
| `golf-atlas` | The surveyed reference courses (elevation + water + **tree canopy** + holes): HTML → packed `assets/atlas.bin` (`GATLAS2`), comparison windows. Grown to 200 parkland courses by `tools/parkland_atlas/` |
| `golf-match` | Seed search: mean-removed RMSE at absolute scale, best window placement, two rayon stages |
| `golf-routing` | **9-hole par-36 routing**: clubhouse/tee/green siting, lines of play, walks, annealing |
| `golf-holes` | **Hole build-out**: fairway boundaries + contouring, greens with enforced pinnability, pins, tee pads |
| `golf-viz` | Terrain → PNG: hypsometric + hillshade height view, slope heatmap, flow/lakes view |
| `golf-viewer` | **wgpu + egui studio**: Explore (seed browser), Match (seed search vs atlas), Hole viewer, Parkland Atlas (survey browser: terrain/tree/water/holes toggles) |
| `xtask` | Headless tools: atlas packing/stats, calibration, erosion sweep/bench, seed search, dumps, goldens |

## Erosion

`generate_course(spec, seed)` runs the full pipeline: noise on a fixed **160²
canonical macro grid** (12.5 m cells) → erosion → composed output at any
requested resolution (`bilinear(macro) + slope-gated 32 m detail band`, so any
window of a seed's world is bit-exact at any resolution). Erosion is pure grid
arithmetic — no RNG, fixed traversal orders — structured as 8 routing epochs:

- **Priority-flood depression routing** (Barnes ε-variant, FIFO-optimized;
  D8 receivers, Braun–Willett topological order) — recomputed every other
  epoch; every cell provably drains to the world edge.
- **Implicit stream-power incision** (m=½ → `sqrt`, n=1 → closed form,
  unconditionally stable), F-capped so the update never saturates — carves
  dendritic valley networks without over-deepening.
- **Graded-target deposition**: a forward pass computes the alluvial profile
  (receiver + 0.6% grade, propagated whole-profile in one sweep), then the
  supply-limited reverse pass fills toward it — this is what builds the flat
  valley floors the atlas demands. Exact mass ledger: eroded = deposited +
  exported (unit-tested).
- **Talus guard** (35°) and **hillslope diffusion** (Jacobi, stable substeps).
- Byproducts kept for the future water system: **flow accumulation, lake
  depth, sediment maps** (see the viewer's Flow view).

A hardness field (strata-banded differential erodibility) is implemented and
manually drivable, but failed its sweep inclusion gate (Δ band objective ≈
noise) and stays out of the sampler for now.

## Water

**No global water level.** After erosion, `water::build` derives a natural
water layer from the canonical flow tree (exported by erosion as a
`RoutingSnapshot` — zero extra routing cost) and carves it into the terrain:

- **Streams, two tiers.** The fine drainage network (the *swale tier*) is
  always traced as routing metadata and the HAND wetland reference — but
  **visible water requires 10× that drainage area**, so only collected mains
  carry streams (a handful per course, starting at creek width, stubs under
  150 m culled) instead of dozens of thin threads. Widths follow hydraulic
  geometry `∝ √(runoff·A)` (brook < 4 m / creek 4–8 m / river > 8 m);
  centerlines are resampled at ~10 m, smoothed, and **meandered with
  width-scaled amplitude and wavelength (λ ≈ 14·w)**, attenuated on steep
  ground. Water surfaces are provably non-increasing downstream; the carve
  follows the meandered line and the **corridor claims every cell within the
  half-width** (a wide river is a ribbon in the data, not a centerline).
  **River courses** (~17% of wet seeds) get an external upstream catchment
  (8–80 km², → 14–45 m rivers); **~1% get a major river** (100–350 km², →
  50–90 m, a full crossing that splits the property — the Banff pattern).
  **Lone small streams** (touching no other stream or body, < 6 m wide) must
  end with pond proportions: they're **widened to length : width ≈ 4–5 : 1**
  (uniform width, tapered ends, flat surface, classed as pond) or removed
  when a sane pool is infeasible (drop > 2.5 m, or needing > 80 m of width) —
  an isolated thread reads as an accident. Meander runs hot on near-flat
  ground (two noise octaves, ×1.5 flat boost) precisely because deposition
  floors are where D8 would otherwise run dead straight.
  For routing, brooks/creeks are carry hazards, not barriers; only
  ponds/lakes/wetlands and river-class channels block
  (`WaterLayer::is_blocking`), and river courses accept a bridged split.
- **Ponds/lakes** from depression basins filled toward their spill, greedily
  selected into the coverage budget (partial-staged at the margin).
- **Terrain water is LARGE features only** — rivers, streams (≥ 10 m),
  lakes, and large ponds (≥ ~4,000 m², rising to ~0.8 ha on mountain).
  Small strategic ponds are *not* generated here: they come later, in the
  hole layout phase, dug at natural low points where the strategy wants
  them. Terrain wetlands are off (the HAND machinery stays, budget 0) and
  shallow basins (< 0.4 m) are skipped rather than kept as marsh.
- **Lowland = hole-scale lake fields (~20% ± 10 coverage).** The
  Sawgrass/Doral pattern comes from the standard noise pipeline: as relief
  falls, the sampler shrinks the feature wavelength (`× (1 − 0.5·k)` — the
  measured lever: floodable basin capacity is ~20%+ at P 500–900 vs 3–9% at
  P ≥ 1400, `xtask lake-capacity`) and damps erosion's graded-target
  deposition (`× (1 − 0.6·k)`), which otherwise fills exactly the basins
  the lakes flood. Coverage draws ~22% median, tightened to the 12–27% band
  the terrain can realize; deep-lowland courses realize 13–22%. Skew
  forcing was A/B-tested both directions and moved capacity ≤ 1 pt — the
  drawn skew is left alone.
- **Coverage solve is exact** (streams counted analytically, ponds greedy —
  no iteration), followed by monotone playability guards: max single body
  5.5% of the world (14% for lake-tail courses), ≤ 3% stream share, and
  **the largest connected dry region must stay ≥ 70% of the world** (60%
  on lake-field courses ≥ 12% coverage — the ground between the lakes IS
  the course; routing verifies real playability).
- **Pocket lakes, not mega-lakes.** Basins flood cell-by-cell in z order
  through a union-find; a connected pocket that reaches the per-body cap
  **freezes** at its current stage and later cells that would touch it stay
  dry — so an oversized basin yields several golf-hole-scale lakes
  separated by land at its internal saddles instead of one truncated
  mega-lake.
- **Organic outlines + golf-hole proportions.** Each body's raw basin
  boundary is regularized (`regularize_bodies`): a per-body signed chamfer
  field is binomially smoothed and re-thresholded (outline movement bounded
  to ±1.6 cells), cells the rounding **adds are dug** to `surface − 0.4 m`
  (cut capped at 1.2 m — the water stays justified by the terrain), and
  multi-pocket bodies split into separate lakes. Every body must pass the
  size/proportion spec — length ≥ ~180 m (target 400 ± 200), aspect ≤ 8.5
  (1:1 up to ~1:8), and no elongated-narrow snakes (aspect > 3.2 with width
  < 22 m). Realized: length p10/50/90 ≈ 200/340/710 m, ratio p90 ≤ 3.
- **Anchored stream endpoints, strictly.** Every visible stream must begin
  AND end at the map edge, standing water, or a junction with another
  channel. Heads that would start mid-slope snap to the nearest edge/body
  within ~150 m via a prepended feeder channel (meandered like everything
  else) — otherwise the **whole link is removed**; no source ponds are ever
  dug. Dangling tails extend down the flow tree or the link is removed;
  removals cascade to a fixpoint so nothing is left anchored to a deleted
  channel. Gated: `water-report` counts **0 unanchored endpoints and 0
  skinny bodies over 300 seeds**.
- Sampled per seed (own stream): dry-course probability rises with relief
  (lowland ~0% dry — it's the lake-field band; mountain ~20%); a 14%
  mountain lake-course tail (Jasper/Banff pattern). Realized coverage
  ~14/3.5/1.9% by band (lowland deliberately ABOVE its atlas 8.5 —
  the game wants lake-field courses; rolling/mountain stay sparse).
- Entities (links/bodies) are the source of truth and the grids are rebuilt
  from them — the hole-routing phase will edit entities (dug ponds, stage
  changes, crossings) and re-derive deterministically.
- **Smooth water outlines.** Standing water and wetlands each get a
  **smoothed signed-distance field** on the macro grid
  (`standing_sdf`/`wetland_sdf`): chamfer in/out → two binomial passes →
  the zero isoline is an organically rounded implicit outline, sampled by
  renderers at any zoom. A ±half-cell **protection band** re-pins every
  decisively wet/dry cell, so islands, dry isthmuses, and one-cell arms can
  never be smoothed away. Diagonal saddles (a wet diagonal pair reads
  locally like a dry isthmus) are resolved toward water connectivity with
  thin **bridge capsules** (~3 m) between the cell centers — chains render
  as organic bands with thin necks. **Streams never render from a raster at
  all**: brooks are 2.5–4 m wide — at or below any practical cell size, so
  rasters pinch them — and the entities carry smooth vector centerlines, so
  close-up renderers fill per-segment capsules from the vectors directly.
  A `fine_class` raster (800², 2.5 m) derived from the same outlines serves
  coarse consumers. Gameplay/routing rules stay on the macro grid; heights
  are untouched, so all goldens are unaffected.

## Hole routing

`golf_routing::route(&CourseTerrain, seed)` lays a **9-hole, par-36 returning
nine** over the finished terrain — read-only over the terrain (terrain goldens,
calibration, and the seed search are untouched) and deterministic on its own
`b"routing-v1"` stream. ~50 ms median on top of the ~28 ms terrain build.

- **Playable box.** Everything — tees, greens, lines of play, walks — stays in
  the central 1.5 × 1.5 km of the 2 km world (500 m visualization buffer).
- **Site fields.** Green / tee / clubhouse candidates are scored on the macro
  grid (pad flatness, water-edge interest bands, prominence for perched greens
  and elevated tees) and Poisson-thinned into deterministic candidate lists.
  The clubhouse draws from the best "semi-interest" sites (flat + near water
  or overlook).
- **Par sequence.** Par 36 forces n₃ = n₅, so the mix draws from a weighted
  composition table — (2,5,2) ≈ 57%, (1,7,1) ≈ 24%, and the 3-3-3 mix held at
  ~15% (a deliberate variety boost over the atlas's 0%) — then orders by
  shuffle-with-rejection so rare patterns stay rare (par-3 opener ~3%,
  back-to-back par 3s ~3%, back-to-back par 5s ~8%). Length targets are
  jittered Latin-hypercube quantiles through per-par atlas curves, so every
  course mixes short/mid/long holes (balance gate: 97% of courses).
- **Lines of play.** Par 3 = one segment, par 4 = two, par 5 = three; dogleg
  vertices are solved on the tee–green ellipse so segment lengths sum to the
  target *exactly* (turns capped at 55°, atlas-shaped). Hard rules at every
  placement: **≥ 45 m buffer between any two lines of play** (which subsumes
  no-crossings), no blocking water (wetland crossings only under 120 m,
  brooks/creeks are carry hazards — crossing one 30–200 m off the tee scores
  a bonus), corridor-grade backstop, clubhouse clearance.
- **Returning nine.** A loop schedule (bearing vs cumulative length around a
  drawn center) guides greens around a circuit that starts and ends at the
  clubhouse; hole 9's green is hard-anchored home. Steep worlds draw compact
  loops and fall back to a linear out-and-back axis — the mountain-valley
  pattern. Greedy construction with backtracking, then a relaxation ladder
  (six stages that also *move the clubhouse away* from unroutable
  neighborhoods before loosening much).
- **Walks.** Green→next-tee paths run A* on the macro grid around all play
  corridors (inflated 15 m, endpoint-exempt) and blocking water; the **curved
  length** (capped at 620 m) is what the optimizer scores, and a strict final
  check proves no walk crosses a line of play.
- **Elevation along the line of play.** Fit to what the atlas holes actually
  do (measured by sampling each survey heightmap along its real hole
  polylines): holes play near-level (Δz tee→green median ≈ 0) with bounded
  tails (atlas p10/p90 par 4: −13/+10 m), climbs are rare (uphill >10 m on
  9.7% of real holes, >20 m on 1.5%), and tees sit slightly above the 137 m
  drive landing point (median +1 m, p90 +7 m — the elevated-tee pattern).
  Routing enforces hard per-hole climb/drop caps (±22/38 m base, ladder-
  relaxed), steers green choices away from climbs, and rewards tees that
  overlook the drive zone. **High ground carries a premium** for both tee
  and green sites; **par 3s earn a bonus for connecting adjacent high
  ground with a carry dip between** (the classic gorge-carry one-shotter);
  **blind opening shots cost** (line-of-sight from tee eye height to the
  drive landing point); and **sustained grade beyond the opening 150 m is
  soft-capped at ~7%** — aggressive ground is reserved for the tee shot,
  preferably downhill. Generated: Δz p50 ≈ 0 per par, uphill >10 m 6.2%.
- **Annealing.** ~1600 fixed-iteration simulated-annealing moves (re-pick
  green/tee, re-draw doglegs, shorten the longest walk) polish site quality,
  length adherence, walk lengths, fairway grade, and the elevation terms —
  hard constraints stay enforced by move rejection, so every accepted state
  is valid.
- **Gates** (`xtask routing-report 500`, 20 gates): 95.4% of seeds routable,
  zero hard-constraint violations, per-par lengths inside the atlas p10–p90
  bands (par 4 p50: 395 m vs atlas 394), hole Δz p50 ≈ 0 with capped climb
  tails, nine totals ~3.3 km, walk p50 64 m / p90 115 m (atlas 67/126),
  route time p50 47 ms / p95 88 ms. Unroutable seeds fail deterministically
  (the game re-rolls).
- **Analysis chart**: `xtask routing-analysis 400` writes
  `output/routing_analysis.svg` — generated-vs-atlas box-whisker comparisons
  (par mix + sequence rarities, hole lengths per par, hole elevation
  changes, drive drops, nine totals, walks), stamped with the sampler /
  erosion / water / routing versions and the atlas fingerprint it was
  measured against.

## Hole build-out

`golf_holes::build(&CourseTerrain, &Routing, seed) -> CourseBuild` turns the
routed lines into built holes (~50 ms, read-only over terrain + routing, own
`b"holes-v1"` stream). The playing surface is
`CourseBuild::surface_at(&ct, p)`: the base terrain everywhere, locally
replaced by fine **absolute-surface patches** (fairways at 2 m, tees/greens at
0.5 m) blended under feathered weights — bit-exact at any query resolution and
bit-identical to `height_at` outside patch bounds. `zone_at(p)` classifies
green/fringe/tee/fairway/rough for rendering and later phases.

- **Fairway boundaries** follow the FairwayStudio spline-shape model (from
  the manual CourseGenerator editor): a **Catmull-Rom spine** through the
  routed points (doglegs are smooth curves, not corners), per-side widths
  from **smoothstep-eased landmarks** (C¹ at every knot) encoding strategy —
  widest at the drive landing zone, narrowed at the deep zone and approach,
  inside of doglegs 20–30% tighter — overlaid with **low-frequency
  sinusoids** (organic edges) and a **meander offset** that bends the
  corridor, strongest on straight holes. A **curvature clamp** (tanh
  soft-saturation of the inside width toward `0.85/|κ|`) makes kinks and
  self-intersection structurally impossible, a kink guard softens overlays
  if water clips still corner, and **semicircular end caps** taper the width
  on a quarter-ellipse — no hard stops; the approach cap tucks under the
  green fringe. Fairways start 45–90 m from the tee. **Par 3s carry no
  run-up ribbon**: their only fairway cut is ONE **surround petal** — a
  slightly larger copy of the green's own petal shape (same rotation and
  harmonics) whose center is shifted a small strategic distance (downhill
  where runoff collects missed greens, or toward the tee on flat sites),
  making the mown collar generous on the runoff side and tight on the far
  side; scaled down until clear of water. Landing zones recorded for the
  bunkering phase.
- **Fairway contouring** (subtle-realistic): smooth macro base (mowing drops
  the detail-noise texture), drainage crown 0.15–0.35 m where the cross-cant
  doesn't already shed, roughness-scaled undulation, extreme cant softened
  toward ≤8%, and a rare (~4% of courses) off-center mound cluster. An
  ε-priority-flood **drainage guard** proves no interior pit remains.
- **Greens are petal shapes** — `r(θ) = a0 + Σ aᵢ(1+cos(iθ+φᵢ))` with
  centered harmonics i = 2..4 (guarded greens sometimes add the strong first
  harmonic: a kidney bulging away from the hazard) — sized by approach
  length (long approach → receptive green; realized r = 0.33) and oriented
  strategically (open / angled / guarded). Contouring: an approach-tilted
  base plane, 0–2 tiers, gentle swales/bumps, micro noise — then a fixed
  relaxation loop **guarantees ≥18% of the green is pinnable** (≤3.3% slope
  over a 1.5 m disc, ≥2.7 m inside the boundary, off risers).
- **Pin**: one per course (procedurally generated, played once), chosen from
  the pinnable set by guarded-side proximity, drawn tier preference, and an
  approach-length depth prior; re-verified on the composed surface.
- **Tee pads**: flat **elliptical** platforms aligned with the opening shot,
  lifted 0.3–1.5 m (less where the ground already falls away) — this closes
  the atlas drive-drop gap: pad-to-landing median is now **+0.7 m** (atlas
  +1.1, raw landform 0.0).
- **Gates** (`xtask build-report 300`, 13 gates, all passing): every pin
  inside its green with margin and ≤4% composed-surface slope, pinnable share
  ≥18% on every green, zero fairway pits, surface bit-equal to base outside
  patches, green-area/approach correlation, landing-zone widest on 97% of
  par 4/5, inside-dogleg narrower on 86% of turns, deterministic builds,
  build p50 ≤ 100 ms. `xtask dump-built` writes course maps + a 3×3 grid of
  green close-ups (0.1 m contours + pin) per seed.
- **Water refinement** (the terrain *suggests* water; the build decides near
  play): standing water/wetland regions whose connected component intersects
  a hole's bounding box get (a) trimmed where they bulge more than ~4 m into
  a fairway — unless they're a genuine **cross hazard** reaching beyond both
  corridor edges, which also keeps its strict outline protection (a thin
  strip across a fairway is a forced carry, not a wacky lobe) — and (b)
  extra outline smoothing with a relaxed protection band, so thin wacky
  lobes melt into rounder banks. Fairways are realized **twice**: once
  against the suggested water to detect intrusions, then again against the
  refined water — so when a hazard is trimmed out of a fairway, the fairway
  takes its ground back. Regions away from every hole keep the terrain's
  outlines bit-for-bit. Purely aesthetic/course-level: heights, entities,
  and routing semantics are untouched.
- **Bunkering**: strategic sand from the two long-form primitives — petal
  blobs (harmonic edges) and capped **spline trenches**. Per hole:
  greenside guards on the flanks/front/back (sometimes clustered into an
  overlapping **complex**, sometimes a tiny round **pot**), landing-zone
  pinchers just off the fairway edge, long spline liners along the
  **outside of doglegs** catching weakly-shaped shots, occasional
  decorative sand off-line, and short-of-green carry teases on par 3s.
  Each candidate validates ~20 outline samples against water, green +
  fringe, tee pads, and already-placed bunkers — a few dozen draws per
  hole, effectively free. Median 27 bunkers per course (real nines run
  ~20–30); gates: none ever touch a green or pin.
- Waste areas and trees are the next phases — their inputs
  (`fairway_outline`, landing zones, `zone_at`) are recorded.

## The seed sampler

`golf_terrain::sample_params(seed)` draws a full parameter set by **resampling the
calibrated `p(θ)` ensemble** (`sampler/ptheta.rs`, generated by
`python3 -m calibration.export_sampler`): one `DetRng` stream picks a design point
by its `p(θ)` weight, adds small unit-space jitter for continuity, and decodes the
**19 knobs** to real units — the same `unit → θ` map the calibration used. Nothing
is solved analytically anymore (v8 fit tri-modal relief bands and *solved* amplitude
from a target relief; v9+ draws every morphology knob straight from `p(θ)`):

- **Base fBm + warp + redistribution** — amplitude, base period, octaves,
  lacunarity, gain, warp amplitude/period, and the redistribution exponent
  (`u^p` basins for positive skew, mirrored `1−(1−u)^(1/p)` plateaus for negative).
- **A fine-detail band** — `detail_amp/gain/octaves` add sub-25 m, query-time
  texture on top of the eroded macro grid (the primary roughness lever: it lowers
  the spectral slope β and raises high-frequency energy / curvature toward the
  atlas), plus a small un-gated `nugget_amp` floor for cell-scale texture on flats.
- **Directional anisotropy** — `aniso_ratio` stretches the noise/warp/detail
  sample coordinates along a per-seed `aniso_angle`, giving the ridge-and-valley
  grain real courses have (the `anisotropy_ratio` metric was the isotropy gap).
- **Landform organization (v10)** — `ridged_mix` blends creases along the fBm
  zero-contours (connected ridge/valley *networks* — the coherent topology
  erosion can deepen but never invent), and `relief_patchiness` applies a
  mean-preserving ~1.1 km amplitude modulation (within-map regime variety: a
  rugged corner, a flat meadow). Fit against five landform-*organization*
  metrics (roughness concentration `rough_top10_30`, clustering `rough_moran_60`,
  mid-scale granularity `extrema_per_ha`, valley shape `valley_elong_120`) added
  to `tools/metrics` after the v9 terrain read as uniformly choppy.
- **Erosion axes** — intensity, deposition, diffusion, and a **reactivated
  `hardness_contrast`** (v8 fixed it at 0; `p(θ)` draws it > 0 for differential
  erodibility → benches/cliffs), all from `p(θ)`.

A forward relief estimate (`amplitude · k_relief · k_redist · k_erode`, via the
`xtask calibrate` tables) feeds the out-of-scope **water** sampler and the
descriptive `relief_target`; the **lowland lake-field period-shrink** is preserved
as a water-serving adjustment on the drawn wavelength. At export time the ensemble
weights get a tempered **relief-axis reweight** (strength 0.5) so the sampled
relief distribution lands near the atlas (median 16.9 vs 19.8 real; ≥65 m share
6% vs 3.5%) — full-strength matching costs too much on the other 40 axes.

The calibration loop closed twice: v9 took real-in-hull **0.59 → 0.985** and
energy-to-real **4.21 → 1.42**; the v10 landform pass (5 new organization axes,
41 matched metrics, hull **1.0**) took roughness-concentration reach 0.84 → 0.96
and kurtosis reach 0.86 → 0.92 while holding aggregate energy, and visibly fixed
the uniform-stipple / fragmented-valley / same-everywhere reads. Validate with
`cargo run -p xtask --release -- sampler-check 600` (bands vs the live 200-course
atlas baseline) and re-run `python3 -m calibration.run` for coverage/round-trip.

## Try it

```
cargo run --release -p golf-viewer                 # the studio (Explore / Match / Hole / Parkland Atlas tabs)

# one-time setup for the Match + Parkland Atlas tabs / search:
cargo run --release -p xtask -- atlas-pack ~/Downloads/parkland_atlas.html   # or tools/parkland_atlas/out/parkland_atlas.html (200 courses)

# refresh/extend the survey itself (Python, networked — see tools/parkland_atlas/README.md):
#   python3 tools/parkland_atlas/seed_reference.py && python3 tools/parkland_atlas/collect.py

cargo run --release -p xtask -- search 100000      # headless seed search -> output/seed_search.json
cargo run --release -p xtask -- atlas-stats        # reference stats per course
cargo run --release -p xtask -- calibrate          # regenerate sampler tables (after noise changes)
cargo run --release -p xtask -- sampler-check 600  # generated vs atlas targets
cargo run --release -p xtask -- slope-report 100000  # slope-grade aggregate vs atlas
cargo run --release -p xtask -- erosion-sweep 30   # grid-tune erosion params vs atlas bands
cargo run --release -p xtask -- erosion-bench 200  # per-seed pipeline timings (incl. water)
cargo run --release -p xtask -- atlas-water        # real-course water-mask statistics
cargo run --release -p xtask -- water-report 600   # generated water vs atlas gates
cargo run --release -p xtask -- water-on-atlas     # our water build ON the real course terrain (coverage + IoU)
cargo run --release -p xtask -- atlas-routing      # real-course hole/routing statistics (incl. elevation)
cargo run --release -p xtask -- routing-report 500 # routed nines vs atlas gates
cargo run --release -p xtask -- routing-analysis 400 # gen-vs-atlas chart -> output/routing_analysis.svg
cargo run --release -p xtask -- dump-routed <dir> <seeds...>  # terrain+water+routing PNGs
cargo run --release -p xtask -- build-report 300   # hole build-out gates (pins, widths, drainage)
cargo run --release -p xtask -- dump-built <dir> <seeds...>   # built-course PNGs + green close-ups
cargo run --release -p xtask -- dump-hole <dir> <seed> [holes...]  # rotated 0.5 m/px per-hole views
cargo run --release -p xtask -- dump-sampled <dir> <seeds...>    # height+slope+flow PNGs
cargo run --release -p xtask -- match-dump <dir> [course] [rank] # side-by-side match PNGs
cargo run --release -p xtask -- golden             # determinism gates (noise + pipeline)
cargo test --workspace
```

## Viewer

**Explore** — deliverable #1. Randomize/step a seed and view its terrain over the
fixed 2 km × 2 km world (height, slope, or **flow/lakes** view; detail or 4×4 seed
grid). Shows total relief, high/low markers with values, lake fraction, and the
slope-grade histogram with the ≤12% playable fraction. Parameters are seed-sampled
by default; a Manual mode keeps raw noise sliders plus an optional erosion section
("Copy → manual" to hand-tune from any draw). A **Routing toggle** overlays the
routed nine on the detail view — colored lines of play with hole numbers, tees,
greens, dashed walking paths, and the clubhouse — with the par sequence and
total/walk lengths in the sidebar, plus a **course elevation profile** panel:
terrain height along the full walked course in play order (hole 1 tee → hole 9
green → clubhouse), colored per hole with walks in gray, hole/par labels, and
the total climb/descent readout.

**Hole viewer** — one built hole at a time (pick 1–9; seed synced from
Explore): the hole's bounding box rotated so the **opening shot points up**,
drawn **deliberately flat/2D at 0.1 m per layout pixel** — fairways filled by
scanline from their actual boundary polygons, greens/surrounds/tees from
their analytic petal/ellipse tests, standing water from the course-refined
smooth SDF outlines, streams from their vector centerlines. No shading; the
only relief cue is a **togglable 1 m contour set** (from a 0.5 m height
lattice when enabled). Overlays: line of play, tee-pad ellipse, green
boundary, pin flag, north arrow, 50 m scale bar; the sidebar shows
par/length, green area + pinnable share, tee-pad lift, fairway start, and
the walk to the next tee. Headless twin:
`xtask dump-hole <dir> <seed> [holes…]`.

**Match** — deliverable #2. Runs the seed search (default 100,000 seeds) on a
background thread with progress + cancel, caching results to
`output/seed_search.json` (auto-revalidated via a content fingerprint). Pick any
of the 27 courses to see its survey heightmap next to the best-matching seed's
terrain under a shared elevation color scale, with RMSE, window placement, datum
shift, and the top-10 seed list. A **Water toggle** overlays the course's real
survey water mask and the seed's generated water side by side (window water
percentages in the captions).

### Matching definition

Score = RMSE in meters between the course's comparison window (survey centered,
cropped to ≤2 km per axis) and a same-size window of the seed's world, after
removing only the best-fit vertical offset — **absolute scale**, no amplitude
fitting. The window slides inside the 2 km world to find the closest placement.
Both stages score the full eroded pipeline (stage 2's 12.5 m/px raster **is** the
canonical macro grid, sampled bit-exactly). Stage 1 scans every seed at
31.25 m/px; stage 2 re-scores each course's top 250 with dense placements.
100k seeds ≈ 4–5 min on an M-series laptop (~26 ms per seed, erosion included);
generating one course for a player is imperceptible.

## Determinism

Noise uses only integer hashing + f64 add/mul/floor — no transcendentals — and
erosion is pure grid arithmetic with fixed traversal orders and tie-breaks, so a
`(seed, params)` pair produces an identical field on any platform. The sampler
draws through `DetRng` (ChaCha8 + blake3 domain split) with Irwin–Hall normals.
`xtask golden` pins four reference hashes (raw noise, full pipeline, routed
nine, built holes) as the cross-hardware gate. Routing and build-out draw on
their own DetRng streams with fixed iteration counts and no map-iteration
order anywhere, so the routed and built course is bit-identical per seed too.

## Next increments

1. **Hazards**: bunkering (landing zones + green surrounds are recorded as
   hooks), waste areas, and constructed water Phase B — dug ponds (the
   Florida pattern — most of Doral's real 16.6% is artificial), island
   greens, bridge/culvert points — deterministic edits of the water entities
   + the same grid rebuild. Could also rescue some of the ~5% unroutable
   seeds.
2. **Trees**: canopy coverage following the atlas tree masks — lining holes,
   very rarely (2–5% of courses) a single in-fairway specimen.
3. **Erosion/water v2 polish**: mountain steep-band residual (hardness-
   modulated talus), deposit-weighted floodplain smoothing, sub-cell shoreline
   refinement at game resolution.
