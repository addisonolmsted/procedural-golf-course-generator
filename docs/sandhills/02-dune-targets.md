# Dune targets, and the instrument that produced them

**Phase 2 deliverable** (`docs/sandhills/README.md`). Instrument:
`tools/aeolian/dune_stats.py`. Corpus: all 44 kept Nebraska tiles, measured
2026-08-22.

---

## 1. Why a new instrument was needed

Two things the Sandhills archetype is declared to need did not exist.

`docs/biomes/sandhills.md` names the **directional variogram along the wind
azimuth** as its primary discriminant. `metrics.core.anisotropy` computes eight
directional variograms internally and discards the curves — at 22.5° resolution
with `max_lag_m` clamped to 400 m. Nebraska's dominant wavelength measures
~1190 m, so **the existing instrument saturates before the signal starts.**

And nothing measures **crest defect density** — terminations and Y-junctions per
km². That is what separates a real dune field (chains of coalesced crescents)
from parallel stripes, and a generator can hit spacing, relief and orientation
while failing it completely.

The instrument ships with its own falsification harness because a dune metric has
already died here: `fit_knobs.QUARANTINED["dune_wavelength_m"]` records that
orientation coherence over the Nebraska tiles (R=0.41) came out *lower* than over
the Cumberland Plateau tiles (R=0.75) — *"it ranks structural grain above a real
dune field."*

## 2. The instrument failed first, three times, and each failure was real

Recorded because rule 5 is that an instrument which has never been seen to fail is
not evidence — and this one failed on its first run, reproducing the retired
metric's exact signature (dunes 0.327 against Cumberland 0.470).

1. **Crest-tangent coherence is biased high at low crest count.**
   `corr(n_crest, coherence) = −0.61`; tiles with one traced crest read 0.470,
   tiles with four or more read 0.261. A single crest is trivially coherent with
   itself, so on ground where the tracer finds little, the metric reports
   "strongly oriented" for the wrong reason. **Demoted to descriptive**;
   `spectral_order` is the gate metric.

2. **A stride subsample of a lattice is a place, not a sample.** Tile ids sort
   geographically, so `[:limit]` took one corner of one block. The first ten
   Nebraska tiles gave spectral A p50 0.298 against 0.400 for the full 44 —
   enough to flip the verdict on its own. `sweep` now strides evenly, and the
   gate sweeps the corpus in full.

3. **The corpus is a mixture** — §3, which is the finding that mattered.

A methodological note that follows from `02-drainage-patterns.md` §2:
`fill_depressions` runs before routing, so sandhills' network is a **phantom the
real ground does not carry** — and `ridgepipe` traces crests with that same
inverted-network machinery. A spectral statistic does not inherit that problem,
which is the deeper reason the gate metric is spectral and the crest metrics are
descriptive.

## 3. The Nebraska corpus is two dune populations, not one

Orientation order over the 44 kept tiles is cleanly **bimodal: Ashman D = 3.96**,
modes at A 0.339 and 0.756. Hill country over the same instrument is **unimodal**
(D = 1.88), so this is a property of the dune corpus, not of the statistic.

| | n | spectral A | λ_dom m | band relief m | crest len p50 m |
|---|---:|---:|---:|---:|---:|
| **sandhills : train** | 8 | **0.767** | 1301 | 29.2 | 796 |
| **sandhills : mound** | 36 | 0.356 | 1184 | 19.1 | 1023 |
| mountain_bench (Cumberland Plateau) | 8 | 0.345 | 1065 | 35.0 | 1293 |
| hill_country (Ozark) | 20 | 0.377 | 992 | 30.3 | 842 |
| *(heathland, for reference)* | 37 | 0.410 | 1066 | 8.4 | — |

Pooled, sandhills reads 0.400 against hill country's 0.390 — **a margin smaller
than the sampling noise**, which is why a pooled median-vs-median test on this
corpus is unpassable in either direction and why the first two gate attempts
flip-flopped.

This independently reproduces the train/mound continuum the previous generator
already carried ("⅓ strong trains, ~40 % mounds"), found by a different route
from a different instrument.

