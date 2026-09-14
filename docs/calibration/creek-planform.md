# Creek planform: the measurements, and why the model was reverted

*2026-08-30 / 09-01. Code reverted; measurements stand. Instruments:
`tools/aeolian/creek_{corpus,straight,skew,crossings}.py`.*

## What prompted it

Review, twice: the creeks "look too sinusoidal with regular meandering that
reads artificial", and after a first fix, "still artificial ... just due to
the sinusoidal nature".

Both creek generators (`gorge.rs` aeolian trunk, `water.rs` fluvial valley
creek) offset a smooth base line by

`off(s) = A · [ sin(phi) + 0.35 sin(phi/2.7) ]`

with **A constant along the whole reach**. Two sines at fixed amplitude make
every bend the same size and the same shape.

The stated reason behind the review was *routing*, and it arrived late: lines
of play were being placed where the creek crosses the fairway **and comes
back**, which is atypical golf. Realism was the proxy; **amplitude was the
requirement**.

## The real-creek measurements (these are the durable output)

All from OSM `waterway=stream` over the corpus search boxes, projected to UTM,
**resampled at 10 m and smoothed at 30 m**.

| statistic | p25 | p50 | p75 | n |
|---|---|---|---|---|
| bend length | 40 m | 50 m | 60 m | 55,281 reaches |
| bend-length CV (irregularity) | 0.38 | 0.46 | 0.55 | " |
| sinuosity | 1.063 | 1.117 | 1.225 | " |
| bend skew (apex position, 0.5 = symmetric) | 0.399 | **0.601** | 0.714 | 1.58 M bends |
| length straighter than R = 150 m | — | **59.7%** | — | 115,154 reaches |
| typical longest straight run (R > 150 m) | — | **118 m** | — | " |
| length straighter than R = 300 m | — | 35.7% | — | " |

**Real bends are strongly asymmetric** (43% of bends have their apex outside
the middle 40%), and **real creeks are straight most of the time** — 60% of
their length, in runs of ~120 m.

### Two measurement traps, both of which produced confident wrong numbers

1. **Digitizing noise.** A 12 m smoothing filter reported real bend lengths of
   24/24/30 m — physically impossible for a creek. OSM stream vertices sit
   10–30 m apart; the filter was measuring the tracing, not the stream. 30 m
   smoothing over a 10 m resample is the honest floor.
2. **Parallel chords.** "How often does a hole cross the creek twice?"
   sampled at random bearings reads 21.9%. A chord nearly parallel to a creek
   re-crosses whatever its shape, and no router would place a hole that way.
   Restricted to chords ≥ 35° to the local creek axis: **2.8%**. The
   unrestricted figure measures the sampler.

A third trap was self-inflicted: comparing the model's **raw** polyline
against the **smoothed** corpus made the model look far more irregular than
it was (CV 0.66 vs a true 0.15–0.20). Any comparison must run the identical
pipeline on both sides.

## What was built, and what it achieved

An Ikeda–Parker–Sawai / Howard–Knutson **migration model** (`migrate.rs`,
reverted): bend growth from an exponentially-weighted upstream-curvature
integral, Hickin–Nanson damping, a paired diffusive term, neck cutoffs, and
spatially-gated bank erodibility. Fixed-iteration, zero draws in the loop.

Measured against the sine offset, same seeds:

| | sine offset | migration model | real |
|---|---|---|---|
| lateral amplitude p95 | 63.0 m | **15.6 m** | — |
| holes crossing the creek twice | 11.8% | **4.0%** | — |
| length straight (R > 150 m) | 53.6% | 58.1% | 59.7% |
| bend-length CV | 0.16 | 0.27 | 0.46 |

**The routing objective was met** — a 4× reduction in amplitude and in
cross-and-return holes — and that win came from the planform itself, not a
dial: bends at 8–11 channel widths cannot wander the way a 283–700 m sine on
a 6 m channel could. That sine was a geometric impossibility (a big river's
wavelength drawn on a creek), and this is what it cost in routing.

## Why it was reverted anyway

The terrain interaction was worse, and the cause was a carve bug the
experiment *exposed* rather than caused:

**`water::fluvial` graded a soft skirt out to ~32 m either side of the creek
line.** At the old 283–700 m wavelength that read as a broad valley swale and
had passed review for months. Once the planform meandered at 8–11 channel
widths, the same skirt swept a trough following every bend — **the creek was
sculpting its valley instead of sitting in it**. The aeolian side
(`water::cut_creek`) has always cut the correct profile: flat bottom to the
wet width, steep bank, nothing beyond.

Narrowing the carve to ~5 m and scaling migration by local valley room was
implemented and measured, but the round was reverted before that could be
judged settled.

## The unresolved tension, stated plainly

Bend-length irregularity is the one statistic the model could not reach at a
usable amplitude: **0.27 against a real 0.46**. Closing it needs neck
cutoffs; cutoffs need sinuosity above ~1.9 (measured: they do not fire below
it); and sinuosity 1.9 is exactly the amplitude the fairway-crossing problem
lives at. **Two targets in genuine conflict, not a tuning gap** — and for a
golf generator the routing constraint wins.

Also left open: at high growth the model's bend skew **mirror-images** the
real lean (0.37 vs a real 0.60 about 0.5). It passes sinuosity, bend length
*and* irregularity while looking wrong, which is why the reviewer caught it
by eye before any statistic did. Shipped vigour sat below where the inversion
appeared.

## If this is picked up again

1. **Fix the fluvial carve first, on its own.** It is a real bug independent
   of any planform work — the creek must not cut a 32 m swale — and fixing it
   under the *current* sine planform is a small, separately-judgeable change.
2. Only then reconsider the planform, with amplitude as the acceptance
   criterion and irregularity as a secondary.
3. Re-derive, do not trust: every number above comes from the four scripts in
   `tools/aeolian/`, and two of the three traps above were caught only by
   re-measuring.

## Round 2 (2026-09-01): the bend train, and the carve fixed

*Branch `sandhills`. Code: `crates/course-sandhills/src/planform.rs`; sheets:
`examples/planform_sheet.rs` + `tools/aeolian/creek_sheet.py` (step 1, no
terrain) and `tools/aeolian/creek_terrain_sheet.py` (step 2, on the ground).
Instruments lifted verbatim into `tools/aeolian/creek_measure.py`; real
reaches for the side-by-side committed in `tools/aeolian/data/real_creek_reaches.json`.*

**What changed.** The creek is now a **bend train**: a sequence of individually
drawn bends and straight runs, each bend with its own length (log-normal,
median 58 m, σ 0.70), amplitude (0.14 × length, σ 0.5, capped by the apex
radius floor of 25 m and by the physical room `(0.34 − u) · w` to the floor
edge) and apex skew (0.55 ± 0.16 from the upstream end). No function of arc
length has a period; the negative test is the shipped sine's autocorrelation
peak (0.73 at λ) against the train's 0.00 past the first zero-crossing. Zero
RNG draws — the five WATER draws that fed the sine are consumed unchanged, and
a golden test pins the stream position after `fluvial` (unchanged from the
sine build, so every pond stays where it was).

The fluvial carve is the `cut_creek` section now: flat bed to the wet
half-width, a 40 % bank that climbs until it meets the ground, nothing beyond.
Footprint ~5 m (≤10 m at the 2.2 m cut cap), not 32. Both old behaviours stay
under `CREEK_PLANFORM=sine` / `CREEK_CARVE=skirt` until sign-off.

**Step 1, same pipeline both sides (10 m resample, 30 m smoothing):**

| | real NC reaches (6 on the sheet) | bend train, straight corridor | shipped sine |
|---|---|---|---|
| bend length p50 | 50–80 m | 40 m | 85–140 m |
| bend-length CV | 0.45–0.69 | 0.36–0.45 | 0.54–0.66 |
| apex skew p50 | 0.58–0.67 | 0.66 | 0.57–0.60 |
| straight % (R > 150 m) | 46–83 | 47–60 | 60–66 |
| double-crossing (≥35° chords) | 0.1–9.8 % | **1.1 %** | 8.5–8.8 % |
| lateral amplitude p95 | 6–55 m | **15.8 m** | 28 m |

The first pass (σ 0.45, ratio 0.09, R ≥ 35 m) hit every band and still read as
"a line with wiggles": the corpus p50 is dominated by 40 m kinks, while the
swings the eye reads on a real reach are the 100–250 m bends in the tail. The
heavier tail is what made the sheet read; the bands did not move.

**Step 2, eight meander seeds on the ground:** creek intact (one wet body) on
every seed; double-crossing 0.2–2.3 % (median 1.2 %) against the shipped
sine's 1.1–15.2 % (bimodal: six lazy-style seeds at 1.1–2.5 %, two at
13.7/15.2 %); amplitude p95 10–35 m including the trunk's own wander.
Transects: the new section returns to the background within ~5 m of the
water; both skirt variants dip over ±15–20 m, and the train + skirt ablation
shows the trough following every bend — the "creek sculpting its valley" of
round 1, isolated to the carve.

