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
