# Macro terrain is designed for golf. Texture is fitted to the corpus.

**User decision, 2026-08-20.** The macro landform is a **design** target, not a
realism target. Where the two conflict — side-asymmetric terracing in a river
valley, a valley floor widened in hill country so there is something to route on
— **golf wins**. The corpus keeps its authority over **texture**.

This document records the decision, the evidence that it is the right one, and
exactly which measurements change hands as a result.

---

## 1. The evidence: corpus-matched macro terrain is mostly not routable

The golfability proxy scores a surface against thresholds measured from **64
real golf-course grids** — and those grids are the same geometry as the corpus
tiles, 1500 × 1500 at 2 m, 900 ha, so the comparison is like for like.

Run over all 203 clean corpus tiles (`tools/golf_proxy/proxy_corpus.py`, raw
output `proxy_corpus.txt`). Floors: `cap ≥ 0.409`, `steep ≥ 0.713`,
`contig ≥ 124 ha`, `relief 7.0–81.9 m`.

| biome | n | frac_under_cap | frac_under_steep | largest_contig ha | relief m | **passes all** |
|---|---|---|---|---|---|---|
| piedmont | 27 | 0.327 | 0.627 | 81.5 | 47.7 | **30 %** |
| great_plains | 36 | 0.707 | 0.916 | 462.6 | 44.5 | 83 % |
| river_valley | 32 | 0.957 | 0.984 | 667.5 | **4.6** | **31 %** |
| hill_country | 27 | **0.169** | **0.368** | **29.7** | 74.4 | **0 %** |
| heathland | 37 | 0.815 | 0.936 | 595.3 | 18.3 | 92 % |
| sandhills | 44 | 0.386 | 0.672 | 144.4 | 37.7 | 23 % |

**Not one of 27 real hill-country tiles is routable by the proxy.** Its largest
contiguous sub-cap component is 29.7 ha against a 124 ha floor — and since siting
selects a 600 m window, which is 36 ha in total, 29.7 ha of contiguous calm in
the whole 9 km² tile means **the window cannot be filled from it**. Piedmont
passes at 30 %, sandhills at 23 %.

And river valley fails from the opposite direction: it is far *above* every
slope floor and fails on **relief 4.6 m against a 7.0 m band floor**. It is not
too steep, it is too dull.

So the archetypes split three ways, and only one of the three wants to be left
alone:

| | problem | what golf needs |
|---|---|---|
| hill_country, piedmont, sandhills | not enough contiguous playable ground | **add** routable area without flattening the character |
| river_valley | not enough relief | **add** vertical interest — which is the terracing |
| great_plains, heathland | neither | leave the macro alone |

Real courses do exist on hard ground — the mountain-bench courses in the
threshold set run to 0.27 under cap on 35 ha. But those are the *unusual
pockets*, or ground that was moved. Median hill-country land is not a golf site.

## 2. This is not a new departure — it makes an existing accident into a policy

`docs/gates/G-SKELETON.md` already recorded that generated cores pass the proxy
at **100/80/40/40/20/20 %** against real cores at **53/51/0/7/9/5 %** — with real
hill country at 0 %, which this survey independently reproduces. `heartland`'s
generator was **already** producing land more routable than reality, and
`d6w-siting-baseline.md` already concluded *"no v2 biome produces unroutable
seeds."*

`heartland` also already made this exact call once, explicitly: the river-valley
terraced lift at a 55–75 m budget is annotated *"golf-over-corpus by reviewer
decision"* — against a corpus whose median relief is 4.6 m.

What was implicit and undocumented is now explicit and principled. That matters
because an undocumented departure gets "corrected" later by someone chasing an
energy-distance regression.

## 3. Designed for golf does **not** mean flattened

The corpus is emphatic on this and it bounds the decision:

> *"**Real courses do not require calm ground.** Sand Hills GC plays at 0.45 calm
> on 19 m of window relief… **Architects don't even take the calmest window**…
> the scorer should treat calm as a **FLOOR, not a maximand** — otherwise
> generated courses will systematically sit on duller ground than real ones."*
> — `docs/calibration/d6w-siting-baseline.md`

So the macro objective is **not** "maximise `frac_under_cap`". It is:

> **Guarantee the proxy floors, then maximise interest subject to them.**

Relief, asymmetry, and a legible landform are the goal; calm is the constraint
that has to be satisfied, not the thing being optimised. River valley is the
proof that the two are different — it already maximises calm at 0.957 and fails
anyway.

## 4. Which measurements change hands

The 55-scalar battery splits by the spatial scale it measures, which is the same
line the pipeline already draws for stage ownership.

**MACRO — design-governed. Corpus becomes a *plausibility band*, not a target.
Not gated.** (25 keys)

`relief_p95_p5`, `elev_std`, `elev_skew`, `elev_kurtosis`,
`hypsometric_integral`, `local_relief_mean_100/200`,
`heterogeneity_index_100/200`, `valley_count_km2_60`, `valley_elong_120`,
`valley_len_p90_120`, `drainage_density`, `valley_spacing`, `concavity_theta`,
`chan_len_p90`, `network_connectivity`, `hypsometric_bimodality`,
`slope_bimodality`, `horton_bifurcation_ratio`, `horton_length_ratio`,
`strahler_max`, `anisotropy_ratio`, `anisotropy_orientation_deg`,
`rough_block_ratio_350`

**TEXTURE — corpus-fitted. Gated, unchanged.** (20 keys)