**Open.** The measured skew (0.66) and bend length (40 m) barely respond to
their constants: the instrument quantises the apex to 10 m samples over ~5-
sample bends, and wobble on the straights adds short inflection-to-inflection
arcs. Both sides are measured identically, so the comparison stands, but the
numbers are coarser than they look. Gorge port (step 3) waits on the render
sign-off.

### Section round 1 (2026-09-01, evening)

Owner signed off the creek **paths** on the placement overlay
(`tools/aeolian/creek_overlay_sheet.py`, `CREEK_CARVE=none`) and gave the
section spec: level across the flow (slight U at most), steep bank past the
wet width, smooth transition into the surroundings, realistic grade bounds,
grade/width varying along the creek, curve-aware (straight = narrow and
steep, wavy = wider and gentler). Implemented in `water::fluvial`: U bed
(edges 25 % shallower), per-node bank grade (0.55 base, ±25 % along the arc,
outer ×1.35 / inner ×0.75, wavy reaches down to 65 %, clamped 17–45°),
per-node shoulder (3 m base, ±30 %, inner ×1.4, wavy up to ×1.8, 1.5–8 m),
and the bank meets the ground through `ease::smin` so the top of bank is
rounded, not creased. Sheet: `out/creek/step3_section.html` (new | previous
ramp | shipped skirt, transects ±40 m). Awaiting the owner's eye; the
differences read in the transects more than in the 2 m hillshade.

### Section round 2 (2026-09-02): the corpus, and "lower, don't replace"

Owner: the round-1 stamp read as "a constant cutout without texture". Step 0
measured what a real sandhills creek does to the ground at 2 m
(`tools/aeolian/creek_sections.py`, cached OSM streams joined to 35 corpus
tiles, 38,507 screened transects): **only 35.7 % of mapped stream stations
show a channel the DTM resolves at all**; among those the section is a broad
ragged V — rim depth p10/50/90 = 0.36/1.03/2.05 m at 16–24 m, closure width
p50 16.5 m (~6 % grade), symmetric (asym p10/90 −0.17/+0.17), depth
decorrelating along the stream in ~40 m. No flat bed, no steep bank at 2 m.
Asset `assets/sandhills_creek_sections.txt` (CSEC1: 256 individual transects
+ 370 reach medians, ±24 m at 1 m, sorted by depth in the loader).

The incision (`water::incise`) picks a real section of the depth the
monotone bed demands, rides it on the smoothed ground's cross-slope, and
returns the texture residual over the first 5 m of the bank; ragged distance
warp and notches from world-space noise; min-composed, applied once, never a
fill. Two wrong turns recorded in the commit (normalise-and-scale → 3 m
troughs; absolute rise → shelf and wall) — both caught by transects, not by
any statistic. Sheet: `out/creek/step4_incision.html` (new | round-1 stamp |
bare ground | real tile t03523_07935 with its transects). Routing metric and
wet-body count unchanged by construction (wetting rule untouched). Cost:
~110 ms per tile (3 seeds: 5.49 s with the incision vs 5.15 s with no carve),
inside S4's 250 ms.

### The aeolian port (2026-09-02): both principles, both modes

Owner, after signing off the Carolina gallery: "the same principles applied
to the aeolian seeds." `gorge.rs` drops the two-sine offset for
`planform::bend_train` (confined to the trunk floor, five draws unchanged,
`limit_curvature`/`MEANDER_RATIO` deleted with the sine), and
`water::cut_creek` stops stamping its flat slot and calls the same `incise`,
against a **Nebraska section pack of its own**
(`assets/sandhills_creek_sections_ne.txt`).

Nebraska vs Carolina, same instrument and pipeline:

| | Carolina (nc) | Nebraska (ne) |
|---|---|---|
| resolved at 2 m | 35.7 % | **16.1 %** |
| rim depth p10/50/90 | 0.36 / 1.03 / 2.05 m | **0.57 / 1.54 / 3.00 m** |
| closure width p10/50/90 | 10 / 16.5 / 18 m | 11 / 14 / 17.5 m |
| asymmetry p10/90 | −0.17 / +0.17 | **−0.39 / +0.27** |
| along-stream depth correlation | 40 m | 30 m |

The low resolved fraction is the archetype being itself: the dune uplands
have no integrated drainage, so most mapped lines there are not channels the
DTM can see. What differs per mode is one struct, `water::Incision` (pack,
wet width and its band, depth law, salt); the aeolian keeps `cut_creek`'s own
`0.30 + 0.045 · width_m` so a wide river cuts deeper. All 20 river seeds keep
a single wet body. Galleries: `out/creek/gallery.html` (Carolina),
`out/creek/gallery_aeolian.html` (Nebraska), both with the water as a
toggleable layer.

### Rollback and the lowered slot (2026-09-02, late)

The swept-corridor rebuild was reverted in full (owner: "very smooth looking
profiles that had sharp discontinuities within them"); Carolina verified
byte-identical to the approved build. Chosen direction: **the slot carve,
lowered not replaced**, for both modes (`water::slot_lowered`): the flat-bed,
steep-bank slot that passed the Nebraska river rounds, meeting the smoothed
local ground through the 2 m texture, one station per cell (nearest point on
the segments), only wet width / depth / grade varying along the arc.
Ablations: Carolina `CREEK_CARVE=incise` (the approved corpus incision),
Nebraska `CREEK_CARVE=slot` (the stamp), `none` in both.

**The finding of the round is in the beds, not the carve.** Carved minus
no-creek, along the creek:

| | footprint half-width p50 | excavation p10 | depth on the line p50/p90 |
|---|---|---|---|
| Carolina lowered (4 seeds) | 5–7 m | **0.0** m²/m | 0.11–0.68 / 2.0–2.4 m |
| Carolina approved incision | 33 m | 1.8 | 0.32–0.79 / 2.1–2.5 m |
| Nebraska lowered (600000) | 11 m | 2.3 | **2.34 / 7.20 m** |
| Nebraska slot stamp (600000) | 6 m | 1.9 | 2.34 / 7.18 m |

On the Carolina stretches where the graded bed sits ABOVE the ground (the
`max(zs[i-1])` monotone pass raises the water into every dip; measured
earlier as ~half the creek on some seeds) the carve has nothing to cut and
the water ribbon lies on the surface; the approved incision hid this by
cutting relative to the local ground, which is where its bowls came from. On
Nebraska the bed (`creek_z`, sampled from the 8 m macro before the dune
texture is laid on) sits up to 7 m below the textured ground; the stamp hid
it by stopping its bank at 4.4 m with a wall, the lowered bank runs 50–60 m
there. Both are bed defects with a carve-side symptom. Next, gated: derive
each mode's water level from the textured ground along the creek with a
descending (never perching) monotone pass — the level logic alone, without
the cap, notches and warp that were rejected with the rebuild.

### The delta carve (2026-09-02, night)

Owner on the lowered slot: "the creek cuts look artificial due to being
untextured and containing discontinuities. The implementation with the blobs
looked better due to the texturing." Diagnosis: the two later carves returned
the texture only outboard of 30 % of the bank (a change made while chasing
teeth), and the slot defined the channel from the graded bed, so it vanished
where the bed sat above the ground.

`water::delta_carve`, default in both modes: every bank cell is
`ground − D`, with `D` the corpus channel depth falling to zero along an
averaged corpus bank shape — texture by subtraction, nothing replaced, no
detail return to tune. Depth and width are the creek's own (corpus, smooth
along the arc), the level is the ground under the creek minus that depth
with one descending running-minimum pass, asymmetry a continuous bend
weight, one evaluation per cell against the segments. No notches, warp,
cap, window fade or min-composition. `Water::river_z` carries the level and
the descent invariant is asserted on it.

| carved minus no-creek | Carolina delta (4 seeds) | approved incision | Nebraska delta (2) | stamp |
|---|---|---|---|---|
| excavation p90/p10 along the creek | **2.1–4.8x** | 33–41x | **2.2 / 8.1x** | 40x |
| footprint half-width p50 / max | 20–22 / 32 m | 33 / 51 m | 17–19 / 29 m | 6 / 8 m |
| bank texture ÷ far texture | **1.27–1.53** | 1.24–1.60 | 1.05–1.23 | (wall) |
| depth on the line p50 / p90 | 1.5 / 2.5–3.7 m | 0.3–0.8 / 2.1–2.5 | 2.1–2.9 / 3.2–8.2 | 0.8–2.3 / 2.6–7.2 |
| wet bodies | 1 | 1 | 1 | 1 |

Real corpus: 7.7x (Carolina), 5.1x (Nebraska). Open: Nebraska 600000
reaches 8 m of cut at p90 where the planform climbs off the canyon floor
and the descent must cut the rise — a planform-room question in `gorge.rs`.
Ablations kept: Carolina `CREEK_CARVE=incise|lowered|section|skirt|none`,
Nebraska `slot|lowered|incise|none`.

### The converging floor and the crease (2026-09-02, night)

