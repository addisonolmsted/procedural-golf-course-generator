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
https://claude.ai/code/artifact/31efc5d4-6fc7-402d-92b7-22a1794ba588.

## 2026-09-14 — the 250 seeds again, with all four fixes

Same seeds as the final look (`out/final250` list), rendered at the head
with items 5, 1, 4 and 3 in: `out/final250_v2`, page
`out/final250_v2/mix_screen.html`.

| | before (cada5d33) | after |
|---|---|---|
| tiles flagged | 9 / 250 | 0 / 250 |
| perched, any / notable | 79 / 9 | 0 / 0 |
| step, any tiles | 132 | 6 |
| wall, any tiles | 66 | 6 |
| spikes (\|z − median3\| > 1.5 m), p90 cells | 5 | 0 |
| standing bodies, tiles with any | 84 fluvial (pools) | 77 fluvial (basin lakes) + 46 aeolian (interdune) |
| axis power, 8 m band, aeolian / fluvial | 3.2 / 1.6 | 1.00 / 1.09 |
| pitch ripple, aeolian / fluvial | 1.30 / 1.15 | 1.10 / 1.09 |
| Mound sparse-style tiles | 15 / 50 | 0 / 67 |
| aeolian slope p99, median % | 52 | 46 |

No duplicate texture blocks, no flat plateaus, no reversal or deep cut,
no non-finite cell. Left as recorded per item: the 8.4–14 m aeolian axis
excess matches the real source tiles; Mound lowland relief ~0.6× real;
prominence instrument too noisy to tune against; creek-ribbon banks on
river tiles carry the only shore-below cells (0.06 worst, pre-existing).
Artifact: https://claude.ai/code/artifact/f8032308-90ca-4eae-9e90-633d1b8682ca.

## 2026-09-14 — freeze: timing, and the aeolian water table's 7 seconds

`final_pass` (the real entry point, timer around the generator only),
100 seeds from 910000, single thread, release: aeolian tiles took a
median 7.8 s against ~0.5 s in August. `SAND_TIME=1` stage timers (new,
`lib.rs::build_full` and `water::find`): 7.2 s of it was the water
table's local floor (2026-08-29): a 700 m window of ~3,400 samples fully
SORTED at every one of the 141k 8 m nodes. Now computed on a 24 m grid
with quickselect and read bilinearly (a 700 m-window percentile does not
change across 24 m): `water::find` 7.4 s → 0.43 s. 116 aeolian seeds
re-rendered: mean lake share identical, 115/116 within 1e-4, height
unchanged but on one tile's berm (0.8 m); screen 0 flagged; 83 tests.

| ms per seed, n = 100 | median | mean | sd | min | max |
|---|---|---|---|---|---|
| all | 1288 | 1250 | 400 | 689 | 2075 |
| aeolian (54) | 791 | 986 | 343 | 689 | 2075 |
| aeolian, no river (32) | 723 | | 38 | | |
| aeolian, river (22) | 1241 | | 278 | | |
| fluvial (46) | 1577 | 1560 | 177 | 1310 | 1938 |

Branch `sandhills` pushed to origin for the first time (2026-09-14).

## 2026-09-14 — speed: the fluvial quilt, and rayon on the per-cell loops

`SAND_TIME=1` on a fluvial seed: network 220, assemble 2 × 227, quilt
679, fields 30, water 300 ms. The quilt's 679 against the aeolian 97:
the fluvial variant sampled the Catmull upsample three times over the
grid and sorted 2.25 M cells for two percentiles (selection now; same
values), then 277 ms of per-cell gains (three Perlin octaves per cell).
Every per-cell loop of the 2 m texture is a pure function of its inputs,
so they run in parallel (rayon, row chunks) with output bit-identical to
the serial form — proved the same way as the clean-up, 96/96 files
identical on the 32-seed sample. Also the aeolian lake finder's per-cell
table loop. Fluvial quilt 679 → 192 ms, aeolian 97 → 53, `find` 434 →
382. The overlap-add paste itself stays serial (its float sums would
reorder under a parallel split).

| ms per seed, n = 100, release, 10-core M-series | median | mean | sd | min | max |
|---|---|---|---|---|---|
| all | 944 | 971 | 291 | 599 | 2004 |
| aeolian (54) | 706 | 901 | 347 | 599 | 2004 |
| aeolian, no river (32) | 640 | | 39 | | |
| aeolian, river (22) | 1154 | | 281 | | |
| fluvial (46) | 1071 | 1054 | 175 | 808 | 1435 |

Where the rest sits: fluvial assemble (two 8 m passes, 455 ms) and the
network (220 ms) are sequential algorithms; the aeolian river adds the
gorge and the 2 m priority flood (`fill_levels`, 175 ms). Next levers, if
wanted: one assemble pass (the first exists only to re-base hanging
gullies), and the gorge's per-channel EDT.

## 2026-09-14 — S6 in Rust: the routing prototype ported, 250/250 identical

The Python prototype (`tools/golf/{siting,greens,routing}.py`,
`dtm_primitives/geomorphons.py`, ~2,600 lines) is ported into
`crates/course-routing` (~6,500 lines: terrain, img, geom, fields, morph,
siting, greens, route, dump; `examples/route_batch.rs`). Owner's scope:
the router consumes the frozen Sandhills terrain directly (2 m height +
water surface + creek lines) — the v2 C2/C3 contracts are retired on this
branch, par is a router output — and is accepted on STATISTICAL parity
with the prototype. It came out stronger than that.

Reference: `tools/golf/batch_route.py` (new) ran the Python prototype on
the 250 frozen seeds (`out/final250_v2` → `out/route_ref/*.jsonl`):
250/250 routed, mix (2,5,2)/(3,3,3)/(1,7,1) = 229/14/7, total length
p10/50/90 3121/3157/3215 m, 0 crossings, 38.9 s per seed. The Rust batch
(`route_batch`, same record shape via `dump.rs`) on the same 250:
250/250 routed, the SAME pars, total lengths (to 0.1 m), clubhouse and
all nine greens on every tile — 0 differences — at 0.33 s median, 0.45 s
max (S6 budget 1300 ms; Python 39 s).

The kernels (`img.rs`) are verified against SciPy 1.16 / NumPy 2.1 on 430
reference cases (edt, rect/disc/ring filters, gaussian, gradient,
percentile; reflect borders, ties) to 1e-9 or exactly. Deliberate
deviations, all documented in-file: the beam's `round(-score, 6)` tie-break
is a total order (score, green tuple, par tuple); the prototype's no-op
loop and double diagnostic assignment are gone; the joint tee x LZ retry
has an explicit cap; `place_lz` has the prototype's actual 23 bearings
(the docstring said 24); SciPy's `percentile_filter` is an order statistic,
not linear interpolation, and is ported as such; Python's banker's
rounding is reproduced where `int(round())` set a cell. No annealing pass
(the prototype has none). 61 tests. Sheet:
`tools/golf/route_sheet_json.py` renders routes from the batch records.

## 2026-09-15 — routing round 1, items 0 + 1: instruments, and no green in another hole's line of play

Plan: `~/.claude/plans/lets-start-addressing-some-eager-teacup.md` (routing
refinement round 1, ten items, owner-approved 2026-09-14). Baseline for the
whole round is the Rust port on the 250 frozen seeds,
`out/route_rs/rs_v1.jsonl` (identical to the Python prototype). Walks are
explicitly NOT optimised this round (owner).

**Item 0, instruments (no behaviour change).** `dump.rs` record v2 (per
hole: index, kind, green_idx, approach_bin, terms, tee_graded, tee_slope,
bridges with kind, walk path; per route: crossings, green_in_play,
coverage). `tools/golf/route_audit.py <dump> <a.jsonl> [<b.jsonl>]` — the
37-row before/after table (edge, coverage, LZ / green setting, above
chord, water, lengths, sequence, green in play, tee slopes, guards).
`tools/golf/route_before_after.py` — stacked before/after cards, hold `b`,
`--pick` orders by the change in one metric. `route.rs` plumbing:
`build_route_fields(f, m, p, win)` (surround / interest rasters for items
5–6), `SiteCtx`, `run_routing(t, sit, f, m, p, pool, pool_fn)`. Spines
byte-identical to the baseline.

**Item 1, green in play.** Cause: `geom::clearance_violation` dropped ALL of
hole A's samples past t 0.82 (its green included) for a consecutive pair,
so hole B could play straight over A's green, and nothing else looked at a
green against a spine. Fix: keep every sample; skip only the pair
`consecutive && ta > 0.82 && tb < 0.18` (regression test: A (0,0)→(0,300),
B (−150,300)→(150,300) → 1.0, was 0.0). Term: for every (A's green, B's
spine), `v = clip((50 − d)/50, 0, 1)` with GREEN_CLEAR_M = CLEAR_MID_M
(the corpus mid-hole p10 clearance); route `−GIP_W·v²` and −12 under
GREEN_VETO_M 30; beam `−GIP_W·v²` and −8 under 30 (new line vs earlier
greens, new green vs earlier spines); `place_lz` gains a veto tier (both
legs ≥ 30 m from every earlier green), `place_tee` folds `v` into `viol`.
Exemptions: the successor's first 60 m of arc (the tee beside this green,
walks p50 67 m), and — added after the ladder — hole 1's first 60 m
against the ninth green: both are pinned to the clubhouse disc, so the
loop junction is the same corridor mouth as the consecutive one (five of
the seven residual cases were this pair at 30–34 m).