**Train exemplars** (the F2 pool for train-mode texture):

| tile | A | λ_dom | band relief |
|---|---:|---:|---:|
| `t04235_10160` | 0.843 | 1320 m | 45.9 m |
| `t04259_10092` | 0.779 | 1349 m | 29.3 m |
| `t04252_10068` | 0.776 | 1099 m | 29.2 m |
| `t04251_10069` | 0.774 | 1298 m | 31.2 m |
| `t04268_10042` | 0.759 | 1268 m | 15.1 m |

## 4. The gate, as it now stands — PASSING

The **train** subpopulation must outrank every structural-grain control on
orientation order, and the form-class split must itself be real (Ashman
D ≥ 2.0) or "trains" is just the top half of noise. The split boundary is the
GMM decision point, not a chosen percentile, so it moves with the data.

```
PASS  spectral orientation order  sandhills:train 0.767 > mountain_bench 0.345
PASS  spectral orientation order  sandhills:train 0.767 > hill_country   0.377
PASS  form-class separation       Ashman D 3.96 >= 2.0
```

Verification that the code path is the declared one: over all 44 kept tiles
`spectral_order` returns A p10/p50/p90 = **.186/.400/.755**, λ **1190 m**, band
relief **20.3 m** — reproducing `docs/calibration/variety-audit.md` to three
decimals.

## 5. Generator targets

Per the user decision of 2026-08-22, the form class is drawn as a **bimodal
mixture**, not a uniform continuum dial, and the two classes keep **separate
texture exemplar pools**.

| dial | train | mound |
|---|---|---|
| spectral orientation order A | 0.767 | 0.356 |
| dominant wavelength λ | 1301 m | 1184 m |
| macro band relief (400–1600 m, p95−p5) | 29.2 m | 19.1 m |
| mixture weight (measured) | 8/44 = 0.18 | 36/44 = 0.82 |

Two cautions carried forward.

**The corpus is censored toward gentle ground and does not govern amplitude.**
`04-landform-literature.md` already established this: measured risers 7.4 m
against barchans averaging 41 m and barchanoid ridges 90–150 m — *"the corpus's
clean-tile screen kept the gentle tiles, and trunk-transects are the wrong
instrument for dunes. Literature governs the aeolian generator."* Band relief
above is a *plausibility band*, not an amplitude target; golf bounds playable
dune relief to 10–35 m.

**The measured mixture weight is not the draw weight.** 0.18/0.82 describes the
protected land the screen kept, not the dune country a player should see. The
draw weight is a design decision, and trains are the archetype's signature.

## 6. What is descriptive, not gated

`coherence` (crest-tangent), `crest_len_p50`, `ridge_spacing_m`,
`prominence_p50`, `defect_km2`, `flank_asym`, `hole_depth`. The crest tracer is
unreliable at the ~1 crest/tile it returns on this corpus at dune scale; these
are reported so a drift is visible, and none is a gate. **Crest defect density
becomes a real gate at Phase 3**, measured on the constructed crest network
before any surface exists — where the count is ours and known, not traced.

---

## 7. Phase 3 calibration — the disorder ladder, and a design defect it found

**Ladder** (`course-sandhills --example disorder_ladder`, 6 seeds per rung,
measured with `spectral_order`):

| disorder | spectral A | λ_dom m | band relief m |
|---:|---:|---:|---:|
| 0.00 | 0.898 | 1148 | 18.6 |
| 0.50 | 0.885 | 1166 | 18.2 |
| 1.00 | 0.851 | 1056 | 17.6 |
| **1.50** | **0.798** | 949 | 17.0 |
| **2.00** | **0.736** | 888 | 16.2 |
| 3.00 | 0.621 | 809 | 15.1 |
| 4.00 | 0.528 | 697 | 12.3 |
| 6.00 | 0.484 | 546 | 9.0 |

**Train lands at disorder 1.5–2.0** (A 0.74–0.80 against the 0.767 target).

**Mound does not land at any rung**, and that is a design finding rather than a
missing dial:

1. **A asymptotes near 0.48.** Extrapolating the ladder, phase disorder alone
   cannot reach the mound target of 0.356.