Owner on the delta carve: "a smooth cut along the length of the creek
varying too subtly if at all and not appearing to have texture or smoothly
blending to the surrounds." The corpus explained it: the real section is a
straight ~5 % rise from the channel (0.12 m at 4 m, 0.46 at 10, 0.82 at 16,
1.33 at 24) with no bank and no rim — the **valley floor converging on the
creek**. Ours was flat for 40 m (the 8 m HAND bins cannot see a 1 m slope),
so every carve sank a trough into a flat floor and drew a rim parallel to
the creek. The corridor was the wrong object.

Split into the two things the real section is: (1) `assemble` and
`gorge.rs` give the floor the measured convergence (`FLOOR_CONV_S` 5.5 %,
easing to a plateau by `FLOOR_CONV_W` 70 m, smooth-max into the HAND
profile, referenced to the trunk axis); (2) `water::creek_crease` is a thin
irregular incision at its bottom — width and depth scalloping at 8–90 m
(±40/30 %), a few metres of bank with a rounded lip, nothing beyond, level
from the ground with one descending pass, `ground − D` per cell.

| median rise above the channel, identical pipeline | 2 m | 4 m | 6 m | 10 m | 16 m | 24 m |
|---|---|---|---|---|---|---|
| real (256 transects) | 0.03 | 0.12 | 0.22 | 0.46 | 0.82 | 1.33 |
| ours, crease + converging floor (598) | 0.06 | 0.28 | 0.51 | 0.76 | 1.02 | 1.53 |
| ours, delta carve (previous) | 0.07 | 0.20 | 0.35 | 0.69 | 1.26 | 1.86 |

Footprint half-width 5–7 m, one wet body per creek. The excavation swing
along the creek (15–46x) is now the deliberate scalloping of a 7 m feature,
not bowls. Side by side with three real reaches at 4x the character matches
for the first time. Awaiting the owner's eye; every earlier carve remains
under `CREEK_CARVE=` for the ablation.

## 2026-09-04 — the gorge floor goes to grade (cut AND fill)

Owner, on Nebraska 600032 with the crease: "when the stream passes through
the mound it appears like the slope grade will be extremely high". Measured:
the water grade there was 0 % (the descending level held flat), the CUT was
17 m deep with a 4.5 m bank — a slot with ~300 % walls. The creek was on the
trunk axis (off-axis p50 2.8 m), so the planform room was not the cause.

Two defects in `gorge.rs`, both fixed under one toggle (`GORGE_THROUGH=off`):

1. **Saturation left a mound on the floor.** `MAX_CUT_M` (62 m, ×0.52 for a
   wide river) is a valley-scale relief valve, and it applied on the trunk
   line too: where the bed demanded more, the floor was the smoothed dune
   surface lowered bodily. Floor above bed on the trunk, 600032: p50 5.7 m,
   p90 18.6 m, max 27.7 m. Now the innermost trunk layer (the trench) cuts
   the residual `want − full` and widens by `sqrt(1 + res / trench_depth)`
   so its wall slope stays in band. Floor above bed after: max 0.3 m.
2. **The cut never filled.** A deflation pan drawn before the gorge (or an
   interdune hollow) on the trunk line stayed a hole; the crease's running
   min then dragged the whole downstream reach to the hole (600006: 4 km of
   creek 13–20 m below its floor, cut p50 12.8 m). Within the trench, ground
   below `bed + conv` is raised to it, the raise fading to zero by the
   middle of the trench wall (`fill_w = 1 − smoothstep(0, 0.6, xs)`).

Instrument: `tools/aeolian/creek_cutdepth.py` (cut = ground 12 m either side
of the line minus the water level; grade over 10 m; ground slope within 30 m).
Viewer: `tools/aeolian/creek_before_after.py` (both builds stacked, `b` to
flip, water toggle, detail at the deepest cut of the before build).

| median over 20 seeds | before | after |
|---|---|---|
| cut depth p90, m | 4.8 | 1.3 |
| cut depth max, m | 10.0 | 2.6 |
| worst cut on any seed, m | 34.5 (600006) | 6.9 (600009) |
| ground slope within 30 m, p99 % | 103 | 40 |
| water grade over 10 m, max % | 5.9 | 3.8 |

The quilt was checked and is innocent (it moves the line by ±0.3 m). The
gorge's `NET_DEBUG` print now reports floor-above-bed on the trunk and under
the creek, the bed's monotonicity and `creek_z`; `lib.rs` prints ground on
the line minus `creek_z` after the gorge, the quilt and the blowouts.
Remaining: 600009 and 600017 still cut ~7 m at one spot each (not yet
examined). Renders: `out/creek/ae_through` vs `ae_crease`;
`out/creek/before_after_through.html`; crops `through_600006.png`,
`through_600032.png`; difference maps `through_diff_*.png`.

## 2026-09-06 — 200-seed mixed run, screened (no fixes)

`examples/mix_dump.rs`: each seed's mode from its own coin, kept only when
the tile carries a creek or river line (seeds 700000+, four workers by
stride). Kept 200 of 645 tried: **155 aeolian, 45 fluvial** — aeolian tiles
carry a river 28 % of the time, fluvial creeks only 7 % (`p_valley_creek`
0.10 in record.rs). Renders in `out/mix200`, screen cache in
`out/mix200/screen/`, page `out/mix200/mix_screen.html` (10.1 MB),
artifact https://claude.ai/code/artifact/8c0c9dd2-d665-404d-a69d-ea5464b873b2.

`tools/aeolian/mix_screen.py` flags, per 2 m cell: **step** (|z − box3| >
0.8 m, away from water), **steep** (> 80 % aeolian / > 50 % fluvial),
**wall** (> 150 %), **perched** (wet cell > 0.3 m above a dry neighbour's
ground), **reversal** (level rising toward the mouth > 0.25 m / 10 m),
**deepcut** (ground 12 m off the line > 5 m above the water), **flat**
(< 0.3 % over > 1 ha). A tile is "flagged" when a class exceeds its notable
size (step 0.5 ha, wall 2 ha, perched 0.1 ha, the creek classes any; steep
never on its own). Score = weighted hectares; the page sorts by it and shows
a 500 m detail at the worst spot of the top 24.

| tiles with class, any / notable | aeolian (155) | fluvial (45) |
|---|---|---|
| step | 155 / 38 | 5 / 0 |
| wall | 152 / 48 | 13 / 0 |
| perched | 148 / 20 | 45 / 44 |
| reversal | 0 / 0 | 33 / 33 |
| deepcut | 24 / 24 | 11 / 11 |
| flat | 0 | 0 |
| slope p99 (median %) | 91 | 18 |
| creek cut max (median m) | 2.4 | 4.2 |

Systematic finds, verified by eye on crops (not fixed):
1. **Aeolian gorge walls above 150 %** on 152/155 tiles (median 1.1 ha, up
   to 17 ha): the inner trench's wall renders black in the hillshade, and
   98 % of the step cells lie on those walls — the 8 m macro's bilinear
   facets on a 200 % slope, which show as blocks in the render.
2. **Fluvial ponds do not fill their basins**: on every fluvial tile the
   largest pond's shore has dry ground below the water level (seed 700124,
   9.8 ha pond: half of the shore's dry cells sit below the level, by up to
   2.7 m). Aeolian ponds show it on 20 tiles.
3. **Fluvial creeks flow into lakes higher than themselves** on 33/45
   tiles: along the line the water surface steps UP toward the mouth by
   1.4 m (median) to 2.9 m where the ribbon meets an impounded lake
   (`surface.max(lvl)` in `fluvial`). No aeolian tile does this.
4. **Deep cuts**: 24 aeolian tiles still cut > 5 m somewhere (the
   through-cut round left 600009/600017 at ~7 m; same family); 11 fluvial
   tiles do too, and the fluvial creek's cut sits at 4.2 m median against
   a 0.35 m crease — the graded `zs` bed lies below the assembled floor,
   the defect noted on the slot-lowered round.
5. Fluvial tiles carry a creek line rarely (7 %), so a mixed run with
   running water is three-quarters aeolian.

## 2026-09-07 — fix 2 + 3: water settles, the creek is backwatered

Owner: fix the mixed-run finds in the recommended order, a viewer after
each. This round is the water stage (`water.rs`), toggle `WATER_SETTLE=off`.

**Cause, traced.** In `find` a tilted table decided which cells were wet
and one flat level was then painted on the body, so on the low side of the
tilt the level stood above dry ground; the fluvial table ponds were clipped
by an opening and by the trunk keep-out and kept their level; the
impoundment was built over the graded BED, which the creek's level no
longer follows (the bed lies metres below the floor the crease cuts), and
its "upstream" walk assumed lower station index = upstream, which on a
mouth-first trunk flooded the reach DOWN to the mouth (seed sets of a
million cells). The creek's own level never knew about the pond it ran
into.

**What runs now.**
- `fill_levels`: a priority flood from the sinks (tile edge; the creek for
  the fluvial table ponds) inward gives every cell its spill elevation;
  walls (a dam) are never entered.