Ladder (250 seeds; cases = greens within 35 / 50 m of another hole's
spine outside the exemptions, audit definition before the loop exemption,
baseline 78 / 122): ×1 (GIP_W 4) → 13 / 64; ×2 (8) → 7 / 57, shipped;
×4 (16, diagnostic) → 4 / 42. No hard tier beyond the plan's: the
residuals sit at 30–35 m where a 30 m tier cannot act.

Shipped (`out/route_rs/rs_s1.jsonl`, audit with the loop exemption,
`out/route_rs/audit_s1.txt`):

| metric | baseline | shipped |
|---|---|---|
| green within 35 m of another hole's spine, cases | 48 | 1 |
| within 50 m | 74 | 23 |
| routed / play crossings | 250 / 0 | 250 / 0 |
| (2,5,2) mix | 92 % | 91 % |
| total length p50 | 3157 | 3157 |
| walk p50 | 993 | 992 |
| seconds p50 / max | 0.33 / 0.45 | 0.33 / 0.38 |
| tee boxes on > 30 % ground | 39 | 52 (item 2) |
| hole 2 a par 3 | 40 % | 33 % (incidental) |
| water courses with a carry | 0 % | 15 % (incidental: moved greens) |

Viewer: artifact 14a67972 (24 courses ordered by the change in green-in-play
cases, hold `b` for before). Rung-by-rung numbers live on `GIP_W` in
`route.rs` and in `docs/calibration/routing-site-use.md`.

## 2026-09-15 — routing round 1, item 2: no tee box on a cliff

Cause (two): `tee_boxes` fell back to the unchecked stagger point when no
cell of the ±10 m lateral scan passed the pad masks, and the masks read one
8 m node (the floor cell) while `z8` is a stride sample — a box whose floor
node was gentle could have a 40 % node under its far edge. Now
`box_slope(p)` = the max over the 2 × 2 nodes around the point, the cap
`TEE_BOX_SLOPE_MAX = 0.15` (owner, provisional: a 7 m pad benches on 15 %
without a wall) gates every accepted point, the scan retries at 0.75× and
0.5× of the stagger on the mask tier then a graded tier (any dry point
under the cap), then a graded scan 3 m ahead of the previous box, and
finally a pad coincident with the previous box (`length_m` non-increasing
throughout). `TeeBox.slope` records the 2 × 2 max.

250 seeds (`out/route_rs/rs_s2.jsonl`, `audit_s2.txt`, vs s1): boxes on
> 30 % ground 52 → 0; > 15 % 345 → 1 (900181 hole 9's back tee, which
`place_tee` vetted on the floor node alone — item 3+ territory, one box);
graded boxes 3,297 → 2,987 of 11,250; coincident pads 135 (1.2 %);
spines, pars, totals, score and mix unchanged (tee boxes do not feed the
score); seconds max 0.38 → 0.42. Viewer: artifact b5c053b2 (`--pick tee`).

## 2026-09-15 — routing round 1, item 3: the soft window

Owner (2026-09-14): a soft window with a compactness reward — holes may use
the halo where the ground is good. Three levers, all in one commit:
`greens.rs` zeroes the pool's `d_boundary_norm` pair (−2.37 linear /
+7.05 quadratic; a matched design feature in the corpus fit, whose
parabola scored the halo OUTSIDE the window +0.3 logit above the
mid-band, so `hot_seeds` filled from the rim; DEVIATION on the fit
reference test); `siting.rs` softens the clubhouse (`CH_EDGE_W` 1.0 →
0.3, `CH_CENTRAL_BOX` (0.35, 0.65) → (0.42, 0.58), `CH_HALO_M` 100 → 40);
`route.rs` charges every green by its signed distance to the window edge
(`EDGE_W 0.6`, free at ≥ 60 m inside, −0.3 on the line, −0.6 at ≥ 60 m
outside; beam cheap score and detail `terms["edge"]`; the reserved loop
anchor exempt).

The compaction crowded greens back into lines of play (35 m cases 1 →
18), so item 1 was re-baselined in the same commit: `GIP_W` 8 → 16 (the
ladder's ×4 rung, guards held), `GREEN_VETO_M` 30 → 35 (every residual
sat at 30–35 m: a successor's tee 70 m from the green playing past it 27°
off the line), and a green abeam of or behind a tee (closest point = the
spine's start) is not in play (7 of the 18 were greens 33 m beside a
non-adjacent tee). `place_tee` also gains a cap-only third tier (item 2).

| metric | s2 | s3 (shipped) |
|---|---|---|
| greens < 60 m from the edge | 57 % | 23 % |
| greens outside the window | 46 % | 7 % |
| window within 100 m of play, p50 | 49 % | 62 % |
| walk p50 | 992 | 808 m |
| green in play ≤ 35 m / ≤ 50 m | 1 / 22 | 0 / 60 |
| (2,5,2) mix | 91 % | 85 % |
| hole 9 par 5 / hole 2 par 3 / b2b | 41 / 33 / 4 % | 58 / 42 / 8 % |
| within-course LZ setting std | 0.18 | 0.15 |
| routed / crossings | 250 / 0 | 250 / 0 |
| seconds p50 / max | 0.34 / 0.42 | 0.35 / 0.48 |
| tee boxes on > 30 % | 0 | 4 (one hole: the budget-pinched saturating fallback, item 9) |

Ladder (35 m / 50 m green-in-play cases, all with the edge term): GIP_W 8
→ 18 / 99; + abeam exemption → 13 / 90; GIP_W 16 → 8 / 64; + veto tier
35 m → 0 / 60 (shipped). The mix sits at the band floor and the sequence
signatures rose (compact loops close with a long ninth): items 9–10.
Viewer: artifact 6f90f00d (`--pick coverage`).

## 2026-09-15 — routing round 1, item 4: coverage