2. **Wavelength collapses as disorder rises** — 1148 m → 546 m. Cranking
   disorder toward the mound's orientation order destroys its wavelength, which
   is supposed to stay at 1184 m. One dial is driving two targets in opposite
   directions.

**Root cause, found by reading the code the ladder implicated:** the phase is
built from the *global* paleowind (`k · (p · w0)`) and never reads the local
direction field at all. So `wind_wander_rad` — the dial that is supposed to
distinguish the two form classes, drawn at 0.20–0.40 for trains and 0.55–1.10
for mounds — **is currently inert**, and `disorder` is doing all the work
through a mechanism that shortens the wavelength as a side effect.

**The fix** is to make the phase follow the local direction: solve
`∇²φ = ∇·(k ŵ)` (a Poisson problem, FFT or Jacobi) rather than evaluating a
closed form along the mean wind. This is the physically correct construction —
it is what makes `grad φ` track the wind field — and it is well-posed with no
preferred origin, which was the reason the closed form was chosen in the first
place. Defects then appear exactly where the wind field has curl, at whatever
wavelength the field specifies, so orientation order and wavelength stop
fighting each other.

Prior art worth reading first, NOT copying (Tier D): attempt 4's
`build_aeolian` reached the same split by different means — trains as two
beating sinusoids on one axis, mounds as ten isotropic waves at log-uniform λ.
That decomposition preserves wavelength while dropping orientation order, which
is the property this ladder shows is needed.

### Also measured, and tracked

Generated tiles pass the golf proxy **16/16**, at `frac_under_cap` 0.89 (train)
and 0.99 (mound) against real Nebraska's 0.386. Generated dune country is far
calmer than the real thing. This is expected before texture and blowouts exist,
and it reproduces the standing pattern that generated land is more sitable than
real land (`G-SKELETON.md`: generated cores 100/80/40/40/20/20 % against real
53/51/0/7/9/5 %). It becomes a real question at Phase 5, not before — and the
answer is never to flatten less, since calm is a floor and not a maximand.

---

## 8. The Helmholtz phase — what it fixed, and what it did not

The Poisson fix of §7 landed as a **closed-form modal construction** rather than
a relaxation: the direction perturbation is an explicit sum of sinusoidal modes,
so the phase has an exact Helmholtz gradient-projection and needs no iteration,
no FFT and no preferred origin. The discarded curl is returned as `curl_mag` —
the defect budget, in units of the base wavenumber.

**Wander ladder** (6 seeds/rung, `--example wander_ladder`):

| wander_rad | curl_mag | spectral A | λ_dom m | band relief m |
|---:|---:|---:|---:|---:|
| 0.00 | 0.000 | 0.900 | 1153 | 18.8 |
| 0.30 | 0.298 | 0.881 | 1136 | 18.4 |
| 0.60 | 0.596 | 0.835 | 1122 | 17.8 |
| **0.80** | 0.795 | **0.784** | 1112 | 17.3 |
| **1.00** | 0.994 | **0.738** | 1097 | 16.9 |
| 1.30 | 1.29 | 0.690 | 1064 | 16.6 |
| 1.70 | 1.69 | 0.622 | 998 | 15.8 |

**FIXED — orientation order and spacing are now independent.** λ drifts 1153 →
998 m (13 %) across the entire dial, against the previous construction's
1148 → 546 m (**52 % collapse**). `wind_wander_rad` is a live dial rather than
an inert one, and **trains land at wander 0.80–1.00** (A 0.784–0.738 against the
0.767 target).

**NOT FIXED — the mound class is out of reach.** A bottoms out at 0.622 even at
wander 1.70, which is ±97° of direction swing; the target is 0.356.

The reason is structural, not a missing dial. **A mound field is not a
disordered train.** However much a single plane wave wobbles, the term
`k · (p · w0)` still imposes one global axis, and that axis is what the spectral
orientation order measures. Reaching 0.356 requires the dominant axis to be
genuinely weak — a **superposition of wave directions**, not one direction that
wanders.