- `settle`: a body is lowered to the lowest spill elevation it reaches and
  every connected cell below that level is wet (four fixed rounds). Nothing
  beside the water is lower than the water; nothing floods that would drain.
- `berm`: a body whose basin would lose half its cells is dammed instead --
  the dry ring is raised to the level plus 0.15 m, tapering out at 30 % --
  never on the creek's banks (`creek_banks`). Most Carolina table ponds are
  sheets on open slopes; settled, five of six drained to nothing, against
  the corpus's 1.7 % water inside a course. Dug and dammed is what they are.
- Impoundments: level = the creek's WATER at the dam + rise (0.8-2.6 m),
  the reach walked upstream along the creek line, the dam a wall of
  `cap·1.8` around the line just downstream of the dam point, the pond
  capped at 4 ha by lowering the dam (six rounds of bisection). Draws stay
  in their transcript order.
- Backwater: walking upstream from the mouth the creek's level never falls
  below a pond it runs through; every valley-floor cell takes the level of
  its nearest station and is wet where its ground is below it. (A spread
  that carried one level sideways crept down the banks to the mouth: always
  lower ground downstream. Bounded per station instead.)
- The station walk over the creek mask is capped at five cells: the mask
  carries channel cells 500 m off the line and the walk reached a pond.

**Dead ends recorded.** A rim rule (stop when the flood drops 0.5 m below
the highest ground popped) fired on floor texture; a growth cap (2x) let
sheet ponds double at a lower level; both replaced by the fill.

**200 seeds, same seeds as the screen** (`out/mix200` → `out/mix200_f2`):

| tiles with class, any / notable | aeolian before | after | fluvial before | after |
|---|---|---|---|---|
| perched | 148 / 20 | 147 / 17 | 45 / 44 | 40 / 2 |
| reversal | 0 / 0 | 0 / 0 | 33 / 33 | 0 / 0 |
| deepcut | 24 / 24 | 24 / 24 | 11 / 11 | 17 / 17 |
| tiles flagged | 79 | 77 | 45 | 17 |

Fluvial water stays in the corpus band (six checked tiles: 1.7-3.4 % of the
tile before, 1.0-2.2 % after; corpus median 1.7 %, p75 3.5 %). The six extra
fluvial deep-cut tiles are the pre-existing family (fix 4): the crease's
level near the mouth sits 5 m below the floor, and an impoundment used to
hide it under water. The 17 aeolian perched tiles are ribbon-edge cells of
the river body standing up to 1 m above a dry neighbour (700375: 371 cells,
4.5 m from the line) -- untouched by this round, to look at with fix 4.

Tests: `incision::never_fills` now allows a raise within 8 cells of standing
water (the berm); 79 green. Pages: after-screen
https://claude.ai/code/artifact/03c4b8b0-ba72-410e-b7d6-8c7be7cad3e2, before/after
https://claude.ai/code/artifact/20311f87-0d48-4f51-8419-28a6cd6afb01. Tool: `tools/aeolian/mix_before_after.py`.

## 2026-09-07 — fix 1 + 5: walls at repose

Find 1 was misdiagnosed: the gorge trench was not the cliff. Ablated term by
term (toggles `GORGE=off`, `GORGE_TEX/ROUGH/DETAIL/THROUGH/FILL/SLOPE/BLUR`,
`BLOWOUT=off`, `QUILT=off`, `SURFACE_TAIL/TALUS/ADVECT`, `TEX_UPSAMPLE=bilinear`):

- **The dune macro itself** (700791: bare 8 m macro slope p99 119 %, max
  538 %, a 65 m knob 100 m wide on a 25 m drawn relief). Real Nebraska
  tiles, 24 of them at 8 m: p99 39 %, max 69 %; (max − p95)/(p95 − p5) 0.43.
  Cause: `shape_body`'s linear crest tail (slope p above t = 1) plus the
  height-proportional advection folding on Train seeds.
- **The through-cut's trench rim**: measured from the apron's FLOOR level,
  the trench rim sat 0.3·full below the apron wherever the apron had begun to
  climb, and the widening pushed the rim out to where that was 20 m — a
  cliff along the trench edge (700046: 5,400 cells above 150 %, 300–570 m
  from the river; 174 with the through-cut off).
- **The plan-view roughness warp**: `x = dist/hw · (1 + 0.52·rough)` with a
  62 m octave; across a 400 m wall the field's gradient makes the wall six
  times steeper than its mean (a 61 m drop in 30 m; 2,085 cells above 150 %,
  six with the warp off). The width swing does the same on the apron.
- The bilinear 8 m → 2 m upsampling printed facets on every steep wall (the
  "step" class on 155/155 aeolian tiles; real tiles: none).
- My first attempt, a mean-slope bound on the trench width, binds nowhere
  and changed nothing; a bed-field blur against nearest-station seams also
  changed nothing. Both recorded so they are not tried again.

**What runs now.**
- `surface::talus_at`: angle of repose, a fixed 40-pass pairwise relaxation
  (4- and diagonal neighbours, mass-conserving) at 0.50 on the dune macro
  (`SURFACE_TALUS=off`) and 0.60 over the gorge cut (`GORGE_TALUS=off`) —
  the real canyon wall's 2 m max is 80 % within 400 m of the Dismal.
- `shape_body`: the crest tail saturates at 1 + TAIL (0.6) with the same
  slope at t = 1 (`SURFACE_TAIL=off`).
- The trench rides on the apron SURFACE: floor at the bed plus the
  converging rise, wall climbing to the apron's own target at that cell.
- The macro comes up through Catmull-Rom (`TEX_UPSAMPLE=bilinear` for the
  old): step cells 3–6x fewer on the same tiles.
- The width-bound on the trench stays (harmless) and the bed blur stays.

Five river tiles, 2 m, after: near-river p99 59–75 % (real 50), p99.9
78–95 (real 62), max 117–256 (real 80); 700007 keeps 114 cells above 150 %
(unexamined; blowout rims or the crease are the candidates).

**200 seeds** (`out/mix200_f2` → `out/mix200_f3`, both with the water fix):

| | aeolian before | after | fluvial before | after |
|---|---|---|---|---|
| tiles flagged | 77 | 37 | 17 | 17 |
| wall, any / notable | 152 / 48 | 109 / 0 | 0 / 0 | 0 / 0 |
| step, any / notable | 155 / 38 | 155 / 0 | 5 / 0 | 4 / 0 |
| perched notable | 17 | 24 | 2 | 2 |
| deepcut | 24 | 25 | 17 | 17 |
| slope p99, median % | 91 | 65 | 18 | 18 |

Real aeolian tile slope p99 is 44–52 %; the remaining 65 is the dune forms'
own steepness (relief over body width with `body_p` 1.6) and the 2 m quilt,
a calibration question, not a defect. Perched on aeolian tiles (24) is the
ribbon-edge family noted with the water round. Pages: after-screen
https://claude.ai/code/artifact/6655b9f5-982f-449c-9a11-75340ef238ea,
before/after https://claude.ai/code/artifact/724e217c-c17d-47a3-a3bd-688f0b496578.

## 2026-09-07 — fix 4: creeks at grade

Deep cuts came in two families, told apart by the ground 60 m either side
of the deepest point. **Aeolian** (25 tiles): high on ONE side only — the
creek at the foot of the trench wall, because the bend train was given a
fixed 119 m of room while the width swing shrinks the trench floor to ~20 m
in places. **Fluvial** (17 tiles): high on BOTH sides — the crease's level is
a running min of the ground toward the mouth, so one rise on the line
(often at the tile edge near the mouth) dug a 6 m trench through the floor.

**What runs now.**
- `gorge.rs`: room from the CUT FLOOR — at each base point, the distance
  either side along the normal at which the cut surface first stands 2 m
  above the floor, less an 8 m margin (`GORGE_ROOM=fixed` for the old).
- `creek_crease`: `cut_cap` (2.0 m Carolina, 3.0 m Nebraska — corpus depth
  p90): the level may not sit deeper than the cap below the ground, and a
  dam backs water up (walking upstream from the mouth the level never
  falls), so a rise the cap cannot cut through becomes a POOL behind it.
  The level still descends to the mouth; the assertion stays. Toggle
  `CREEK_POOL=off`. Pools stand over the floor beside the ribbon: the
  fluvial backwater pass already does this; `pool_beside` does it for the
  aeolian build (150 m reach).
- Bank cells carved below the water line are wet — the ribbon-edge
  "perched" family (floor-level cells just outside the wet half-width,
  0.3 m under the surface).

**200 seeds** (`out/mix200_f3` → `out/mix200_f4`):

| | aeolian before | after | fluvial before | after |
|---|---|---|---|---|
| tiles flagged | 37 | 2 | 17 | 9 |
| deepcut | 25 | 0 | 17 | 0 |
| perched, any / notable | 148 / 24 | 38 / 2 | 41 / 2 | 43 / 9 |
| creek cut max, median m | 2.4 | 1.8 | 4.5 | 3.6 |

