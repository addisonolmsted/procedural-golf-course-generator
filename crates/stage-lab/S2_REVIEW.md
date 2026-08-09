# S2 review guide — judging the skeleton *before* texture exists

S2 outputs the structural base: the drainage network, the flow-coordinate
fields, and a smooth catena surface hung off them. S3 has not textured it.
As with C1, judging S2 by "does this look like real lidar" produces false
failures — the fine dissection you see on a real hillshade is < 64 m
texture, which is exactly what S3's patch dictionary adds. This guide says
what to judge now and how to compare against real ground honestly.

## Overlay legend (S2 tab, S2 gallery, `out/s2_review/`)

| Color | Meaning |
|---|---|
| **Blue lines, light → dark** | The authored channel network, colored by Strahler order: light thin = order-1 fingers/headwaters, mid = order-2, dark thick = order-3+ / trunk. Thicker = more drainage behind it. |
| **Orange lines** | Derived divides — catchment boundaries extracted from the flow field (never authored). These are the ridge crest lines. |
| **Magenta circles** | Closed-basin embryos — INTENDED pits (kettles, blowouts), recorded so later stages don't "fix" them. |
| **Blue edge band** | The base-level edge: where water leaves the box, at the drawn elevation. |
| **White box** | The 1.5 km core — the region the 600 m play window will be searched in (S5). |

## Where the real calibration data lives

Two viewers show what is measured from the lidar corpus and which parts of
each tile feed it:

- **tile-lab** (`cargo run -p tile-lab --release`): select any kept tile;
  the layer checkboxes show the E5 extraction — **channels** (blue,
  flow-routed accumulation ≥ 6·10⁴ m², the mask behind `dist_to_channel`
  and drainage density), **ridges (geomorphon)** (orange, 500 m-lookup
  ridge classification — QA only, nothing is fitted to it), **fill flats**
  (purple, lakes/flooded ground — excluded from texture harvesting), and
  **developed** (red, OSM-masked). Everything not purple/red on a kept
  tile contributes to residual (texture) statistics; the blue mask is what
  spacing is measured from.
- **Side-by-side sheets** (`out/s2_review/*_v2.png`): real tile with its
  calibration masks next to a generated tile of the same biome.

What is currently calibrated from the corpus: channel spacing
(`d2c` quantiles — generated matches real at p25/p50/p75/p90), drainage
density (shared invariant, ~2.2–2.7 km/km² everywhere), and the residual
band statistics S3 will draw from. What is NOT yet fitted: catena curvature
θ and per-biome relief amplitude (E7 targets); ridge/hillslope FORM beyond
the profile shape (S3's dictionary carries it).

## MUST PASS — judge these

1. **Drainage anatomy.** Every hillslope should fall toward some channel;
   interfluves are rounded crests between valleys, not plateaus with
   trenches. (The catena profile guarantees finite valley-wall slope and
   zero crest slope — if you see slot canyons or knife-edge ridges, that
   is a bug, and was: two were fixed from exactly this review.)
2. **Space-filling without uniformity.** No large channel-free quarters on
   integrated biomes — but spacing should VARY (tight clusters and broad
   interfluves), like the real p25–p90 spread (≈ 50–240 m).
3. **Hierarchy.** Thin light fingers feed thicker darker streams; junction
   angles acute (pointing downstream); no channel crosses another; no
   loops.
4. **Base-level obedience.** The trunk exits through the blue edge band;
   elevations fall toward it.
5. **Discontinuity behavior.** Where C1 drew a scarp: tributaries stop at
   it, drainage runs along its base, only the trunk crosses (water gap).
6. **Biome expression through dials, visible:** sandhills = zero channels,
   dune trains ⊥ wind; heathland = kettle-pocked, channels sparse and
   dangling (deranged); river_valley = broad trunk floodplain + terrace
   steps; hill_country = benched slopes; great_plains = low relief, flat-
   topped interfluves; piedmont = the plain reference case.

## MUST NOT JUDGE — S3's job, expected "failures"

- **Fine dissection / spur-and-hollow texture.** The real hillshade's
  rich fine grain is < 64 m residual; its absence here is required.
- **Kettle/basin irregularity.** Embryo pits are smooth Gaussians now;
  hummocky kettle character is dictionary texture.
- **Catchment-boundary staircases.** D8 flow on a smooth surface follows
  45°/90° lines, so derived divides stair-step and basins can read
  square-ish. Real-texture flow (post-S3) irregularizes them; the divide
  overlay is drawn smoothed but the underlying labels stay honest.
- **Surface realism at a glance.** Candle-wax is correct.

## How to run a comparison session

1. Open a real kept tile in tile-lab (channels + ridges layers on).
2. Open the same biome in stage-lab's S2 tab (or the `_v2.png` sheet).
3. Compare: spacing rhythm, junction style, valley-width-to-spacing ratio,
   how crests round off. Ignore texture.
4. Log findings in the session table below; a specific recurring artifact
   is a bug report — say which biome/seed.

## Session log

| Date | Reviewer | Verdict | Notes |
|------|----------|---------|-------|
| 2026-08-09 | davisolmsted + agent | 3 fixes | "square-ish basins, ridges much too thin" → catena profile rebuilt (finite wall slope, full-hillslope incision, D_FULL 260 m), incision blur 72 m, divides drawn smoothed. Trench anatomy resolved; fine texture correctly deferred to S3. |