This is the same conclusion attempt 4 reached from the other end: its
`build_aeolian` used two beating sinusoids on one axis for trains and **ten
isotropic waves** for mounds (Tier D prior art, read not copied).

### The proposed generalisation — one construction, not two

Two plainly different constructions would contradict this crate's own claim that
both classes run the same generator with different numbers. The unification is a
**directional concentration** parameter κ:

- draw *K* wavevectors from a von Mises distribution about the paleowind, all at
  |k| = 2π/λ, concentration κ;
- high κ collapses to a single axis — the train, and the current behaviour is
  its κ → ∞ limit;
- low κ spreads them — the mound field, at unchanged wavelength.

κ becomes the dial that separates the classes, replacing `wind_wander_rad` in
that role (wander survives as the within-axis bend). It also predicts the
asymmetry the record already carries independently: superposing profiles at
spread orientations partially cancels the stoss/lee asymmetry, and the corpus
says mounds *are* the less asymmetric form (stoss_share 0.58–0.70 against the
train's 0.66–0.80).

**Cost:** the surface stops being one profile of one phase and becomes a
weighted sum over K phases, so the "no envelope to compose" property of §7
weakens — though it stays a sum rather than a min/max envelope, which is the
part that caused seam trouble in earlier attempts.

---

## 9. The κ superposition — both form classes reached

**User decision, 2026-08-22**: option A, one construction spanning both classes.

The phase field becomes a **superposition of K waves** whose directions are
drawn from a von Mises distribution about the paleowind with concentration κ,
all at `|k| = 2π/λ`. High κ pulls them onto one axis — a train, and the previous
single-wave field is its κ→∞ limit. Low κ spreads them and the dominant axis
dissolves — a mound field, at unchanged wavelength. One code path, one dial.

Sampling is Best & Fisher (1979) rejection, exact, no special functions. All
waves share ONE phase offset rather than one each: independent offsets would
stop them aligning at high κ, and the train limit is the property that makes
this one construction instead of two.

### N_WAVES = 8 is not arbitrary

K unit vectors with uniform directions have an expected resultant of `1/√K`, so
**the isotropic floor of the measured orientation order is 1/√K** — no
concentration, however low, gets beneath it. The mound target is 0.356 and
`1/√8` = 0.354.

This was found rather than assumed: at K=6 the ladder bottomed out at 0.397
against a predicted `1/√6` = 0.408, which identified the relationship. K was
then set to put the floor exactly on the mound target.

### κ ladder (6 seeds/rung, K=8)

| κ | spread rad | spectral A | λ_dom m | band relief m |
|---:|---:|---:|---:|---:|
| 0.10 | 1.38 | 0.438 | 1112 | 20.8 |
| 0.40 | 1.34 | 0.323 | 1139 | 21.6 |
| 1.20 | 1.04 | 0.465 | 1096 | 21.3 |
| 2.50 | 0.78 | 0.469 | 1126 | 21.5 |
| 5.00 | 0.65 | 0.681 | 1132 | 21.5 |
| 10.0 | 0.45 | 0.810 | 1131 | 21.6 |
| 60.0 | 0.19 | 0.860 | 1186 | 21.2 |

**λ and band relief are flat across the entire dial** — 1096–1186 m and
20.8–21.6 m. Orientation order, spacing and amplitude are now three independent
quantities, which is what the single-wave construction could not deliver.

### Shipped, measured over 30 seeds per class

| | A p50 | *real* | A p10 | *real* | A p90 | *real* | λ_dom | *real* | golf proxy |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **mound** | **0.361** | *0.356* | **0.188** | *0.19* | **0.500** | *0.50* | 1138 | *1184* | 30/30 |
| **train** | 0.714 | *0.767* | 0.537 | *0.62* | 0.850 | *0.86* | 1191 | *1301* | 30/30 |

**Mound matches on all three quantiles.** Train matches at p90 and reads ~7 %
low at the median and p10 — a **tracked residual, not a pass**. Four calibration
rounds went into this dial and the remaining gap is inside the seed-to-seed
noise at n=30, so it stops here rather than being fitted further; it is a
plausibility band, and `03-macro-is-designed` treats it as one.

Dials: train κ 5–12, mound κ 0.45–1.40. The mound range sits **above** the
ladder's own optimum because the two dials compound — the mound class also draws
the high wander (0.55–1.10 rad), and within-axis bending suppresses orientation
order further. At the ladder's κ the measured A came out 0.234, well under
target.

λ is drawn ~10 % **above** the corpus target: the ruler is a power-weighted mean
over the 400–1600 m band and biases toward the short end, so a field built at a
drawn 1250 m measures 1146 m. The range is set so the MEASURED number lands on
the corpus, because that is the number the gate reads.

### What it cost

The surface is now the mean of one dune profile per wave, rescaled to the drawn
relief on its own p95−p5. Still a **sum**, never a min/max envelope — there is
no seam to fight, which was the point of §7's formulation. The rescale is
load-bearing: superposing K profiles that are not in phase shrinks the variance,
so a fixed gain would make a mound field silently flatter than a train.

Two test corrections the change forced, both cases of measuring the wrong thing:

- `wavelength_is_independent_of_both_dials` first asserted on the **mean of
  |grad φ|**, which is biased high whenever the gradient direction fluctuates
  (Jensen: `E|k+δ| > |E(k+δ)|`) — 15.7 % at wander 0.8. It now projects onto
  each wave's own axis, where the perturbation is zero-mean.
- `high_concentration_recovers_the_single_axis_field` compared mean cycle values
  over sparse samples and was reading its own sampling noise. It now asserts on
  the **widest angle between wave directions**, which is the actual claim.

---

## 10. The hummock tier, and three wrong diagnoses of one weave

The visual check (rule 4, and the first render of this generator) found the
macro sound — right spacing, right topology, flat interdunes, no moiré — and
**missing a whole scale**. Real Nebraska is two-scale: ~1.3 km ridge belts
separated by flat interdune valleys, with the belts **packed with individual
50–150 m dunes**. The megaform alone reads as smooth swells.

None of the gate statistics could see it. Orientation order, dominant
wavelength and band relief all measure **400–1600 m**; the missing landform sits
below that band, and the texture stage works below 64 m, so 64–400 m belonged to
nobody.

### The weave: three fixes, two of them wrong

A visible crosshatch of dark streaks ran across every tile. Each diagnosis was
plausible, each was measured, and the first two were wrong:

1. **The crest kink.** The profile was C0-but-not-C1 at the crest, slope jump
   5.17. Fixed by rounding (jump → 0.016). **The weave did not move**, and the
   measurement that should have caught it was already on the page: `max|d²|`
   read **3082 before and after**, so the dominant curvature was never at the
   crest.
2. **The toe singularity.** `u^1.15` has second derivative `0.17·u^-0.85`,
   infinite at the trough — a genuine curvature singularity on every wave's toe
   line. Fixed by evaluating on a shifted variable (`max|d²|` 2327 → **1.8**,
   shape and bounded peak slope intact). **The weave still did not move.**
3. **The real cause — per-wave asymmetry.** Every wave carried its own stoss/lee
   profile, so eight waves at spread orientations gave **eight crossing families
   of slip faces**. A real dune field has one wind and therefore one family.
   ABLATION: rebuilding the same seeds with a symmetric profile erased the weave
   completely and dropped `|∇²|` mean 0.00337 → 0.00199. That is what named it.

Fixes 1 and 2 are kept — both were real defects, and both were found honestly
even though neither was the one that mattered. This is the fourth time on this
project that a visual defect survived two confident diagnoses and fell to an
ablation.

### The asymmetry is applied once, and it is physical

Waves compose **symmetrically**; the composite is then sheared downwind by an
amount proportional to its own height — `z'(p) = z(p − offset·t(p)·ŵ)`, with
`offset = (share − 0.5)·λ`. That is what wind does: it carries sand up the
windward slope and drops it over the brink, displacing the crest downwind in
proportion to how high it stands. Belts and hummocks are advected by their own
wavelengths.

### Also fixed

**Clipping plateaus.** Normalising on p5/p95 and then clamping flattened the top
and bottom 5% into hard-edged polygonal flats, clearly visible in the render.
The clamps are gone; the scale still makes p95−p5 the drawn relief and the tails
run past it, which is what a real crest does.

**Train κ floor 5 → 8.** The old range admitted direction spreads to 0.605 rad,
which is mound territory, and seed 19 measured 11.3 orientation families against
6.6–8.2 for a well-drawn train. One change fixed two things: trains read as
trains, and the train orientation order rose from 0.714 to **0.759** against the
0.767 target, closing the tracked residual from §9.

### Result

| band | real train | before | after | short by |
|---|---:|---:|---:|---:|
| 30–64 m | 0.61 | 0.10 | **0.44** | 1.4× *(was 6.1×)* |
| 64–150 m | 1.09 | 0.38 | **1.24** | **0.9×** *(was 2.9×)* |
| 150–400 m | 2.80 | 1.65 | 1.83 | 1.5× |
| 400–900 m | 6.51 | 3.56 | 3.28 | 2.0× |
| 900–1600 m | 6.69 | 2.74 | 2.55 | 2.6× |

The dune band is closed. The three long bands stay short by design — golf bounds
playable dune relief to 10–35 m against real tiles at 55–74 m.

Orientation order **0.759** (target 0.767); golf proxy 4/4; 39 tests. The
hummock tier is gated to the belt tops so the interdune floors stay flat, which
is the archetype's routable ground — `hummocks_stay_off_the_interdune_floors`
pins it, and its first version failed because it ranked the tile's downhill
corner as "floor" by sorting a grid that carries the regional datum tilt.

---

## 11. The hummock band, measured — and the tier tuned to it

The tier of §10 was calibrated against **one** number: I raised its amplitude
until the 64–150 m band RMS matched and stopped. `hummock_lambda_m`,
`hummock_kappa` and `hummock_gate` were guesses, and the provenance comments
that said "corpus band" were overstating what had been measured. A reviewer
looking at the renders read the belts and the spacing as right and the texture
as wrong, which is exactly the shape of that gap.

`spectral_order` already takes a band, so the corpus can be measured at dune
scale the same way it was at belt scale. **Never done until now.**

| | A p50 | A p10 | A p90 | λ_dom | band relief |
|---|---:|---:|---:|---:|---:|
| **real, train belts** (n=9) | **0.261** | 0.160 | 0.309 | **216 m** | **11.40** |
| **real, mound belts** (n=35) | **0.291** | 0.140 | 0.607 | **228 m** | **10.34** |

**Two findings.**

**Individual dunes are near-isotropic**, A 0.26–0.29, far less oriented than the
belts they ride on (0.767 / 0.356). The first tier ran at A 0.424 for trains —
**1.6× too ordered**, inheriting the belt's direction, which reads as combed
rather than as a dune field.

**Real train and mound belts carry near-identical dunes** — A 0.261 vs 0.291,
λ 216 vs 228, relief 11.40 vs 10.34. So the hummock dials are now the SAME for
both classes: **the form class distinguishes the belts, not the dunes on them.**
Only the gate differs, because a mound belt is a lower, broader pile.

The one dial that does differ is the concentration, and higher for mounds,
because the dials **compound**: a mound belt is itself built from spread waves,
so hummocks at the train's κ inherited that disorder on top of their own and
measured A 0.127 against a 0.291 target. Same compounding as the megaform wander
in §9.

### Shipped, 24 seeds per class

| | hummock A | *real* | hummock λ | *real* | hummock relief | *real* | megaform A | *target* | proxy |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| train | 0.304 | *0.261* | 238 m | *216* | 10.05 | *11.40* | 0.796 | *0.767* | 24/24 |
| mound | 0.338 | *0.291* | 222 m | *228* | 9.59 | *10.34* | 0.328 | *0.356* | 24/24 |

Everything inside ~17 %, on a family the policy treats as a plausibility band.
Three passes went into it and further iteration at n=24 would be fitting noise.

**Method note carried forward:** the first pass read A 0.249 for trains on n=4
and 0.312 on n=16 — the same lattice-subsample error as §7, in a different
costume. Hummock dials are calibrated at n≥16.