`spectral_slope_beta`, `hurst_from_beta`, `highfreq_energy_frac`,
`variogram_nugget/sill/range`, `rms_roughness_small/mid/large`,
`mean_abs_profile_curv`, `profile_curv_p90`, `mean_abs_plan_curv`,
`plan_curv_p90`, `laplacian_abs_mean`, `laplacian_abs_p95`, `rough_cv_30`,
`rough_top10_30`, `rough_moran_60`, `extrema_per_ha`, `slope_autocorr_length`

**MIXED — reported both ways, gated on neither alone.** (7 keys) The slope
family: `slope_mean/median/p90/p95/skew`, `frac_flat`, `frac_steep`. Slope at
2 m is set by texture *and* by the macro it sits on, so it cannot be assigned.

**COVER — not terrain.** `canopy_fraction`, `water_fraction`,
`canopy_fraction_clip`.

### The restated gates

| | was | is |
|---|---|---|
| **Energy distance** | pooled over 52 shared keys vs 2.35 | **ED_texture** over the texture family only, against the real split-half floor. **ED_macro reported, never gated** — a macro departure is the design working, not a regression. |
| **Golf proxy** | a readout | **the macro gate.** Floors must be met; `frac_under_cap` is not maximised. |
| **Horton, d2c, density, `near_par_frac`** | corpus bands as targets | plausibility bands. A biome may sit outside one **with a written reason**; drifting outside one silently is still a bug. |
| **P2 blind A/B ≤65 %, P3 name-the-biome ≥80 %** | unchanged | **unchanged, and now load-bearing** — see below. |

### The safeguard

Freeing the macro family removes a numeric brake, so something has to stop
"designed for golf" drifting into "obviously artificial". That is what **P2**
is for: a human deciding whether the ground could be real. It was always the
final authority — *"drawn-coefficient PCA synthesis passed 8/8 metrics and was
visually fake"* — and it is now the **primary** plausibility check on macro,
with `ED_macro` and the plausibility bands as its early-warning instruments.

If P2 starts rising and `ED_macro` has drifted, the macro design went too far.
That is a legible failure with a named cause, which is what the old arrangement
did not have.

---

## 5. River valley, resolved

The corpus problem from `02-drainage-patterns.md` §5 **dissolves under this
decision.** There is no longer a need for a terraced corpus, because the terrace
flight is a design choice and not a realism claim.

What river valley gets:

1. **Macro: authored terraces, deliberately side-asymmetric.** One bank carries
   the flight, the other stays a steeper cut bank — which is both what a
   migrating meander actually builds and what gives a hole a high side and a low
   side. Mechanically this is **step 5 run at successive datums**: a terrace is
   the network's own long profile at an earlier base level, so a flight is the
   profile solved N times and the valley-side surface blended toward each tread.
   Relief target comes from the **golf** band (≥ 7 m, and enough tread width to
   hold a corridor), not from the corpus's 4.6 m.
2. **Texture, floor: the 26 low-relief tiles.** They are a genuinely excellent
   `ValleyFloor` corpus — well sampled, clean, and exactly the fabric a
   floodplain has. Nothing is wrong with this data; it was only ever wrong as a
   *macro* target.
3. **Texture, valley sides: borrow hill country's exemplar pool.** A stratified,
   benched valley side in a river valley is the same texture family as a
   benched slope in hill country, and the dictionary is conditioned on
   `(slope, TPI, hillslope position, distance-to-channel)` — not on biome. So
   the conditioning already routes valley-side positions to valley-side patches.
   **This needs no new data**, which is why it is preferred over re-sampling.
4. **The 4 valley-spanning tiles become a plausibility check**, not a fit target
   — enough to ask "does our terrace flight look like that one" and too few to
   fit to.

`WindowClass` already carries `ValleyFloor` and `TerraceFlight` as separate
categorical classes, so the split has a home in the existing types.

**Consequence: river valley is no longer blocked, and no re-sampling campaign is
needed.**

## 6. Hill country, resolved

0 % of corpus tiles pass, driven by `largest_contig_ha` 29.7 against a 124 ha
floor. The intervention is the one named in the steer — **widen the valley
floors** — and it is the right lever rather than a blunt one:

- It adds contiguous sub-cap area exactly where a router wants it, in connected
  ribbons along the valley network rather than as scattered flat patches.
- It **preserves the identity**, which lives in the benched risers and the
  plateau tops (`slope_bimodality`, `tpi_landform_fractions`), not in the width
  of the valley bottom.
- It is a real landform: wide flat-floored valleys exist in dissected limestone
  country wherever a resistant bed holds the floor.

In the pipeline this is a **step 6 cross-section dial** — floor width as a
function of discharge — not a post-hoc flattening. The network stays where it
is; the ground hung off it gets a wider floor. `heartland` measured real
corpus valley floors at 24–40 m with a V-ratio of 0.25–0.33; hill country's own
trunk measured 70 m at 0.47, so **widening toward its own upper range is inside
real variation** before it is even a departure.

Acceptance is the proxy floor, not a width number: widen until
`largest_contig_ha` clears 124 ha, then stop. Reported alongside `ED_macro` so
the cost is visible.

---

## 7. What is explicitly NOT relaxed

- **Texture.** The whole corpus, dictionary and ED gate keep their authority.
  This decision does not license "fake but fun" fabric.
- **Drainage correctness.** Monotone descent, no loops, no crossings, water
  running downhill. These are physical truth, not realism preferences, and a
  golf course on ground where water runs uphill is a bug in any framing.
- **P2 / P3.** See §4.
- **The ruler.** Still frozen. Redefining a metric because a design decision
  moved it is exactly the drift `tools/ruler_lock.sh` exists to catch.
