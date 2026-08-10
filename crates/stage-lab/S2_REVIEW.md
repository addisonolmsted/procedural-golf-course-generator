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

## MUST PASS — by feature family

### Channels (the network)

1. **Topology.** No channel crosses another, no loops, no parallel twins
   closer than ~40 m (they merge in reality). Tributaries join at acute
   angles opening upstream; confluences are Ys, not Xs or Ts.
2. **Space-filling WITH variance.** No channel-free quarter on an
   integrated biome — but the rhythm must vary: tight dissection in the
   lowlands, broader spacing near divides (real spread is ~50–240 m from
   p25 to p90). Uniform spacing everywhere is as wrong as clustering.
3. **Hierarchy legible.** Light thin fingers feed mid-blue streams feed
   the dark trunk; each order longer and steadier than the one below;
   the trunk runs continuously from the interior out through the blue
   base-level band. Status line: Ω ≥ 3, rb ≈ 3–5, rl ≈ 1.5–3.
4. **Grade sense.** Valleys deepen and widen downstream (floors are
   order-scaled); dead-flat channel reaches appear ONLY where the land
   genuinely dips below the spill level (future ponds — rare on
   integrated biomes).
5. **Planform character.** Gentle wandering, not dead-straight beelines
   (watch long infill fingers — the known residual weakness) and not
   high-frequency scribble.

### Ridges (interfluves and divides)

1. **Anatomy.** Every interfluve is a rounded crest between two valleys,
   with hillslopes falling to a channel on BOTH sides. No knife-edges,
   no plateau-with-trenches, and never a ridge line along a channel
   (the inversion this review process caught and killed).
2. **Divides ride crests.** The orange lines sit on the visible high
   ground, never cross a channel, and close around catchments.
3. **The dual-network read.** Ridge lines should form a connected
   branching system interlocking with the drainage — where a spur
   splits, a divide forks. If you can trace ridges as a network, the
   layout is right; isolated blobs of high ground are not.
4. **Width family.** Compare against the geomorphon ridge masses on a
   real tile of the same biome (tile-lab, ridges layer): generated
   crests should be in the same breadth family. Per-eye for now —
   measured widths arrive at E7.

### Basins (kettles, blowouts, ponded reaches)

1. **Kettle fields (heathland).** Irregularly spaced (clusters AND
   voids — a perfect lattice is wrong), sizes varied ~70–200 m, never
   centred on a channel. Magenta circles mark intent. Smooth bowls are
   correct pre-S3; hummocky rims are dictionary texture.
2. **Derangement (heathland).** Channels sparse and DANGLING — they do
   not reach the base edge (connectivity ≈ 0 in the status line);
   kettle catchments own most of the ground.
3. **Sandhills.** Zero channels, dune trains perpendicular to the wind
   arrow, enclosed interdune lows. The engine ran; it grew nothing —
   that is the design.
4. **Ponded reaches elsewhere.** A flat channel segment crossing a
   terrain dip is legitimate (S4 paints it as water); many of them on a
   piedmont-like biome means the base level or relief draw is off —
   flag the seed.

### Cross-biome (the gallery test)

In the S2 gallery, the six biomes must be tellable apart by STRUCTURE —
river_valley's floodplain + terrace steps, hill_country's benches,
great_plains' low flat-topped interfluves, piedmont's mid-relief
reference, heathland's kettles, sandhills' dunes — while their channel
spacing statistics stay in the same shared band. Identity through
structure and (later) texture; never through spacing.

## KNOWN PROVISIONAL — real, acknowledged, scheduled

- **Channel cross-sections.** Floors are order-scaled constants and the
  bank profile is parametric; walls can read as sharp cuts, especially
  where the 8 m grid facets under bilinear upsampling. Measured
  widths/cross-sections come from the corpus valley-transect fit (E7).
- **Near-parallel streams (~40–60 m).** The separation floor is 40 m;
  long tangential junction approaches and yazoo-style floodplain
  parallels can sit just above it (seed 44). Real pattern, tight
  expression — watch, don't fail.
- **Rare residual crossings.** Post-smoothing displacement is bounded
  below the separation floor, so any remaining crossing is a bug —
  report seed + location.

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
| 2026-08-09 | davisolmsted + agent | 7 findings, 7 fixes | Session on seeds 1006–1012: overlapping embryos → separation rejection; trunk "ridging" → stratigraphy fades within 120 m of channels (alluvium isn't benched); ridge hairpins → divide chaining blocks the 2-cell-thick return path; angular channels/90° kinks → 2× endpoint-preserving Chaikin on all polylines; T-junctions + last-moment swerves → fingers blend the target's downstream tangent within 220 m and join tangentially; sandhills corduroy → sinuous crests (±55 m wave) + second train rotated 7° (crest merges/splits). |