Over the whole round (`out/mix200` → `_f4`): tiles flagged 124 → 11;
reversal 33 → 0; deepcut 35 → 0; wall-notable 48 → 0; step-notable 38 → 0;
perched-notable 64 → 11. The nine fluvial perched tiles are the new pools
meeting dry ground below their level outside the backwater's reach --
small (0.1-0.3 ha) and next in line. Fluvial water share rose with the pools
(six checked tiles 1.3-2.8 %, corpus p75 3.5 %). 79 tests green. Pages:
after-screen https://claude.ai/code/artifact/47abbae2-1199-4919-abd1-a81e426ff43f,
before/after https://claude.ai/code/artifact/62cb3f07-dbc2-4ab7-b253-ff6e07e473de.

## 2026-09-09 — fix 4 revised: the floor converges on the creek, not pools

Owner, on the levels viewer: "quite a few mini lakes running across the
creek line ... not normal at this frequency". Confirmed on 700234: the cap
rule pinned the level at a rise near the mouth and the running max behind
it put the whole valley floor under water for 700 m (wet 1.0 → 3.1 % of the
tile); on aeolian tiles the pool spread printed a side lobe at bends.

Cause, one level down: the valley stage slopes the floor at 5 % toward the
TRUNK AXIS and the creek wanders across it, so 40 m off the axis the ground
stands 2 m higher and every bend read as a rise. The descent cost was paid
either as a narrow cut (the deep-cut family) or, capped, as a lake.

**What runs now** (`creek_crease`, toggle `CREEK_REGRADE=off`):
- the excursion `ex = g − level − depth` (≤ 6 m) is REGRADED: subtracted
  from the ground over a width `ex / FLOOR_CONV_S` (20–150 m) with a
  smoothstep, never below the converging floor's own target
  (`level + depth + conv(dist)`), texture kept — the corpus section's floor
  converging on the creek, applied where the creek actually runs;
- `cut_cap` is 6 m in both modes, so only a rise the regrade cannot take
  pools; `creek_crease` returns which stations pooled, and standing water
  spreads beside the ribbon only there (aeolian `pool_beside`) or behind an
  impoundment (fluvial backwater) — the ribbon's own level over a hollow
  beside it is running water, and spreading it was the side-pond source;
- the backwater raise touches only cells whose ground lies below the water
  line (the creek mask carries carved bank cells too, which were being
  given a surface above their own ground);
- the bank holds at the water line plus a 0.05 m lip across the whole
  crease window, not just the scalloped bank width: a texture hollow beside
  the ribbon is filled to the water line rather than left dry (perched) or
  wetted (a side pond). Both never-fills tests allow that raise within 12
  cells of water.

**200 seeds** (`out/mix200_f4_capped` = fix 4 as first shipped → `_f4`):
tiles flagged 11 → 6 (aeolian 2 → 0, fluvial 9 → 6); perched notable 11 → 6;
deepcut stays 0; water bodies over 400 m² within 60 m of the creek per km
back at the pre-fix-4 rate (fluvial p90 0.45 → 0.61, matching f3's 0.63);
fluvial wet share median 2.55 → 1.31 % (f3 1.78; part of the drop is bank
cells no longer counted as water). Pages republished at the same URLs:
before/after https://claude.ai/code/artifact/62cb3f07-dbc2-4ab7-b253-ff6e07e473de,
after-screen https://claude.ai/code/artifact/47abbae2-1199-4919-abd1-a81e426ff43f.

## 2026-09-09 — two owner notes: water teeth, and a creek on a ridge

**Teeth (700234).** A comb of wet cells 6–19 m off the ribbon, one station
wide each, at successive stations' levels. The backwater's beside-spread
was gated per station on `at > lv + 0.05`, and a station cell touching a
berm or a lip lifts `at` by a few centimetres, so alternate stations
qualified. Gate raised to a real impoundment rise (`STANDING_M` 0.30; the
draw is 0.8–2.6 m), and the backwater raise touches only cells already wet.
700234: wet cells 5–25 m off the line 60 → 0.

**A creek on a ridge (700200, 700775).** True: 376 m and 832 m of creek stood
12–15 m above BOTH flanks at 60–120 m. Not something a stream does (an
alluvial ridge on a floodplain is decimetres). Cause: the graded bed is
measured off the routing datum, and where the trunk crosses a hummock with
open low ground beside it the bed sat above that ground; the fill-to-grade
then built the platform. Two fixes in `gorge.rs`:
- the trunk bed runs no higher than the lowest spill elevation within 100 m
  of each station (a priority flood of the pre-cut macro from the tile
  edge), then monotone toward the mouth again (`GORGE_BEDLOW=off`). An
  enclosed pan keeps its rim above the bed and is untouched; an open low
  pulls the bed down and the rest of the trunk deepens to carry it. On the
  two tiles the trunk dropped 15–19 m and the valley deepened ~10 m at 400 m;
  over all 155 river tiles the median valley depth is unchanged (45.2 m,
  real Dismal 46–54), two tiles deepened by more than 5 m.
- the fill-to-grade is capped at each cell's (blurred) spill elevation
  (`GORGE_LEVEE=allow`), so it can never stand above lower ground beside it.
  A first version without the bed lowering left a flat terrace with a scarp
  (700200 flagged `flat` 2.3 ha); recorded, not kept.

200 seeds (`out/mix200_f5`): levee reaches 2 → 0 tiles; flagged 6 → 4 (all
fluvial, small pool-edge perched); deepcut 0; aeolian slope p99 unchanged.
Pages republished at the same URLs (62cb3f07 before/after, 47abbae2 screen).

## 2026-09-09 — item 5 measured; the last four perched tiles

**Item 5, aeolian slopes, measured tier by tier** (2 m tiles split into a
100 m belt tier, a 16 m hummock tier and the texture below it; real = the 24
Nebraska dune tiles, ours = 12 river seeds built with `GORGE=off`):

| tier, slope p50/p90/p99 % | real dune tiles | ours, gorge off |
|---|---|---|
| belt (100 m) | 5 / 12 / 19 | 4 / 16 / 31 |
| hummock (16 m − 100 m) | 7 / 18 / 32 | 6 / 19 / 35 |
| texture (2 m − 16 m) | 3 / 8 / 19 | 2 / 8 / 20 |
| hummock std, m | 1.53 | 1.70 |
| texture std, m | 0.26 | 0.27 |

Texture and hummocks match the real tiles. The belt tier is the gap, and it
is a FORM question: our Mound seeds (belt p99 24, relief 22 m) sit close to
the real mound tiles (19, 19.5 m); our Train seeds (belt p99 36, relief
67 m) are twice as steep as the other real tiles (18) with 40 % more relief
(48 m). `dune_relief_m` for Train is drawn 32–74 m, "measured (course
sites)" in record.rs, and that is a calibration decision the owner made
against course sites, not these tiles — left alone, recorded here. The
gorge adds the rest of the tile-wide excess (real river tiles are steeper
than real dune tiles too: full p99 52 vs 44).

