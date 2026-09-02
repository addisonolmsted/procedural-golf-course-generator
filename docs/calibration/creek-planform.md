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