Beam: `BeamState.cov`, a bitset over a 40 m lattice on the window
(aeolian 37 × 24, fluvial 25 × 18); each new straight hole stamps the
cells within 120 m of its line and earns `COV_W_BEAM · new / n_cells`
(a fresh 400 m hole in open ground ≈ +0.33 at 3.0; a hole beside an
earlier one or out in the halo nothing). Route: `coverage` = the share of
the window's 8 m cells within 120 m of any placed spine, `rterms
["coverage"] = 2.0 · coverage`, the 100 m share as a diagnostic;
`Route.coverage` fills the record's slot. The saturating tee fallback in
`detail_route` now takes the nearest dry node under the 15 % cap within
40 m (item 2 follow-up).

Ladder on COV_W_BEAM (window within 100 m of play, p50 / hole-9 par 5 /
mix / 50 m green-in-play cases): s3 62 % / 58 % / 85 % / 60; ×⅔ (2.0)
65 / 52 / 86 / 52; **×1 (3.0) 66 / 43 / 87 / 57 — shipped**; ×4/3 (4.0)
66 / 44 / 86 / 44. Flat: item 3's edge term already did most of the
interior work; 3.0 kept the ninth-hole signature lowest.

Shipped (`rs_s4.jsonl`, `audit_s4.txt`, vs s3): coverage100 62 → 66 %,
coverage120 69 → 72 %; walk p50 808 → 861 m (holes spread into the
interior: reported, not guarded); hole-9 par 5 58 → 43 %; mix 85 → 87 %;
250/250, 0 crossings, green-in-play ≤ 35 m 0; seconds max 0.47. Tee
boxes on > 30 %: 5, all one hole (900114's closing par 3, forced 334 m
from the previous green onto ground with no capped node within 40 m —
the beam's loop lookahead, revisit under item 9). Viewer: artifact
2e7aac2a (`--pick coverage`).

## 2026-09-15 — routing round 1, item 5: green setting

`route::pool_setting`: `sett_raw = trapezoid(surround, 3.5, 14, 0.25,
0.4) + 0.5·clip((relief_pos − 0.35)/0.30, 0, 1)` per pool candidate
(real greens: surround p25/50/75 2.9/4.9/7.9 m, relief_pos p25→p75
0.31→0.67), ranked within the pool like the fit score (`rank01`, factored
out of `pool_pct`); beam cheap score and detail `terms["setting"]` at
`SETT_W`.

Ladder (green surround p50 all / aeolian / fluvial; green upland;
coverage100; edge < 60 m): s4 4.7 / 5.2 / 4.4; 12 %; 66 %; 23 % — 0.3:
4.9 / 5.5 / 4.6; 13 %; 64 %; 24 % — **0.6 (shipped): 4.9 / 5.8 / 4.5;
15 %; 60 %; 25 %** — 1.2: 5.0 / 6.1 / 4.6; 18 %; 57 %; 28 %.

Aeolian meets the 5.5 m target at ×1; fluvial does not at any rung, and
the ladder shows why: the fluvial windows have the ground (surround
p50/p75 of the window 4.9/6.8 m, of green-grade cells 4.3/5.9) but the
pool offers little of it — the rank term can only choose among
`greens::generate`'s candidates, whose fit favours calm ground there. The
lever is the pool (its typed detectors / hot seeds on fluvial), not this
weight; parked for the owner. Coverage gives back 66 → 60 % (the target's
floor) and the edge share 23 → 25 %: better greens sit where they sit.

Shipped (`rs_s5.jsonl`, `audit_s5.txt`): 250/250, 0 crossings, mix 89 %,
green-in-play ≤ 35 m 0, tee boxes on > 30 % 0 (900114's ninth moved),
seconds max 0.48. Viewer: artifact bccba2cc (`--pick setting`).

## 2026-09-15 — routing round 1, item 6: landing-zone setting

`place_lz`'s score was room 0.5, flatness 0.3 linear from 0 %, water 0.1,
remainder 0.4, dogleg −0.6 — the flattest legal cell on the annulus won.
Now flatness is free to `LZ_SLOPE_FREE` 3 % and zero at the 8 % fairway
gate, the water trapezoid pays 0.3 over 15–60 m (`LZ_WATER_BAND_M`, item
8's lateral-hazard band), and `interest40` (the step-0 raster: surround
relief, |tpi200|, saddle/peak/pit, maxed over 40 m) earns
`LZ_INTEREST_W`. Gates and tiers untouched; `terms["lz_interest40"]`
(diagnostic, not scored) records the best feature near the hole's LZs.

Ladder on LZ_INTEREST_W (LZ dead flat / setting std / walk p50 / mix):
s5 46 % / 0.16 / 828 / 89 % — 0.2: 27 % / 0.16 / 836 / 90 % — **0.35
(shipped): 26 % / 0.16 / 827 / 88 %** — 0.5: 26 % / 0.16 / 828 / 89 %.
Flat: the dead-flat drop comes from the flatness ramp, which every rung
shares; the interest weight orders among already-good cells.

Two acceptance lines stay unmet and are not this item's to meet: the
within-course LZ setting std (0.16, target > 0.25) and LZ upland (12 %,
target > 20 %) are `relief_pos` classes of points 200–250 m along each
hole's line — they follow where the beam puts the holes, not which cell
of a ±45° annulus `place_lz` picks. The "feature within 40 m" line
saturates (100 %; `lz_interest40` p10/50/90 0.74/0.82/0.98): a 40 m
max-filter over a raster that scores surround relief from 3.5 m finds
something nearly everywhere on these tiles, so the metric does not
discriminate and is reported, not claimed.

Shipped (`rs_s6.jsonl`, `audit_s6.txt`): 250/250, 0 crossings, mix 88 %,
green-in-play ≤ 35 m 0, tee boxes > 30 % 0, coverage 61 %, seconds max
0.51. Viewer: artifact fb39fcc7 (`--pick lz`).

## 2026-09-15 — routing round 1, item 7: line of play

`route::line_terms(f, m, spine, n)`: `pen_chord` = the PROF_CHORD ramp
(free to the corpus p90 1.7 m, saturating at the p99 5.6) on the max rise
of the sampled line above its chord; `flow` = mean over samples of
`|sin(theta − rise_axis)|` on slope / hollow / footslope / valley ground
(geomorphon classes 6–9; 1 along the contour or down the valley, 0
straight up the face), 0.5 elsewhere, centred. Beam (9 samples on the
straight tee→green line): `+LINE_FLOW_W·flow − LINE_CHORD_W_BEAM·pen_chord`.
Detail: `terms["line_flow"]` exact on the placed spine (8 m samples);
`prof_chord` −0.8 unchanged. New here: `place_lz` charges each candidate's
legs the same chord penalty (`LINE_CHORD_W_LZ` 0.8).

Ladder (above-chord p50 / p90 / p99; walk p50; mix): s6 0.81 / 4.9 /
16.8; 827; 88 % — beam 0.8: 0.66 / 3.4 / 9.1; 832; 88 % — beam 1.2: 0.67
/ 3.0 / 7.6; 806; 86 % — beam 1.6: 0.65 / 3.0 / 7.3; 787; 86 % — **beam
1.2 + place_lz 0.8 (shipped): 0.63 / 2.4 / 7.5; 794; 89 %**. The beam
saturated at 1.2 (its straight line stopped being what the audit
measures; the placed dogleg was), so the plan's own fallback applied:
the lever moved to detail placement.

Shipped (`rs_s7.jsonl`, `audit_s7.txt`, vs s6): p90 4.9 → 2.4 m (target
≤ 2.5), p99 16.8 → 7.5 (target < 8), p50 0.81 → 0.63 (target 0.5:
fluvial 0.48, aeolian 0.75); 250/250, 0 crossings, green-in-play ≤ 35 m
0 (≤ 50 m 56 → 76: holes bend around rises), tee boxes > 30 % 0,
coverage 61 %, seconds max 0.52. Viewer: artifact 4b4738c1 (`--pick
above`).

## 2026-09-15 — routing round 1, item 8: water as a hazard

Owner: lateral hazards AND short forced carries. `route.rs`: `hazard_at`
= trapezoid(d_water, 15, 60, ramp 0.25, tail 0) — water in reach of a
miss, far water earning nothing; detail `terms["hazard"] =
HAZARD_W_DETAIL · max over the LZs and the green`, beam
`HAZARD_W_BEAM · hazard_at(green)`. `carry_term`: a SPINE bridge of
15–70 m (`CARRY_BAND_M`, owner-provisional) is a forced carry and earns
`CARRY_W` once per hole; a span over 70 m costs `(−0.3 − 0.005·(span −
70)) · WATER_SCALE` and forfeits every water credit (a hole crossing a
lake is not using it as a hazard); under 15 m is a ditch, free. Walk
bridges keep the old charge (floor −1.5). The beam estimates the carry
on its straight line over `wet8` at `CARRY_W_BEAM`. All four weights
share `WATER_SCALE` (0.5 / 0.3 / 0.4 / 0.3 at ×1).

The ceiling: 14 of the 70 water courses have no wet cell inside the play
window (9 none within the 120 m halo) — the router cannot reach those
without re-siting, so the reachable target is ~56 of 71, not the plan's
"> 50 of 71" as written.

Ladder on WATER_SCALE (water courses with a hole within 40 m / with a
carry / spine spans > 70 m / mix): s7 24 / 10 % / 0 / 89 % — ×0.5 32 /
15 % / 1 / 88 % — ×1 35 / 21 % / 1 / 87 % — ×2 44 / 31 % / 3 / 87 % — ×3
48 / 42 % / 7 / 88 % — ×4 52 / 46 % / 13 / 84 %. The long spans were
72–88 m crossings the beam accepted for a −0.35 charge the other terms
outbid; withholding the credits on a long span barely moved them (×3: 7
→ 6), scaling the charge with the credits did (×3: 6 → 4). **Shipped
×3 with the scaled charge**: 48 / 41 % / 4 / 87 %.

Shipped (`rs_s8.jsonl`, `audit_s8.txt`, vs s7): water courses with a
hole near water 24 → 48 of 71; with a carry 10 → 41 % (target ≥ 40);
carries per water course 0.11 → 0.83; spans over 70 m 0 → 4 (guard
deviation, reported: 900068 h2 82 m, 900179 h2 142 m, 900206 h2 80 m,
900246 h4 80 m); 250/250, 0 crossings, green-in-play ≤ 35 m 0, tee
boxes > 30 % 0, above-chord p90 2.4, coverage 63 %, seconds max 0.62.
Viewer: artifact de250471 (`--pick water`).

## 2026-09-15 — routing round 1, item 9: hole lengths

`PAR_BAND_*` stays the legal gate; the scores reward the corpus
interquartile ranges (`PAR_TARGET_3/4/5` = 143–183 / 317–383 / 446–500,
5,201 holes) — par 4 as a TENT (1 at the 350 m median, 0.5 at the
quartiles, 0 at 284/416), par 3 and 5 as the flat IQR with a 20 m ramp
— in the beam (0.9), `place_tee`'s `band_t` (1.1) and detail
`terms["length"]` (0.5); pace midpoints 163 / 350 / 473. Spread by
construction: the second par 3 / 5 targets the OUTER quarter of its band
on the side the first did not take (`band_override` on `place_tee`);
the beam charges `TWIN_W` 0.8 when a new par 3 / 5 lands within 30 / 40
m of an earlier one's estimate; route `spread`: par 3 +0.5 if range ≥
30 m AND one ≤ 150 m, −0.3 if < 15 m; par 4 +0.5 at ≥ 80; par 5 +0.5 at
≥ 40. `TOTAL_BAND_M` (2800, 3250), `BUDGET_M` = its ceiling (the budget
had been truncating late holes to `lo + 20`), and the total reward is a
tent at the real par-36 nine (2·163 + 5·351 + 2·474 = 3,029 m), 0 at
±250.

Passes (par 3 median / par-3 pairs within 20 m / par 4 / par 5 / total
p50 / mix): s8 185 / 50 % / 346 / 518 / 3,147 / 87 % — (a) flat IQR
rewards, other-half override, twin 0.4, flat total to 3,250: 165 / 34 %
/ 374 / 490 / 3,177 / 91 % (par 4s pinned to the top of a flat reward;
the total rose) — (b) + tent total, outer-quarter override, twin 0.8:
165 / 14 % / 371 / 489 / 3,154 / 91 % — (c) tents for every par: 163 /
52 % / 359 / 488 / 3,139 / 82 % (both par 3s pulled onto 163; the mix
fell) — **(d) tent for par 4 only (shipped): 165 / 11 % / 358 / 487 /
3,123 / 94 %**.

Shipped (`rs_s9.jsonl`, `audit_s9.txt`, vs s8): par 3 median 185 → 165
(target 160–170), pairs within 20 m 50 → 11 % (≤ 15), a par 3 ≤ 150 m
on 38 → 57 % of courses, par 5 518 → 487 (< 500), par 4 346 → 358, total
p50 3,147 → 3,123 (target 3,000–3,100: the sum of the shipped medians
is 3,094), hole-9 par 5 51 → 28 % (a side benefit ahead of item 10),
mix 87 → 94 %; 250/250, 0 crossings, green-in-play ≤ 35 m 0 (≤ 50 m 76
→ 110), tee boxes > 30 % 0, above-chord p90 2.6, seconds max 0.56.
Side effect reported: water courses with a carry 41 → 32 % (shorter
holes cross less water; near-water 48 holds). Viewer: artifact 7ae0e537
(`--pick length`).

## 2026-09-15 — routing round 1, item 10: the sequence prior, and the round closed

Route `rterms["sequence"]`: −`SEQ_W_ROUTE` 0.4 for a par 3 at hole 2, the
same for a par 5 at hole 9; the beam mirrors it at 0.3 when it places
those holes. No ladder needed: ×1 met both targets. Shipped
(`rs_s10.jsonl` = `rs.jsonl`, `audit_s10.txt`, vs s9): hole-2 par 3 36 →
14 % (≤ 20), hole-9 par 5 28 → 22 % (≤ 25), mix 94 → 93 %, b2b 5 %;
250/250, 0 crossings, seconds max 0.51. Viewer: artifact 636b3cac.

**The round, baseline (`rs_v1`, the Python-parity port) → final
(`rs_s10`)**, `audit_round1.txt`; viewer artifact 186e6318 (30 courses):

| metric | baseline | target | final |
|---|---|---|---|
| greens within 60 m of the window edge | 57 % | < 25 % | 25 % |
| greens outside the window | 45 % | ≤ 10 % | 9 % |
| window within 100 m of play, p50 | 48 % | > 60 % | 64 % |
| LZ dead flat / upland | 48 / 12 % | < 30 / > 20 % | 27 / 8 % |
| within-course LZ setting std | 0.18 | > 0.25 | 0.15 |
| green surround relief p50 (real 6.1–6.8) | 4.6 m | > 5.5 | 4.9 (ae 5.3, fl 4.7) |
| above-chord p50 / p90 / p99 (real 0.08 / 1.7 / 5.6) | 0.82 / 4.4 / 14.1 | ≤ 0.5 / < 2.5 / < 8 | 0.59 / 2.7 / 7.6 |
| water courses with a hole within 40 m (of 71; ~56 reachable) | 27 | > 50 | 47 |
| water courses with a carry | 0 % | ≥ 40 % | 35 % |
| spine spans over 70 m | 0 | not up | 5 |
| par 3 median / both within 20 m | 180 / 37 % | 160–170 / < 15 % | 167 / 10 % |
| par 5 median | 528 | < 500 | 492 |
| par 4 median / total p50 | 353 / 3,157 | — / 3,000–3,100 | 358 / 3,126 |
| hole 2 par 3 / hole 9 par 5 | 40 / 39 % | ≤ 20 / ≤ 25 % | 14 / 22 % |
| green within 35 m of another hole's line (audit def.) | 40 | 0 | 0 |
| green within 50 m | 64 | — | 110 |
| tee boxes on > 30 % / > 15 % ground | 39 / 329 | 0 / ≤ 5 | 0 / 0 |
| routed / play crossings / (2,5,2) | 250 / 0 / 92 % | 250 / 0 / 85–95 % | 250 / 0 / 93 % |
| walk p50 (not optimised) | 993 m | — | 911 m |
| seconds p50 / max | 0.33 / 0.45 | < 1.3 max | 0.44 / 0.51 |

Open after the round (each explained in its item's section): LZ upland
and within-course LZ setting std are hole-level properties the LZ term
cannot move; fluvial green surround is pool-limited (`greens::generate`);
the water ceiling is siting (14 windows hold no water); five spine spans
of 72–142 m; and the 50 m green-in-play count rose 64 → 110 while the
35 m count fell to 0 — compaction puts greens 35–50 m off other lines,
which the 50 m term charges but does not forbid; whether that band needs
a tier is the owner's call.

## 2026-09-15 — routing round 2, items 0 + 3: real shot profiles, real par positions

Round 2 plan (owner-approved 2026-09-15): bend around dunes / weave, reward
carries over low points, match the real par-by-hole distribution.
Baseline `out/route_rs/rs_s10.jsonl`.

**Item 0a — `tools/golf/corpus/shot_profiles.py`** (the `holes.py` chain on
the same 5,201 ways; 3,662 par 4/5 ≥ 250 m): the DRIVE leg (arc 0 → min
(220 m, 0.63·L)) against its own chord dips ≥ 1.5 m on **46.7 %** of real
drives (≥ 3 m on 22.6 %), p50 among those 2.95 m, p90 6.3; drive rise
p90 1.24 m; drives net downhill (p50 −1.3 m). Par-5 second legs dip
≥ 1.5 m on 21.8 %. Doglegs (max vertex turn, legs > 20 m): p50 16.5°,
p90 43.2°, 53.8 % bend > 15°, the bend at t = 0.64, 4.4 % node-straight;
par-5 S-shapes **6.8 %**. Region-invariant doglegs (p90 ≈ 43° everywhere);
piedmont has the deepest drive dips.

**Item 0b — `tools/golf/corpus/par_positions.py`** → `out/par_positions.json`
(412 par-36 nines: 220 front, 192 back): par 3 by hole 3/24/33/28/20/22/
30/33/8 %, par 5 22/22/22/20/23/23/23/19/27 %; mix (2,5,2) 94 %; b2b 3s
0 %, b2b 5s 4 %; hole-1 openers 75 % par 4, 22 % par 5.

**Item 0c — audit rows** (`route_audit.py`): hole-1 / hole-9 par 3, b2b
3s, the par-by-hole table distance (mean |ours − real| over 27 slots),
dogleg p50/p90 and share > 15°, S-share, "straight line crosses a rise →
spine stays under 1.7 m", drive / second-leg dips; real references read
from the two JSONs. Viewer picks `dogleg`, `dip`; `sequence` now counts
the opener and b2b 3s.

What the rows say about s10 (the round-1 final), by the corpus's own
definitions: doglegs p50 28° / p90 53°, 77 % bend > 15° (real 16.5 / 43 /
54 %) — our holes bend MORE than real, not less; only 28 % of the bends
resolve a rise the straight line would cross; par-5 S-shapes 50 % (real
7 %); drive dips ≥ 1.5 m on 34 % (real 47 %), p50 2.6 m (real 2.95).
Item 1's target is therefore purposeful bends (fewer, where a rise asks),
not more bending; the weave credit is dropped (S-shapes are rare), and
S-shapes should fall.

**Item 3 — the real par sequence.** `PAR_POS` (the 412-nine table) →
slot value `clip(ln(share / marginal), −2, +1)`; route `SEQ_W_ROUTE · Σ`,
beam `SEQ_W_BEAM` per placed hole; back-to-back par 3s removed from
`legal_pars` with an adjacency-aware look-ahead (`mix_feasible_seq`; the
brute-force test now counts 588 + 700 + 72 legal sequences). Also: the
beam checks every earlier hole's line against the reserved loop anchor
(the ninth green is known from the start), hole 1's first 60 m exempt.

Ladder (SEQ_W route / beam; table distance, hole-1 par 3, hole-2 par 3,
hole-9 par 3, hole-9 par 5, mix): s10 0.060, 22, 14, 19, 22, 93 % —
0.2 / 0.15: 0.049, 6, 37, 13, 30, 94 — **0.4 / 0.3 (shipped): 0.044, 2,
37, 10, 33, 95** — 0.8 / 0.6: 0.069, 0, 32, 4, 36, 94 (over-steers).

Shipped (`r2_s3.jsonl`, `audit_r2_s3.txt`): b2b 3s 3 → 0 %, hole-1 par 3
22 → 2 %, hole-9 par 3 19 → 10 %, hole-9 par 5 22 → 33 % (real 27; back
nines 33), hole-2 par 3 14 → 37 % (real 24: the par 3 pushed off hole 1
lands on hole 2, where the prior is neutral — the largest residual slot),
table distance 0.060 → 0.044; 250/250, 0 crossings, mix 95 % (target
90–95), seconds max 0.55. Green-in-play ≤ 35 m: 1 case (900171, hole 1's
line 28 m past the ninth green at arc 100 m — both pinned by the
clubhouse disc; the beam paid −11 for it and had nothing better).
Viewer: artifact da3c4101 (`--pick sequence`).

## 2026-09-15 — routing round 2, item 1: purposeful bends

Reframed by item 0's numbers: our holes bent MORE than real (vertex turn
p50 29° / p90 53°, 77 % > 15°, S-shapes 53 % of par 5s; real 16.5 / 43 /
54 % / 7 %) and only 28 % of the bends resolved a rise the straight line
crossed — `place_lz`'s room / interest rewards bought bends the terrain
did not ask for, while its old dogleg ramp (free to 15° of BEARING
deviation ≈ 37° of turn) never bit. Changes (`route.rs`): the dogleg
penalty is now in TURN terms on the real band (`turn_at`, free to the
p50 16.5°, saturating at the p90 43°, `DOGLEG_W` 0.6); a par-5 second
leg turning against the first pays `S_PEN_W` 0.4 (weaving legal, rare);
a RISE TIER in `place_lz`'s ranking — legs over a rise ≥ `RISE_TIER_M`
rank below every candidate that goes around (like the green veto); the
beam scores the best of the straight line and two lines bent ±12° of
bearing at the approximate LZ (`BEAM_BEND_DEG`, ≈ 30° of turn), so green
pairs reachable by bending survive to detail; `LINE_CHORD_W_LZ` 0.8 →
2.4; `line_terms` returns a struct (flow, chord penalty, above, below).
The weave credit was dropped (real S-shapes 6.8 %).

Ladder (chord weight; rise tier; dogleg p50 / p90 / > 15° ; S-share;
straight-line rise resolved by the spine; above-chord p90 / p99; mix):
r2-s3 — / — ; 29 / 53 / 77 %; 53 %; 29 %; 2.7 / 7.8; 95 % — 1.2, tier at
the p99 5.6 m: 14 / 38 / 46 %; 15 %; 24 %; 2.8 / 5.4; 92 — 1.6: 15 / 43 /
47 %; 15 %; 27 %; 2.6 / 5.3; 91 — 2.4: 15 / 49 / 48 %; 15 %; 31 %; 2.5 /
5.3; 91 — **2.4, tier at the corpus p95 3.5 m (shipped)**: 15 / 48 /
48 %; 15 %; 31 %; 2.5 / 4.7; 92.

Why the "resolved" share stalls at ~31 % (target ≥ 50): a diagnostic on
the 209 aeolian par 4s whose straight line rises ≥ 1.7 m — 75 % have a
candidate on the LZ scan under 1.7 m, but the bend it needs is p50 33° /
p90 45° of BEARING (turns well beyond the real p90). Resolving them at
real-band doglegs is geometrically impossible for most; the tier now
forces the bend only where the rise is ≥ 3.5 m (real p95), the rest
trade off. Aeolian pays in dogleg p90 (57° vs real 43°): dunes are
where going around costs a big bend. The owner's "not every hole" is
respected by construction.

Shipped (`r2_s1.jsonl`, `audit_r2_s1.txt`, vs r2-s3): dogleg p50 29 →
15°, p90 53 → 48°, holes bending > 15° 77 → 48 %, S-shapes 53 → 15 %,
above-chord p90 2.7 → 2.5 (aeolian 3.1 → 2.8), p99 7.8 → 4.7, total p50
3126 → 3089 (straighter holes are shorter; inside the 3,000–3,100
target), 250/250, 0 crossings, mix 92 % (aeolian 88), green-in-play ≤
35 m 1 → 0, tee boxes > 30 % 0, seconds max 0.51. Viewer: artifact
1a61df82 (`--pick dogleg`).

## 2026-09-15 — routing round 2, item 2: carries over low points; the round closed

`line_terms` now also returns `below` (max dip under the chord) and
`climb100`; `carry_dip` = trapezoid(dip, 2.0–6.3 m, 0.25, 0.4) — full
from 2 m (between the 1.5 m carry threshold and the real p50 2.95) to the
real p90 of dipping drives, the 0.4 tail keeping a deep swale a carry, and
nothing when the climb-out exceeds `PROF_CLIMB_100_SAT` (a canyon is not a
carry). Paid at `DIP_W` on the tee→LZ1 and LZ1→LZ2 legs only (owner: the
approach stays with the green terms): `place_lz`'s candidate score on
its first leg, detail `terms["carry_dip"]` (max over the full-shot legs),
the beam on its straight line's first 220 m at half weight.

Ladder (DIP_W; drives carrying ≥ 1.5 m / par-5 second shots / depth p50
/ dogleg p90 / S-share / mix): r2-s1 35 % / 25 % / 2.5 / 48 / 15 % /
92 — **0.3 (shipped): 51 % / 34 % / 2.4 / 51 / 19 % / 90** — 0.6: 62 /
43 / 2.4 / 62 / 21 / 91 — 1.2: 75 / 54 / 2.4 / 72 / 29 / 81. The real
shares are 46.7 % and 21.8 %; the ×0.5 rung lands on the drive share and
the higher rungs buy carries with big bends and a broken mix. The depth
of the dips (2.4 m vs real 2.95) does not move: the band is full from
2 m, so nothing asks for deeper — left as is (the owner's ask was that
carries happen, not how deep).

Shipped (`r2_s2.jsonl` = `rs.jsonl`, `audit_r2_s2.txt`, vs r2-s1): drives
carrying ≥ 1.5 m 35 → 51 %, second shots 25 → 34 %, above-chord p90
2.5 → 2.3 (aeolian 2.8 → 2.7), p99 4.7 → 4.6, dead-flat LZs 24 → 23 %,
250/250, 0 crossings, mix 92 → 90 % (aeolian 84), seconds max 0.56.
Viewer: artifact 3cbbff28 (`--pick dip`).

**The round, round-1 final (`rs_s10`) → round-2 final (`r2_s2`)**,
`audit_round2.txt`; viewer artifact f0b9a727 (30 courses):

| metric | real | before | after |
|---|---|---|---|
| hole 1 par 3 / hole 9 par 3 / hole 9 par 5 | 3 / 8 / 27 % | 22 / 19 / 22 % | 0 / 12 / 30 % |
| hole 2 par 3 | 24 % | 14 % | 34 % |
| back-to-back 3s | 0 % | 3 % | 0 % |
| par-by-hole table distance (27 slots) | — | 0.060 | 0.043 |
| dogleg p50 / p90 | 16.5 / 43° | 28 / 53° | 15 / 51° |
| holes bending > 15° | 54 % | 77 % | 52 % |
| par-5 S-shapes | 7 % | 50 % | 19 % |
| straight-line rise resolved by the spine | — | 28 % | 35 % |
| above-chord p90 / p99 | 1.7 / 5.6 | 2.7 / 7.6 | 2.3 / 4.6 |
| drives carrying a ≥ 1.5 m dip | 47 % | 34 % | 51 % |
| par-5 second shots carrying a dip | 22 % | 24 % | 34 % |
| total length p50 | 3,029 | 3,126 | 3,096 |
| routed / crossings / gip ≤ 35 m / tee > 30 % | — | 250 / 0 / 0 / 0 | 250 / 0 / 0 / 0 |
| mix (2,5,2) | 94 % | 93 % | 90 % |
| seconds p50 / max | — | 0.44 / 0.51 | 0.46 / 0.56 |

Open after round 2: the resolved-rise share (35 %; the bends needed
exceed the real band, forced only over 3.5 m rises); hole-2 par 3 at
34 % (the opener's par 3 lands there; the prior is neutral at hole 2);
aeolian dogleg p90 57° and mix 84 % (dune ground asks for bends; the
sequence rule removed (3,3,3) sequences with adjacent 3s); dip depth
2.4 vs 2.95 m; and the round-1 leftovers (fluvial green pool, LZ upland,
the 35–50 m green band, the water ceiling).

## 2026-09-15 — routing round 3: you should be able to see where your drive lands

Plan: `~/.claude/plans/lets-start-addressing-some-eager-teacup.md`, owner
approved. Baseline `out/route_rs/r2_s2.jsonl` (round 2's final).

**The measurement that set the scope.** `tools/golf/corpus/blindness.py`
(new, 4,874 real holes on their own 2 m tiles) and `route_audit.py` (ours),
one definition: an eye 1.7 m above the ground at the station, the target the
GROUND at a centreline point, the sight line straight in plan, blocked when
intermediate ground breaks it by more than 0.3 m. Real courses blind one
drive in five (20.1 %) but with a gentle roll — the cut that would open the
shot is p50 0.72 m and only 3.6 drives per 100 holes need more than 1.5 m.
Ours blinded half and needed more than 1.5 m on 17.2 per 100.

Green and approach blindness is NOT routing's: a 1 m tee bench and a 0.5 m
green pad take par-5 green blindness from 11 % to 6 % against a real 6 %,
and par 3 from 18 % to 7 %. It is an artefact of measuring ungraded ground
and S7/S8 removes it. Owner's decision: routing takes the deep tail only,
threshold 1.5–2 m, drive leg only, the shallow remainder left to earthmoving.

**Item 0, instruments.** `blindness.py` records the drive obstruction depth,
its position as a fraction of the leg, and the blocked length; its cell
lookup changed from rounding to TRUNCATION to match `holes.py`,
`route_audit.py` and the router's `trunc_clip` (the real figures moved
trivially: LZ blind 20.7 → 20.5 % on par 4). `route_audit.py` factors
`blocked` into `obstruction` and gains six rows plus a par-5 second-leg
diagnostic; `route_before_after.py` gains a `blind` pick.

**Item 1, the visibility tier.** `sight_block_m(t, a, b)` — the first reader
of the 2 m heights in the router; `line_terms` samples the 8 m STRIDE grid
nine times over a 250 m leg, 28 m apart, which cannot see a crest narrow
enough to hide a drive. Sampling mirrors `wet_spans` (same `trunc_clip`) and
`route_audit.py::obstruction` (same step count, same interior-only test), so
the audit measures exactly what the router enforces. `SiteCtx` carries the
terrain. In `place_lz`'s FULL-clean tier a drive blocked by more than
`VIS_TIER_M` ranks below every candidate that can be seen, evaluated lazily;
a blind candidate still qualifies as the play-clean fallback, so a hole with
nothing visible on its annulus degrades to a blind drive, not to a walk
crossing. `clean` folds the visibility in, which is what makes the TEE move:
it drives the existing joint tee × landing-zone retry through the six tee
options, so a tee in a hollow is abandoned automatically. Drive leg only
(owner); the par-5 second leg is measured at 46 % blind and left alone.

Ladder (deep drives per 100 par 4/5 at > 1.5 m and > 2 m; LZ blind;
obstruction depth p50/p90; seconds max): baseline 17.2 / 10.9; 49 %; 1.12 /
2.93; 0.56 — 1.0 m: 7.3 / 2.9; 46 %; 0.87 / 1.76; 0.84 — **1.5 m (shipped):
10.7 / 3.7; 47 %; 0.96 / 1.86; 0.53** — 2.0 m: 13.7 / 5.7; 49 %; 1.03 /
2.09; 0.58. Two things were built, measured and REMOVED: promoting the tier
to the play-clean band moved nothing (11.0 vs 10.7), because the residual is
a green no tee can see; and a beam-side mirror on the 8 m grid made it WORSE
(deep drives per 100: tier alone 3.7, + beam at 0.3 5.7, at 0.6 5.0) — the
stride grid is too coarse to rank greens by visibility and only perturbs the
selection the detail tier then has to work with. As in round 2: detail
placement is the lever.

Shipped (`r3_s1.jsonl` = `rs.jsonl`, `audit_r3_s1.txt`):

| metric | real | before | after | target |
|---|---|---|---|---|
| blind drives needing > 2 m of cut, per 100 | 2.2 | 10.9 | **3.7** | ≤ 4 |
| needing > 1.5 m | 3.6 | 17.2 | 10.7 | ≤ 8 (missed) |
| obstruction depth p50 / p90 | 0.72 / 2.08 | 1.12 / 2.93 | 0.96 / 1.86 | — |
| landing zone blind from the tee | 20 % | 49 % | 47 % | ≤ 40 % (missed) |
| obstruction position | 0.77 | 0.65 | 0.66 | ≥ 0.70 (missed) |
| green blind from the approach | 6 % | 16 % | 15 % | S7's |
| routed / crossings / gip ≤ 35 m / tee > 30 % | — | 250/0/0/0 | 250/0/0/0 | guards |
| mix (2,5,2) | 94 % | 90 % | 92 % | 85–95 |
| seconds p50 / max | — | 0.46 / 0.56 | 0.47 / 0.53 | < 1.3 |

The primary target is met: the deep tail is down 66 % and now sits within
1.7× the real rate. Three secondary targets are missed and the reason is the
same for all three — the residual blind drives are holes where the GREEN sits
behind the ridge, so no tee or landing zone on the annulus can see past it,
and the only fix is a different green, which the beam experiment showed we
cannot select cheaply. Round-2 metrics held or improved (above-chord p90 2.3
→ 2.2, dip carries 51 → 53 %, mix 90 → 92 %, par-by-hole distance 0.043 →
0.041). One drift to report: **dogleg p90 51 → 59°** against a real 43, since
a landing zone that can be seen is often wider off the line; the p50 is
unchanged at 15 and the share bending over 15° at 52 %, so the median hole is
unaffected and the tail is longer.

Viewer: artifact 04282918 (`--pick blind`); hole cards refreshed on the new
build at artifact d8badecf.

## 2026-09-16 — the corpus gap report: every hole measured against 4,900 real ones

Owner asked what else could be compared to the corpus besides blindness, and
flagged that the dogleg measure looked wrong. Both answered.

**The dogleg measure was wrong and is replaced.** The old one was the largest
turn at an interior vertex with both legs over 20 m. It reads a sharp dogleg
on a hole straight for 400 m that kinks in the last 40 m, where the shot
affected is a pitch you aim at; and on our spines the vertices ARE the landing
zones while on a real way they are wherever the mapper clicked, so it measured
cartography as much as architecture. Replaced in `tools/golf/hole_metrics.py`
by the angle that carries the strategic content: the **drive dogleg**, the
angle AT THE TEE between the line to the drive station and the line to the
green, plus **lateral offset** over length as the shape companion. Both use
three points at fixed arc positions and are immune to vertex placement.
Consequence: round 2's dogleg band was calibrated against the old statistic,
so its target was wrong even though its outcome was not.

**New tooling.** `tools/golf/hole_metrics.py` holds ONE definition of every
quantity; `corpus/corpus_metrics.py` and `route_metrics.py` are thin runners
so the two sides cannot drift. `shot_profiles.joined()` now also yields the
hole's `ref`, so real nines can be assembled in play order; its own output is
byte-identical. Covered: length, dogleg, lateral offset, sinuosity, net rise,
total climb, roughness, above and below chord, approach grade, CROSS SLOPE at
the drive and the green, section relief, and per nine the length spread by par,
bearing change, walk, corridor gap, parallel pairs and tilt reversal.

**The five worth fixing, ranked** (report artifact 5c40f200):

1. **Par-4 lengths have collapsed onto one number.** Within a nine, real par 4s
   span 94 m; ours 32 m, under 14 m on a quarter of courses. Across the set
   real par 4s run 317-383 m through the middle half, ours 350-359. Gap −1.24
   interquartile units, the largest on the board. Self-inflicted: round 2 item
   9 gave par 4 a tent reward peaked at the real median to stop the other terms
   pinning lengths to the band edge, and it worked too well.
2. **Greens sit on ground too flat across the line.** Real green sites tilt
   over 3 % across play on 39 % of holes and over 6 % on 16 %; ours 24 % and
   4 %, and fluvial greens over 6 % are 0.1 %. Cross slope is a dimension
   nothing in the router has ever scored.
3. **Holes still play over rises, par 3s worst.** Real par 3s cross a rise over
   1.7 m on 0.5 % of holes; ours on 9.4 %. Par 4 and par 5 each about 9 points
   over. Above-chord sits +0.6 to +0.8 interquartile units on every par.
4. **Par 5s are never a decision.** 32 % of real par 5s are reachable from the
   drive station with a 230 m second; ours 7.7 %. With the layup finding
   (29 % of our par 5s leave a final leg under 80 m) our par 5s are three
   prescribed shots. The cause is arithmetic: a 190-250 m drive plus a 160-220 m
   second consumes 350-470 m of a 446-500 m target band.
5. **A dogleg tail real courses do not build.** Median dogleg now matches
   almost exactly, 5.0 vs 5.1 deg on par 4s, but real par 4s bend over 25 deg
   on 0.6 % of holes and ours on 7.1 %.

**Two corrections found along the way.**
- `route_audit.REAL["g_sur"]` has carried "6.1-6.8 m" as the real green
  surround relief, and we have reported ours at 4.9 as a shortfall since round
  1. Measured under the corpus's OWN definition (`features.py` ->
  `surround_relief`, 8 m grid, 25-90 m annulus) the real median is **4.93 m**
  and ours is **4.94 m**. We match. Round 1 item 5 spent a ladder closing a gap
  that was a mis-transcribed number and concluded the fluvial pool was limited
  when it was not. The real p75 is 7.9 against our 6.4, so the boldest sites
  are still under-used, but the median was never short. Every other hardcoded
  reference was re-verified and is exact (lengths 163/351/474, above-chord
  0.08/1.67/5.58); green upland "25 %" is really 21.6 %.
- Two proposed metrics measure nothing and are greyed out rather than reported
  as agreement: the par-5 second-shot dogleg, because the station sits 60 m
  from the green by construction, and par-3 bend, because most par-3 lines are
  two points on both sides.

**Where we already match:** hole length medians per par, drive dogleg at the
median, bend and sinuosity on par 4/5, cross slope at the DRIVE, bearing change
hole to hole, the share of consecutive holes within 45 degrees, tilt reversal
rate, approach grade on par 3/4, par-3 and par-5 length spread within a nine,
and the median gap to the neighbouring hole. Walks are 103 m against a real 69,
as expected since they are deliberately not optimised.

## 2026-09-16 — routing round 4: the five corpus gaps

Plan approved by the owner, who set two scope calls: lower the par-5 legal
floor to 420 m, and allow small NAMED regressions rather than dropping a fix.
A regression gate was fixed up front over everything rounds 1-3 won and
checked after every item. Baseline `r3_s1.jsonl`; final `r4_s4.jsonl` =
`rs.jsonl`. Viewers: round artifact 5005ee3a, gap report 5c40f200 refreshed.

Two diagnoses changed once measured and the work followed the measurement:
the green cross-slope gap is the GROUND, not the angle of attack (the ratio of
cross slope to section relief is 0.82 real against 0.78 ours, and the
omnidirectional surround already matches), and the over-a-rise problem is par
3 ONLY (real above-chord p90 by par 0.42 / 1.89 / 2.79 against ours 1.64 /
2.16 / 2.48 — par 5 is better than real).

| gap | before | after | real |
|---|---|---|---|
| par-4 spread within a nine | 32 m | 62 m | 94 m |
| par-4 interquartile range | 350-359 | 335-382 | 317-383 |
| par 5s reachable in two | 8 % | 21 % | 32 % |
| par 3s over a 1.7 m rise | 9.4 % | 6.2 % | 0.5 % |
| greens over a 3 % cross slope | 24 % | 29 % | 39 % |
| par-4 doglegs over 25 deg | 7.1 % | 2.4 % | 0.6 % |
| walk, green to next tee | 103 m | 85 m | 69 m |

Gate held: 250/250 routed, 0 play crossings, 1 green in play within 35 m, 0
tee boxes over 30 %, deep blind drives 3.7 per 100 (exactly round 3's ship),
mix 93 %, 0.47 s median, `gentle_tile_routes` passing.

**Item 1, the dogleg tail.** The drive is charged the angle AT THE TEE between
the line to the landing zone and the line to the green, free to the real p75
9.4 deg and saturating at the p95 17.4, with a rising charge beyond the p99.
A HARD TIER was built and REJECTED: going around a rise is a bend, so a tier
competed with round 3's visibility tier (par-4 doglegs over 25 deg / deep
blind drives per 100: tier at 25 gives 1.8 / 4.9 rejected; tier 35 + charge
3.1 / 4.5; charge alone at 4.0 3.4 / 4.2, shipped).

**Item 2, par 3s over a rise.** `par_chord_band` per par; only par 3 tightens,
par 4/5 keep the pooled band deliberately. `place_lz` is never called for a
par 3, so the rise tier and the carry veto inside it stay pooled. Ladder
(par-3 p90 / over 1.7 m / blind / mix): the real p90 and p99 at (0.4, 1.3)
0.92 / 4.2 % / 4.7 REJECTED / 86 %; (0.7, 2.5) 1.10 / 4.4 % / 4.5 on the
limit; (1.0, 3.5) 1.29 / 6.5 % / 4.0 / 91 % shipped, the only rung improving
both the rise and the blindness it trades against.

**Item 3, greens that tilt across the line.** `cross_at` reads `f.z8` either
side of the played line, NOT `Candidate::grad` (the pad plane, which would
push pads into the 8 % build cap). Shipped at route level 0.6 with the beam
copy first at 0 (27 % / blind 4.2), then RESTORED to 0.15 after item 4 freed
the budget (29 % / 3.7). At 0.6 in both it hits the real 39 % but blind goes
to 5.3 with a play crossing; at beam 0.3 five tee boxes return to > 30 %
ground. Greens over 6 % do not move at any rung because the reward saturates
at 8 %.

**Item 4, lengths.** `tent_t`'s par-4 tent is gone; every par now takes the
flat interquartile reward and `TWIN_SEP_4` 30 m spreads the five holes (round
2 had measured the flat reward WITHOUT a twin penalty and it pinned the median
to 374). `PAR_BAND_5` floors at 420, the spread override's short side reaches
it, and the par-5 second leg's radius follows what is LEFT to the green
instead of a fixed annulus that could not satisfy its own remainder band.
Rather than spend the gate, `VIS_TIER_M` tightened 1.5 -> 1.0, the rung round
3 measured but did not ship; that bought the blindness back to 3.7 and let
item 3's beam term return.

**Named shortfalls, all recorded rather than papered over:** par-4 spread
reaches 62 m of a real 94, because a real nine puts one par 4 near 290 and
another near 420 and our band plus the total budget cannot hold both; par 5s
under 450 m reach 13 % of a real 29, because only one par 5 a course gets the
short target and only when the other came out long; par 3s over a rise reach
6.2 % of a real 0.5, because dune ground has few par-3 sites with nothing on
the line; greens over a 3 % cross slope reach 29 % of a real 39, and over 6 %
do not move; the dogleg tail reaches 2.4 % of a real 0.6. Every one of these
is a terrain or budget limit that was measured, not a weight left untuned.

## 2026-09-16 — canopy round 1: where trees are, and how wide courses clear them

Owner's design call, agreed first: canopy is TWO objects. Where trees could
grow is a property of the land and belongs to terrain generation; where trees
are on a course is a construction decision and belongs after routing. This
round measures both so the generator matches rather than guesses. Report
artifact 7a4779a9.

**Two tile sets, because the golf tiles are contaminated for the first
question.** `tools/macro_campaign/out/tiles`, 572 archetype tiles of 3 km at
2 m sited on national forest, refuge and wilderness land, answer where trees
grow. The 651 course tiles answer how wide a course clears. New tools:
`corpus/fetch_canopy.py`, `canopy_metrics.py`, `corridor_width.py`.

**Two rasters, kept apart on purpose.** ESA WorldCover 10 m says whether a
cell is treed LAND; NLCD TCC 30 m says what fraction is under CANOPY. Their
ratio separates a savanna from a closed wood, which coverage alone cannot.
WorldCover is fetched by reprojecting the COG straight into each tile's own
projected grid, so canopy cells line up with elevation cells rather than
merely covering the same ground; 10 m divides every tile side (3000, 3500,
4000, 4500) where 30 m does not divide 3500.

| archetype | treed land | canopy | density | patches/km2 | lone/km2 |
|---|---|---|---|---|---|
| hill_country | 88 % | 69 % | 0.78 | 1 | 1 |
| piedmont | 84 % | 68 % | 0.81 | 2 | 1 |
| heathland | 85 % | 59 % | 0.69 | 1 | 0 |
| **sandhills_nc (fluvial anchor)** | **76 %** | **50 %** | **0.66** | **5** | **2** |
| river_valley | 69 % | 53 % | 0.76 | 3 | 1 |
| **sandhills (aeolian anchor)** | **7 %** | **2 %** | **0.33** | **2** | **1** |
| great_plains | 4 % | 0.4 % | 0.12 | 0 | 0 |

Carolina sandhills has the LOWEST density of any wooded archetype at 0.66,
the smallest median patch at 3 cells and the most patches per km2 at 5: the
savanna signature as a number, and it validates the owner's anchor choice.
Nebraska sandhills is effectively treeless, 67 % of its tiles under 2 % cover.

**The cleared corridor, all wooded courses.** Tree cover on the
centre line 5.3 % against 39 % in untouched woods beyond 120 m; cover
reaches half its far-field value at 30 m either side, so **the corridor is about
60 m across**. It is NOT constant: half-width 18 m at the tee, 30 m through
the landing zone, **35 m at the green** (4,813 holes, all 651 course tiles
fetched, 0 failures). Clearing a fixed rectangle would be wrong
by nearly a factor of two end to end. The profile is symmetric, which is the
check that the hole geometry and the raster are aligned.

**The placement model.** A logistic fit per archetype on the router's own
`Fields` predictors, so it can be evaluated on our tiles with no new terrain
machinery. Fluvial anchor: relative elevation −0.51, roughness +0.36,
tpi200 −0.22, so trees sit LOW and rough and the open ground is the high
smooth sand. Aeolian anchor: distance to water +1.12, northness +0.35,
elevation −0.31 against a 9 % base rate, which is the moisture story of a
dune field, grass in the interdunal meadows and what woody cover exists on
the shaded north flanks. **Brier skill runs 0.04 to 0.21.** Terrain explains a
real but modest part of where trees are; fire history, ownership and land use
explain the rest and none are available to a generator, so the model must
place trees with the right bias and patch structure rather than pretend to
predict stands.

**The lone sandhills tree, measured before being designed.** Isolated tree
patches in real Nebraska sandhills run 0.6 per km2 at the median tile,
quartiles 0.2 to 1.1, with only 11 % of tiles having none. Over a 1.3 km2 play
window that is 0.8 trees. So the owner's aesthetic is close to honest: zero to
two specimens a seed, about one on a typical seed, sits inside the measured
spread, and making them RARER than that would be the deliberate departure.
Placement follows the fitted coefficients: north-facing, rougher ground away
from water.

**Guards held.** Canopy is derived from the frozen height and water fields and
written as a sidecar, never baked into the tiles, so the 250-seed baseline and
every metric from four routing rounds stay comparable. Visibility stays bare
earth: our blindness numbers and the corpus's both exclude vegetation, so
trees must not enter that metric until the real reference includes them.

---

## 2026-09-16 — canopy round 2: the trees, grown on our own tiles

Round 1 fitted the model and stopped. This applies it to the 250 frozen seeds
and closes "where trees could grow". Clearing is the next round.
Viewer: **Our Own Canopy** (paired renders, acceptance table, corridor preview).

**Two sidecars per seed**, in `out/canopy250/`, 300×300 at 10 m, exactly 9.00
km2, the same geometry as a real tile so every per-km2 statistic compares with
no rescaling. `m_<seed>.canopy.cgrid` carries **WorldCover class codes** — 10
tree, 80 water, 30 other — which is the whole reason `corridor_width.py` reads
our output with no change at all, and `m_<seed>.tcc.cgrid` carries percent
canopy so the savanna signature survives.

**The rule.** `s = a·zscore(lowpassed model logit) + sqrt(1−a²)·fBm`, water set
to −inf after both z-scores, thresholded at the quantile that hits a per-seed
coverage drawn from the real per-tile distribution.

**Three findings that changed the design, each from a probe rather than an
argument.**

1. **One correlation length cannot do it.** Sweeping a single smoothed Gaussian
   across every width and mix never produced a largest stand above 13 % of the
   treed area against 40–50 % on real tiles, because clumps of one size give
   one patch per *n*. A two-Gaussian variant capped at 30 %. The field is an
   fBm stack over seven octaves, 10 m to 640 m; correlation length L means
   `sigma = L/(2·cell)`, and each octave is normalised before weighting.
2. **`np.quantile` interpolates and never returns the largest observation.**
   On a distribution whose mean lives in its tail that is the whole error: the
   aeolian cohort came out 6.66 % treed against a real 7.19, p90 21.5 against
   25.4. `method="inverted_cdf"` gives 7.16 and 25.4, and has the better
   property anyway — every coverage we generate was actually observed on real
   ground.
3. **A majority vote shatters our creeks.** They are 3–4 m wide on a 2 m grid,
   so no 10 m block is ever half water. Over the 45 river seeds a majority
   leaves a median **13 fragments whose longest run is 9 cells**, and erases
   the creek entirely on two; a block fraction of 0.2 leaves **one continuous
   channel, median 385 cells**. Distance-to-water needs a connected channel in
   the right place; the area cannot be right either way at 10 m.

**Calibration.** Structure only — coverage is right by construction. Scored on
the **cohort median** of six patch statistics against every real tile of the
archetype, because real tiles at the same coverage disagree enormously with
each other (two 80 %-treed Carolina tiles carry p90 patch sizes of 15.6 and
154 cells) and demanding tile-by-tile agreement scores us on noise nobody can
reproduce. The median is also what the acceptance table compares, so the search
optimises the number it is judged on. Fitted on half our seeds of each cohort,
chosen on the other half: an earlier 16-tile search returned a setting scoring
0.138 on its own tiles and **0.248 on tiles it had not seen**, while the
runner-up scored 0.165 and 0.168. Final errors, fit → holdout: sandhills
0.161 → 0.066, sandhills_nc 0.221 → 0.236, sandhills_river 0.204 → 0.108.

**Fit versus target are allowed to differ.** The 29 aeolian river seeds are
PLACED by the `sandhills` coefficients and SCORED against the `sandhills_river`
tiles. The `sandhills_river` fit has the opposite sign on distance to water
(−0.61 against +1.12) on ~2,850 positive cells across 18 tiles, and a
"trees hug the water" rule applied to a 4 m synthetic creek paints a one-cell
green ribbon down the tile. Coverage and structure are the parts of that
corpus that are well measured, and those are the parts used.

**Result: 32 of 33 acceptance rows inside tolerance.**

| | sandhills (87) | sandhills_nc (134) | sandhills_river (29) |
|---|---|---|---|
| treed % ours / real | 7.16 / 7.19 | 75.69 / 75.66 | 4.19 / 3.94 |
| canopy % | 2.34 / 2.36 | 50.25 / 50.23 | 1.17 / 1.10 |
| density | 0.328 / 0.328 | 0.664 / 0.664 | 0.279 / 0.279 |
| patches/km2 | 1.78 / 1.78 | 4.00 / 5.33 | 1.44 / 1.89 |
| lone/km2 | 0.44 / 0.61 | 2.22 / 2.22 | 0.33 / 0.22 |
| largest patch % | 53.9 / 50.0 | 99.1 / 98.5 | 47.0 / 40.1 |
| edge density | 0.452 / 0.505 | **0.039 / 0.076** | 0.357 / 0.529 |

**Named shortfall: Carolina wood edges are too clean,** 0.51× real. The dial
exists (`grain`, extra weight on the 10 m octave) and turning it to 0.30 drives
the miss to nothing — while multiplying the patch count fourfold and the
lone-tree rate eightfold, taking the cohort error from 0.22 to 1.07. At 76 %
cover the real edge density is largely classification speckle in a 10 m
satellite product rather than the geometry of a wood. Left at zero in all three
cohorts and recorded.

**The lone tree, delivered by measurement rather than by a rule.** Nothing
places specimens. Counted inside the routed play window: **66 % of aeolian
seeds carry none, 16 % carry one or two, 17 % carry three or more**, mean 1.33
over a 1.38 km2 window = 0.96 per km2, against a real dune-country median of
0.61 with quartiles 0.2–1.1. An explicit specimen pass was designed and
dropped: it would have stacked trees on a process already delivering them at
the measured rate. 7 of 250 seeds are genuinely treeless and 66 of 116 aeolian
seeds sit under 1 % cover, which is the archetype, not a bug.

**The corridor that is not cut yet.** Our holes were routed on bare earth, so
the centre line carries the same cover as the country around it: centre/far
ratio **0.90 aeolian and 0.99 fluvial, against 0.21 on real courses**. Mean
cover inside the tapered corridor is 6 % aeolian and **76 % fluvial (median
98 %)**. That is the size of the clearing round, and
`corridor_width.profile_for_hole` measures it on our sidecar unchanged.

**Guards held, mechanically checked.** The 250 frozen heightfields hash
identically before and after (`cb62563b…`). A second run of the generator is
byte-identical across all 500 files. No tree on water, coverage exact to
2/90000, canopy percent positive exactly where trees are and never above 100,
on all 250. Visibility stays bare earth. Python this round; a Rust port would
need `n5/sand/canopy/v1` in `rng.rs` REGISTRY plus the row in
`docs/sandhills/README.md` §7, and a cross-language agreement test on
`predictors`, none of which is cheaper now than later.

---

## 2026-09-17 — canopy round 3: cut the corridors, finalized canopy

Round 2 grew the natural canopy and left the fluvial seeds correct and
unplayable: the median fluvial hole ran a corridor 98 % treed. This removes the
trees a course removes and leaves the ones a course leaves.
Output `out/canopy250_final/`; `out/canopy250/` stays as the natural layer.
Viewer: **Cutting The Corridors**.

**Three measurement corrections, each found by a probe that went wrong first.**

1. **Normalise by untouched ground, in the nearest-hole frame.** The obvious
   method divides each hole by its own cover 120–160 m out along the normals.
   On a real course that band is the next fairway: it holds **0.64** of truly
   untouched cover, and on our sparser routes **0.80**, so the two sides are
   normalised by differently contaminated denominators and every ratio is
   inflated 1.25–1.6×. The nearest-hole frame has no such band.
2. **Clearing recovers fully by 80 m and there is no property-wide thinning.**
   Against ground >500 m from every hole (60.9 % treed), cover runs 0.083 /
   0.164 / 0.325 / 0.548 / 0.760 / 0.887 / 0.962 over the first 80 m and then
   sits within 3 % of untouched all the way to 500 m. A golf course is a set of
   corridors, not an opened-up estate.
3. **Par 3s are a different animal.** The middle of a par 3 is carry, not a
   landing zone: keep 0.133 on its centre line against 0.034 for a par 4 and
   0.020 for a par 5. Every table is keyed by par.

**Measured, from 3,851 holes on 315 wooded courses** (`clearing_profile.py`,
frozen into `clearing_model.json`). Keep rate against distance to the nearest
hole line, over untouched ground:

| zone | 0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 |
|---|---|---|---|---|---|---|---|---|
| tee | 0.190 | 0.308 | 0.494 | 0.706 | 0.849 | 0.915 | 0.935 | 0.943 |
| landing | 0.042 | 0.090 | 0.217 | 0.424 | 0.657 | 0.830 | 0.912 | 0.938 |
| green | 0.020 | 0.058 | 0.191 | 0.434 | 0.700 | 0.867 | 0.935 | 0.962 |

Patches in the corridor 2.53/hole; heavier flank 82 %; all-on-one-side 34 %; no
corridor tree 20 %. Zone dependence is real and partial: rank correlations 0.64
/ 0.63 / 0.44, and 22 % of holes bare in all three zones against 39 % if the
zones moved together and 7 % if independent.

**No strategic placement.** On 720 bent holes, trees left in play show **no
preference for the inside of the dogleg — 50.9 % against 50**. They are
residual, not sited. The owner was offered a strategic pass and declined it.

**The generator.** `p = shape(par, zone, distance)`; proportional thinning, so
the local tree rate cancels and never has to be estimated. Survivors are chosen
by **ranking a fractal field inside each pool and comparing the rank to the
cell's own probability** — exactly uniform without assuming a distribution, and
monotone so it stays clumped. Three earlier constructions and why they failed,
all measured:
- Gaussian copula (`field − ndtri(p)` at one threshold): kept 0.083 on the
  landing line where the profile said 0.041 and 0.343 at the edge against 0.425.
  The 640 m octave has ~2 independent samples per tile, so `zscore`'s deviation
  is not trustworthy that far into the tail.
- Solving in probability bands: fixes the marginal, shreds the clumping —
  patches 2.54 → 3.18 per hole, one-sided holes 39 % → 23 %.
- Odds multiplier: concave in the multiplier, so Jensen pulls every level down;
  cost 0.15 at the corridor edge.

Per-hole variety is a plain multiplier on the profile, drawn by correlated
stratified inverse-CDF (Gaussian copula on the measured rank correlations, then
ranked within the cohort so each zone's marginal stays exact).

**Result: 24 of 30 gated rows inside tolerance** (1,314 holes on 146 tiles
≥25 % treed; 104 bare tiles excluded and reported separately, as the corpus
excluded its 360 bare courses). All retention rows, all six patch rows and both
asymmetry rows pass. Landing keeps 0.042 / 0.206 / 0.384 against 0.042 / 0.217 /
0.424 at 0 / 20 / 40 m.

**The owner's two features, counted rather than built.** A specimen within 15 m
of the landing-zone line on **7 % of holes**, and one 20–40 m from the green on
**7 %**. Worth stating plainly: the real wooded corpus scores near zero on this
exact measure, not because such trees do not exist but because at 60 %
surrounding cover anything left in a corridor stays joined to the tree line.
Ours stand clear more often. That is the requested feature and a small
departure upward, named rather than claimed as a match.

**Tees opened on purpose.** Real tees sit in the trees — 0.190 kept on the tee
line against 0.020 at a green, with 46 % of tees essentially bare and a quarter
tight. The owner asked to keep that variety and err open, so tee retention alone
is drawn at a warped quantile (`TEE_OPEN_EXP = 1.40`), giving 57 % bare against
a measured 46 %. Those rows are reported as the departure, against both the
measured value and what the warp predicts.

**Named shortfalls.** The multiplier's tail is capped at 2× its zone mean: the
measured retention reaches 9× its mean on real holes whose corridor carried more
tree than the ground around it, and our homogeneous canopy cannot supply that —
uncapped it saturates and costs 0.1 at the corridor edge. Same cause leaves
28 % of our holes with a bare corridor against 20 %.

**Guards held, mechanically.** `out/final250_v2` and `out/canopy250` hash
identically before and after. A second run is byte-identical across all 500
files. Clearing never plants a tree, never touches water, only ever writes class
30, and never changes a cell outside a corridor. Canopy density 0.660 after
clearing against 0.664 natural, the right direction (real course tiles 0.611).
The 104 sub-25 % tiles lose 0.04 pp of cover on average and 78 are untouched.