**The last four flagged tiles** (all fluvial, perched 0.11–0.15 ha): the
reach just below an impoundment's dam, backwatered by the next pond, ran
wet between dry lower banks, and a pond's edge along its dam band was
perched — the dam was a wall to the flood only, its ground below the water
it held. Now every impoundment gets `berm` (its dry ring to the level plus
a lip, 30 % taper, the creek's banks kept as the spillway), so the dam is
earth, and the backwater spread no longer treats the dam band as a barrier
(the berm holds the upper pond on its own; the lower pond's water stands up
to the dam's toe). 200 seeds (`out/mix200_f6`): flagged 4 → 0.

Viewer https://claude.ai/code/artifact/237e16f8-58a1-46b2-af4e-73db1d7dbf2e; the final screen
page (47abbae2) republished from this build.

## 2026-09-09 — the dam is an embankment

Owner, on the dam viewer: "sharp narrow land bridges into these lakes with
steep slope". Confirmed on 700135: the berm ring was a single-cell ridge,
and once the next pond's backwater reached its toe it stood as a needle
across the lake with water on both sides; on 700403 a straight bar with a
0.6 m crest.

`embank` (water.rs): at the creek-line point nearest the dam station, a
crest DAM_FREEBOARD (1.2 m) above the pond, DAM_CREST_HALF (2.5 m) either
side of the dam line, faces at DAM_FACE (3:1) up- and downstream, run across
the valley along the normal until the ground stands 0.5 m above the crest
(at most `cap·1.8`). Ground is only raised; the wet ribbon plus one cell is
kept through it as the spillway (the first cut kept the whole bank zone and
left an 18 m notch). Cells raised above their water go dry. Chained ponds
step down: a plan's level is capped 0.3 m below the pond upstream of it
(700135 had the lower pond 0.25 m above the upper). The berm ring stays for
the natural shore, where it is centimetres.

200 seeds (`out/mix200_f7`): flagged 0/200; fluvial water share −0.02 pt
(the dams' footprints). `never_fills` allows a dam's low tail to 40 cells.
Viewer republished at https://claude.ai/code/artifact/237e16f8-58a1-46b2-af4e-73db1d7dbf2e,
screen at 47abbae2.

## 2026-09-13 — fluvial lakes rebuilt: one dam, the valley's own contour

Owner, on the fresh 200-seed screen (seeds 800000+, artifact 0e1b67a2):
aeolian creeks and rivers look good; fluvial lakes "look quite artificial
at all points, and the damming looks quite off" — remove creek-line lakes,
or keep them rarely and rework the damming.

Diagnosis, 51 fluvial tiles, crops beside six real Sandhills-NC ponds
(Asheboro CC, Pinehurst No. 7, Mid Pines, Dormie, Tobacco Road, Carolina
Lakes; `tools/golf/corpus/out/tiles`):

1. **The berm ring.** `berm` raised every shore cell to the level plus a
   lip and dropped a 30 % bank outward: a bathtub with dry ground below
   the water line all round. 30 of 140 bodies carried it on more than 30 %
   of their shore; on 800957 and 800077 the whole perimeter. The screen's
   perched test never saw it (the lip is above the water).
2. **Radial footprints.** An impoundment was a 38–105 m disc swept along
   the trunk, settled, then bisected down to 4 ha: a sausage along the
   valley. A real pond is the valley's contour at the spillway level, with
   an arm up every side draw and one straight edge at the dam.
3. **Uncapped depression ponds.** The 4 ha cap applied to impoundments
   only; ponds filled in closed depressions ran to 21.6 ha (800669), and
   the opening + compactness passes smoothed them into cut-outs. 53 of 140
   bodies were over 4 ha against the corpus's typical 0.2–4 ha.

The site rule the owner proposed (large flats at creek level) is inverted
by the physics: a flat needs a dam around the whole perimeter, which is
the berm. Real Carolina ponds are dammed creek reaches in a confined
valley section, where one short embankment holds a lot of water (93 % of
Sandhills-NC courses carry a body; median 1.7 % water inside the polygon).
Flats at creek level are wet meadow, a cover class later, not open water.

Shipped (`water.rs`, fluvial only; aeolian untouched):
- Depression ponds, their opening/compactness/one-lake-per-meander passes,
  the lake count cap, the under-water flatten and every `berm` call in the
  fluvial path are deleted (`creek_banks` with them). The `pond_min` draw
  and the disc-radius draw are still consumed so every later draw keeps
  its value; the WATER stream ends here, so nothing downstream moves.
- Impoundment = one dam. For each drawn station the candidates are every
  8 m of creek arc within ±200 m, ranked by dam length at the crest
  (`dam_axes` / `dam_extent`: abutment where the ground stands 0.5 m above
  the crest; no abutment within 240 m, or a dam over 220 m, is rejected —
  **220 m is provisional, not measured**). Up to six are tried. The pond
  is a 4-neighbour flood of the untouched ground below the level from a
  seed on the creek just upstream of the dam plane, blocked at the plane;
  a flood that reaches the tile edge or three times the cap does not hold.
  The drawn level is kept if it holds ≤ 4 ha, else the level is bisected
  (seven rounds) between creek + 0.3 m and the drawn one; under 0.2 ha the
  candidate is passed over. Chained ponds still step down 0.3 m,
  upstream first.
- `embank` unchanged in form (1.2 m freeboard, 5 m crest, 3:1 faces) but
  the creek's ribbon crosses it as a broad-crested WEIR capped at the
  pond's level, carrying a sheet of water down the face, instead of a
  slot to the bed. Ground raised above its water goes dry (the footprint).

51 fluvial seeds, before → after (`out/mix200_new` → `out/mix200_lakes`):

| | before | after |
|---|---|---|
| impoundments made / planned | — | 67 / 67 |
| dam length p10/50/90 m | — | 44 / 68 / 114 |
| pond area p10/50/90/max ha (impoundments) | — | 0.6 / 2.0 / 4.0 / 4.0 |
| standing bodies ≥ 0.1 ha per tile | 5.1 | 3.6 |
| bodies over 4 ha | 27 | 0 |
| shore below the water line, p90 share | 0.76 | 0.00 |
| water, median % of tile | 0.94 | 0.50 |
| screen flagged | 0 / 51 | 0 / 51 |

Tests 79 green. Viewer (`mix_before_after.py --pick lake`, details centred
on each build's largest lake):
https://claude.ai/code/artifact/b23a9c26-8b98-4dd7-b12a-45cfcdcf1f30
Open: the dam-length band and the water share against the real polygons
are unmeasured; a rim class for the screen; whether an off-channel dug
pond (Mid Pines has one) is wanted at all.

### 2026-09-13, later — the dams go too: the fluvial terrain carries creeks only

Owner, on the one-dam viewer: "I don't really like the damming too much.
Even though it is realistic I think the terrain looks weird and
artificial around it and the water kind of weird." Looked at three dams
at 0.5 m contours: a 1.2 m embankment with a 5 m crest and a weir notch
is a block with a slot at 2 m resolution; the pond ends in a straight
line against it; the creek leaves the toe between banks that stand below
the pond. Not a bug — what a small earth dam is, and it still reads
wrong because a structure at terrain resolution has no texture to hide
behind. Options put to the owner: (A) remove ponds and dams from the
terrain, the feature is CONSTRUCTION (dug and dammed when the course was
built, placed relative to holes: stages S7/S8); (B) a natural sill, 1–2 m
over 60–100 m of valley, an invented landform. Owner chose A: "I am not
sold on dams needing to be present anyway."

Shipped: `fluvial` draws the pond count, station, rise and disc as
before and builds nothing; the one-dam build is `pub fn water::impound`
for the earthmoving stage to call with a hole in mind (`DAM_MAX_M` 220
still provisional there). 51 fluvial seeds: standing bodies ≥ 0.1 ha per
tile 5.1 → 0, water median 0.94 → 0.22 % of tile (the creek alone),
screen flagged 0/51, tests 79 green. Viewer republished at the same URL
(b23a9c26, details on the before build's largest lake). Aeolian untouched.
Open for the construction stage: whether courses want water at all, and
the corpus's 1.7 % inside the polygon as the target if they do.

## 2026-09-13 — the belt tier re-measured against the real COURSE tiles

The 2026-09-09 belt-tier gap (Train belt p99 36 vs real 18, "2x too
steep") was measured against the 24 clean Nebraska dune lidar tiles, which
04-landform-literature.md already notes were the GENTLE tiles. The right
population for a course generator is the real course tiles: the 32
`sandhills_ne` courses in `tools/golf/corpus/out/tiles` (2 m, 3–4.5 km,
water masked). Tiers as before: belt = 100 m gaussian, hummock = 16 m
band minus belt; relief = belt p1..p99 over the tile
(scratch `belt/measure.py`, ours = the 149 aeolian tiles of the fresh
200-seed run — all river seeds, so the gorge is in every one).

Most of the 32 are town courses on river flats (median relief 36 m, belt
p99 9). The dune-country ones, by name and relief: Dismal River 109 m
(belt p90/99 16/34, hummock std 2.1), Sand Hills GC 83 (12/24, 1.8),
Frederick Peak 83 (19/35, 3.0), Ballyneal 42 (8/14, 1.5); Chamberlain,
Pelican Beach and Bayside are Missouri-bluff and Lake McConaughy sites,
not dunes.

| p10 / 50 / 90 over tiles | relief m | full slope p99 % | belt p90 % | belt p99 % | hummock std m |
|---|---|---|---|---|---|
| ours Train (63) | 66 / 99 / 127 | 60 / 68 / 76 | 14 / 21 / 25 | 26 / 34 / 44 | 2.2 / 3.1 / 3.8 |
| ours Mound (86) | 49 / 79 / 96 | 58 / 66 / 72 | 11 / 17 / 22 | 20 / 28 / 36 | 2.1 / 2.9 / 3.5 |
| real dune-field courses (8) | 44 / 80 / 99 | 43 / 58 / 70 | 8 / 13 / 17 | 14 / 24 / 34 | 1.5 / 2.0 / 2.6 |

Read: every one of our tiles is a Dismal River. Train relief and belt
slopes sit at the TOP of the real course band (median = real p90), Mound
relief matches the real median with belt slopes a third over, and the
hummock tier is ~1.5x the real dune courses for both forms (the gorge is
in both populations' steep tiles, so that is not the whole excess). The
gentler Sand Hills / Ballyneal end of real dune golf is not produced.
Drawn `dune_relief_m` for Train on these seeds: p10/50/90 37/53/68.
Decision belongs to the owner (the dial is "measured (course sites)").

### 2026-09-13 — option 2 built: Train relief 28–64 m, hummock relief x0.7

Owner chose option 2 (pull toward the real median; Dismal River stays at
the top, Sand Hills and Ballyneal become possible). `record.rs`: Train
`dune_relief_m` 32–74 → 28–64; `hummock_relief_m` Train 12–21 → 8.5–15,
Mound 11–19 → 8–13.5. `surface.rs` ablation threshold 1.5 → 1.3 (seed 19
measures 1.47; the test is about the band existing). 79 tests green.

Whole-tile numbers barely moved (Train belt p99 34 → 33, hummock std 3.1
→ 2.8) because every mixed-run tile carries a gorge, and the gorge sets
the tile-wide p99s — exactly as it does on the real Dismal River tile
(whole-tile belt p99 34, 24 with the river masked). Measured again with
everything within 500 m of the river line (ours) or the water mask (real)
excluded, i.e. the DUNES ALONE:

| p10 / 50 / 90 over tiles, dunes only | relief m | belt p90 % | belt p99 % | hummock std m |
|---|---|---|---|---|
| Train before → after | 68 → 61 | 15 → 13 | 27 → 24 | 2.5 → 2.1 |
| Mound before → after | 41 → 40 | 11 → 10 | 20 → 19 | 2.3 → 2.1 |
| real dune courses (8), median | 82 | 13 | 21 | 1.8 |
| Dismal River / Sand Hills | 110 / 83 | 15 / 12 | 24 / 24 | 1.9 / 1.8 |
| Ballyneal / Meadowlark | 42 / 44 | 8 / 8 | 14 / 10 | 1.5 / 1.4 |

After the pull a Train tile's dunes measure as Dismal River or Sand Hills
(belt p90 13 = the real median, p99 24 = both, hummock 2.1 vs 1.9), and a
Mound tile's as the gentler Ballyneal / Meadowlark end. The whole-tile
excess that remains is the gorge on every river seed, which the real
gorge tiles carry too. Screen: 149/149 flagged 0. Renders
`out/mix200_belt`; viewer (`mix_before_after.py --pick slope`, details at
the before build's steepest belt spot):
https://claude.ai/code/artifact/b9b99d34-c4a9-4a96-9b88-4e1e6153037b

## 2026-09-13 — final look: 250 random seeds, every seed kept

`mix_dump --all` (seeds 900000+, four workers by stride; `river` / `dry` as
a fourth index token; the screen page filters on it): 87 aeolian dry, 29
aeolian river, 118 fluvial dry, 16 fluvial river. Renders `out/final250`,
page `out/final250/mix_screen.html`, artifact
https://claude.ai/code/artifact/cada5d33-0067-4727-9965-22b93a9bc23e.
Screen: flagged 9/250, all fluvial dry, all `perched`; aeolian 0/116.
Extra audit (scratch `audit/audit.py`): no non-finite cells, no duplicate
32 x 32 texture blocks on any tile, no flat plateaus (max 1.1 % of a tile
under 0.3 %), tile-scale periodicity peak ≤ 0.13 (aeolian) / 0.23
(fluvial, at the 24 m ring floor = none).

Findings for the owner, by concern:
1. **Pools on the creekless fluvial valleys.** `p_valley_creek` 0.10 puts
   a creek on 12 % of fluvial tiles; the other 88 % carry "occasional wet
   reaches" flooded as POOLS on the trunk (water.rs, `!meander` branch):
   84/118 dry fluvial tiles have them, median 1 and p90 4 bodies of
   0.7 ha (p90 2.6, max 5.0), 9 tiles perched. These are the valley-floor
   ponds the owner rejected, on the tiles the 51-seed viewer never showed.
2. **Fluvial sameness.** Across 134 fluvial tiles slope p99 runs 17–21 %
   and belt relief 32–47 m (p10–p90); every tile is the same honeycomb of
   small valleys. Corpus Carolina median relief 46.9 m. Variety by degree
   only.
3. **Gorge corridor texture.** On aeolian river tiles the rim runs as a
   band of ~100 m lumps distinct from the dune field (900119, 900226,
   900114); plausible at 2 m contours, a pasted corridor at tile scale.
   The known "rim sawtooth" gap.
4. **Isolated mound bodies** on a flat sheet with abrupt margins (900116,
   900166, 900169, 900204); the audit's spikes (|z − median3| > 1.5 m, to
   79 cells on 900116) all sit on their slip faces.
5. **Faint axis-aligned lattice** in flat lee ground on some aeolian tiles
   (900160, 900098, 900225, 900068, 900240; 43–48 m autocorrelation peak
   0.09–0.13, and an 8–10 m checker in the hillshade): the 8 m macro grid
   through the upsampling. Cosmetic.
6. Lakes cut by the tile edge on ~10 % of aeolian tiles (900090, 942 edge
   cells): a window effect.
7. Fluvial slope rises 2.8 → 4.5 % from the tile edge to 400 m in,
   gradually; aeolian 7.5 → 8.9. Not a seam.
Straight steep runs of up to 2.2 km are the gorge itself (river tiles only).

## 2026-09-14 — final pass, item 5: the axis-aligned crosshatch

Owner: a subtle checker / crosshatch, always aligned with x and y, on the
500 m details of nearly every aeolian tile (900226, 900114, 900116,
900144, 900166, 900099, 900231; 900160 and 900034 at tile scale).

**Measured first** (`tools/aeolian/axis_power.py`, new): ratio of spectral
power within ±8° of the axes to ±8° of the diagonals on the 2–64 m
residual, per wavelength band; plus the RMS of the 2–16 m band by cell
index mod 24 (the 48 m patch pitch), max/min. Real 2 m lidar (Dismal
River, Sand Hills, Ballyneal) measures ~1.0 in every band ≥ 6 m and
1.5 at 4–4.5 m (the 3DEP 1 m → 2 m bilinear decimation's own lattice);
ripple 1.02–1.06. Ours, 20 tiles per mode of `out/final250`:

| median | 4–4.5 m | 6–7 | 7.6–8.4 | 8.4–9.5 | ripple x / y |
|---|---|---|---|---|---|
| aeolian before | 13.2 | 2.8 | 3.2 | 2.2 | 1.30 / 1.32 |
| aeolian after | (≈0 power) | 1.22 | 1.02 | 1.32 | 1.10 / 1.08 |
| fluvial before | 3.3 | 1.16 | 1.60 | 1.40 | 1.14 / 1.16 |
| fluvial after | (≈0 power) | 0.91 | 1.08 | 1.12 | 1.10 / 1.10 |
| real NE source tiles | 1.56 | 1.17 | 1.19 | 1.21 | 1.02 / 1.03 |

So the pattern was in the height data, not the viewer, with three sources
in `texture.rs`, each confirmed by its phase signature:

1. **The 48 m patch-quilt lattice, both modes.** Hann windows sum to 1 at
   half overlap so the MEAN is flat, but the patches are independent
   samples and the RMS goes as sqrt(Σw²), which swings 0.5–1 per axis
   (0.25–1 as the x·y product). The measured RMS-by-phase profile matched
   that curve at r = 0.86–0.97. Lattice anchored at world (0, 0) on every
   seed. Fix: accumulate Σw², output `acc / sqrt(Σw²)` (constant RMS;
   1.01 on synthetic independent patches), each patch's own mean removed
   at paste time (the pack is a 64 m high-pass of the tile, not of the
   patch), the lattice phase offset per seed from the draw already taken,
   and the patch pick a hash of (draw, tile) rather than an arithmetic
   progression (a thin bucket put the same patch under overlapped
   neighbours, correlated: ripple 1.13 with the normalisation alone).
   `OVERLAP_ADD_RMS = 0.76` (the old mean loss, (mean sqrt(w²+(1−w)²))²)
   applied as a constant so `texture_gain`'s band calibration holds:
   fine 2–16 m RMS after/before 0.97 (aeolian) / 0.98 (fluvial); without
   it +17–19 %.
2. **The 8 m nearest-neighbour texture gate, aeolian.** `g = floor +
   (1−floor)·smoothstep(mpos)` was read with `round(w/8)` on the macro grid
   and multiplies the residual by a 3–6x swing: a staircase with 8 m
   treads in x and y. The excess sat on the two cells straddling each
   block edge and vanished where the gate saturates. Fix: bilinear on the
   8 m grid (`Grid { spec: mspec, data: mpos }`).
3. **Fluvial v2 returned a bilinear 8 → 2 m upsample** (`base.bilinear` at
   three sites), so the 2026-09-07 Catmull change never reached the
   fluvial surface, and the residual carried the Catmull−bilinear lattice
   re-amplified. Fix: `catmull_rom_2d` at all three; the separable 26 m
   box in the fine/coarse split (Dirichlet nulls on the axes, the "boxy"
   kernel family) is a gaussian of the same sigma.
4. **The 2-cell lattice the sources carry** (1.5x in the real tiles,
   2.4x on ours after the fluvial fine gain): a [1 2 1]/4 in x and y on the
   normalised residual, before the gate. Absolute 4–4.5 m power fell ~100x
   (axis 0.15 → 0.002 of the 11–14 m diagonal reference); the band ratio
   there is now noise over ≈0 power.
5. **Viewer**: `.detail` sized so the 500 px image sits at 1 px = 1 m and
   `image-rendering: auto` (a 500 px image in a ≤482 px box under
   `pixelated` beat at 8–27 px with the window width).

Left as is: `lib.rs` `u2/w2/d2` (gates, bilinear, not height); the
8.4–14 m aeolian excess (1.24–1.32) matches the real NE source tiles
(1.21–1.23), i.e. inherited fabric, not ours; ripple 1.08–1.10 against a
real 1.02–1.06. 79 tests green; screen on the 48 rebuilt tiles: aeolian
0 flagged, fluvial 1 (a creekless-valley pool, item 1). Viewer
https://claude.ai/code/artifact/91a69192-4804-445b-baf7-c6cef97c963d. Still an 8 m facet on steep slip faces (900099): the macro's own resolution, not the quilt.

## 2026-09-14 — final pass, item 1: basin lakes on the creekless valleys

Owner wants lakes for golf (Pinehurst has them) but never perched above
a low point, and natural. Diagnosis of the old pools (`water.rs`,
`!meander` branch): a 68 m disc per station flooded to the downstream
bed + 0.42 with no spill test and no backwater (that pass needs a creek
line): 84/118 creekless tiles carried them, 9 perched. And no basin can
exist on a trunk: `carve.rs:197` forces every bed to rise strictly
upstream.

**Shipped: `water::basin_lake`** (record in the function doc). A smooth
scoop is sunk into the finished 2 m ground along a reach of the trunk —
subtraction only, texture intact — deepest just above the untouched
downstream sill and shoaling upstream, its footprint following the
valley's own coordinate `u` (so it takes side draws with it, which is
where the arms come from); the reach's along-arc weight is read at each
cell's NEAREST station so nothing downstream of the sill is lowered (a
disc per station reached past it and cut the sill: first bug). Then the
water is found: an 8-connected priority flood gives every cell its spill,
`settle8` lowers the level to the lowest spill the body reaches and wets
every connected cell below it. The LEVEL sizes the lake, bisected between
the basin's floor + 0.4 m and its spill for the largest that holds the
target area (where the sill stands well above the reach's floor — the
trunk's own fall, a natural hollow — even a 0.4 m scoop floods hectares
at the spill: second finding); a lake below its spill is not perched,
it is a lake not full to the brim. The written level is the SETTLED one
(the body's own spill can lie below the seeds': third bug, 1.2 m of
perched water on 900054 until fixed). Passed over, draws consumed: a
reach within 400 m of the last lake, one that runs off the trunk, under
0.2 ha, or no basin. Dials `record.rs`: `lake_weights` [0.30, 0.45, 0.25]
(0/1/2 per tile; corpus: 93 % of NC courses carry a body, typically two),
`lake_ha` 0.3–3.0 log-uniform. Draws at the WATER tail, always two slots;
the pool draws stay consumed; transcript pins `GOLDEN` (creek seed,
unchanged) and new `GOLDEN_DRY` 3992857827 (seed 600035). Tests:
`basin_lakes` (one body in band, never perched, one level, open outlet,
nothing raised, texture kept, deterministic, taken reach passed over).
83 tests green.

134 fluvial seeds of the final look, `out/lakes_fix`: 87 lakes on 75 of
118 creekless tiles (0 / 1 / 2 lakes on 59 / 65 / 10 tiles); passed over
15 too near, 8 under 0.2 ha, 2 off the trunk, 2 no basin; area p10/50/90
0.38 / 0.77 / 1.65 ha, max 2.57, 79 of 87 within 70–105 % of target.
Screen: flagged 0/134, perched 0, `shore_below` 0 on every creekless tile
(the 0.06 worst is a creek ribbon's bank, unchanged); creek tiles
byte-identical to the texture-fix render. New screen rows: standing
bodies per tile, body area, shore-below share. Viewer
https://claude.ai/code/artifact/62d5ec46-35c7-4125-b7e5-264714c0b167.

## 2026-09-14 — final pass, item 4: the Mound form's isolated bodies

Owner: three styles of creekless aeolian tile — dune train (900043),
dune-dominant (900098), isolated mounds (900116) — the last too frequent;
is it real, how to fix, how to tune its likelihood.

**Instrument** (`tools/aeolian/mound_bodies.py`, new; there was none):
on the 100 m belt above a 1.2 km rolling floor, cover (belt > 30 % of
p95), bodies/km² (8-connected), lowland relief (belt − floor, std, off
the bodies), margin slope, and the sparse flag (cover < 0.40 and > 1.2
bodies/km²). Real = the 54 Nebraska tiles split by a spectral
orientation proxy (A < 0.5 mound-like, n = 39).

| p10 / 50 / 90 | cover | bodies / km² | lowland relief m | margin % | sparse |
|---|---|---|---|---|---|
| real mound-like (39) | 0.39 / 0.54 / 0.87 | 0.22 / 0.56 / 1.13 | 1.3 / 2.5 / 4.8 | 4.5 / 6.3 / 8.1 | 2 / 39 |
| ours before (50 Mound dry) | 0.30 / 0.45 / 0.63 | 0.32 / 1.00 / 1.90 | 0.9 / 1.3 / 1.9 | 2.8 / 3.9 / 4.9 | 15 / 50 |
| ours after | 0.51 / 0.60 / 0.72 | 0.22 / 0.55 / 1.00 | 0.9 / 1.4 / 2.0 | 2.2 / 3.2 / 4.2 | 0 / 50 |

Not real: twice the bodies, each smaller, on a lowland with half the
relief; margins were never steeper than real — the flat sheet is what
read as abrupt. Mechanism (`surface::supply`, `shape_body`): the knee
`smoothstep(-0.30 + 0.55p, 0.28, n)` narrowed from 0.45 to 0.11 as
patchiness rose, so coverage and margin sharpness were one dial; the
off-patch floor kept only 1 − p of the megaform; `body_p` 1.6 crushed
what survived. The owner's reading (a low-frequency high-relief draw
with a low-amplitude high-frequency one) is what that produces.

**Shipped:** the knee's width has a floor (`KNEE_MIN` 0.29, binding only
above patchiness ~0.53, so every Train tile is byte-identical — checked
on 10); Mound `belt_patchiness` 0.45–0.85 → 0.50–0.85 with the width
floor; `body_p` Mound 1.6 → 1.15. Tried and reverted: hummocks let into
the lows (`hummock_gate` 0.12–0.28, `hummock_floor` 0.30–0.45) — the
lowland relief did not move and playable pads fell 0.72 → 0.59;
`dune_relief_m` +10 % — the prominence instrument swung 0.82 → 1.19
between near-identical builds (`dune_stats.py prominence_p50` is ±40 %
tile to tile, nan on a third), so relief stays at the calibrated 15–31.
Final prominence 0.76× real (p25/75 5.3/11.6 m) — recorded, not chased.
Playable-pad share (≤ 8 % at 30 m in ≥ 40 m pads) 0.73 → 0.73. Screen 0
flagged on 50. Lowland relief stays ~0.57× real: that is the small dunes
real interdunes carry, and it costs playability to add.

**Tuning the likelihood:** the sparse style is the top of the
`belt_patchiness` draw. 0.85 lands it at 0–2 in 50 (real 2 in 39); a
lower ceiling removes it, a higher one brings it back. Viewer
https://claude.ai/code/artifact/6043838c-0f62-43e1-a905-b315481974b7.

## 2026-09-14 — final pass, item 3: the gorge rim, a masked blur

Owner: the rim band reads fine, a quick fix only, no rework. Diagnosis
(`gorge.rs`): tributaries no longer cut (the comments at `HEAD_TAPER`
and in `build` said they did — corrected); the rim's lumps are the
plan-view fields `wide` (300/118 m, `WIDTH_SWING` 0.70 on the half-width)
and `rough` (165/62 m, `WALL_ROUGH` 0.52 warping the wall coordinate),
and the 8 m macro's own facets on a 60 % wall; nothing smoothed the cut at
that scale (`h0g` ~15 m, the final pass ~4 m, `talus_at` clips slopes
only). The comment claiming 420/155 m octaves was wrong (300/118 ship);
the owner kept the shipped pitch.

**Shipped:** after `talus_at`, the cut above the trench floor — 0.3 to
60 m of cut, fading out over the deepest 30 m — takes a 25 m blur
(`blur8` × 40 on the 8 m macro), the 2 m quilt landing afterwards. A
first cut at 0.3–10 m (the shallow apron ring only) did nothing visible:
the lumps sit on the apron and upper wall where the cut is deeper.

29 aeolian river seeds of the final look (`out/rim_fix`), corridor 60–500 m
from the river: |∇²| RMS of the 12 m-smoothed surface 15.2 → 9.1 (−40 %),
of the 30 m-smoothed 8.2 → 4.5; slope p99 66 → 49 % (real Dismal at 2 m
near the river: p99 50, p99.9 62), p90 43 → 30. Screen: flagged 0/29,
wall 1 → 0 tiles. By eye (900119, 900114 at 1 km): the polygonal facets
are rounded, the notches remain. Viewer
https://claude.ai/code/artifact/rim-placeholder.
