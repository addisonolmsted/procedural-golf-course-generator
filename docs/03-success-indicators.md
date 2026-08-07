# Success Indicators

How we know each stage is on the way to good, plausible, varied terrain
**without waiting for the whole pipeline** — and how we know when to stop and
rethink instead of pressing on.

The stage docs' *Acceptance criteria* cover correctness: determinism,
invariants, budgets. This document adds the other three axes and the gates.
Together they are the definition of done per milestone; [../PLAN.md](../PLAN.md)
references the gates by name.

## The four axes

Every stage is judged on:

| Axis | Question | Instrument |
|---|---|---|
| **Correct** | Deterministic? Invariants hold? Budget met? | the stage doc's acceptance criteria |
| **Plausible** | Do the metrics *this stage owns* land in the corpus band? | the battery ([calibration/metric-battery.md](calibration/metric-battery.md)), same code as measures real tiles |
| **Varied** | Is between-seed dispersion right? | the dispersion ratio, below |
| **Legible** | Does a human see what the stage claims to have made? | stage-lab galleries + the protocols below |

### Judge each stage only on what it owns

The single most important rule, because violating it produces false alarms and
wasted rework: **every stage has an expected-failure list, and it is as binding
as its targets.**

S2's output is *deliberately* candle-wax — smooth, tubular interfluves with no
texture. Its variograms and roughness metrics **should** look wrong; judging S2
on texture is a category error. Conversely, S3 must not be blamed for a
drainage density S2 missed. Each gate below states both lists.

### The dispersion ratio — variety as a number

For each owned metric *m* and biome *b*:

```
DR(m, b) = std of m over generated seeds / std of m over real corpus tiles
```

**Target: 0.7 ≤ DR ≤ 1.3** for amplitude and discriminant metrics.

- DR « 1 → the generator makes the same course repeatedly (the classic
  certified-envelope failure — narrowness eating variety).
- DR » 1 → the biome has lost coherence.

This makes "varied" measurable with the machinery that already exists, and it
sets the *right* target: as varied as reality, not merely "different".
Categorical variety is checked separately (S0 gate: class rates at fitted
frequencies).

### The qualitative protocols

Formalized because of this project's most expensive lesson (v1 campaign,
2026-07): **every scalar was in range while three detectors were geometrically
wrong — only drawing the overlays on the hillshade exposed it.** Metrics
cannot tell you a thing is in the wrong *place*. Humans can, if the review is
structured:

| # | Protocol | Method | Pass |
|---|---|---|---|
| **P1** | Overlay review | Stage geometry (network, divides, discontinuities, window) drawn **over** the hillshade in stage-lab. Reviewer checks *placement*, not presence. | No feature visibly contradicts the surface it sits on |
| **P2** | Blind A/B | N generated + N real hillshades, matched rendering/scale/lighting, shuffled. Reviewer sorts real vs generated. | **Reviewer accuracy ≤ 65%** (near-chance is the aspiration; 65% is the honest bar) |
| **P3** | Name-the-biome | Generated hillshade; corpus-familiar reviewer names the biome. | ≥ 80% correct — identity is legible |
| **P4** | Same-or-different | Two courses, same biome, different seeds, side by side. | Reviewer sees *different sites* with the *same character* — both halves must hold |
| **P5** | Golf-eye | The chosen play window, hillshade + water overlay. Reviewer counts plausible hole corridors. | ≥ 9 imaginable corridors; no "where would you even put a green" verdict |

Every stage that produces geometry runs P1 **before its metrics are trusted** —
in that order, because P1 is what catches the class of bug that makes the
metrics lie.

### The golfability proxy — golfable before routing exists

Routing (S6) is far away, but "will this ground take a golf course" is
measurable from the surface alone — **if the thresholds come from reality**:

1. **Measure the 64 real course grids first**: fraction of the site under the
   fairway slope cap, largest contiguous sub-cap area, relief range occupied,
   ponding-risk coverage. Real courses define what "routable ground" measures
   as.
2. Score any generated window on the same quantities.

The proxy is a leading indicator at S2 (is the base surface in the right
regime?), the fitness function's backbone at S5, and a sanity check at S3
(did texture push slopes out of the playable band?). It reuses the course
corpus the siting scorer needs anyway — one measurement, three consumers.

---

## Per-stage gates

Format: **must pass** / **must NOT be judged on** / gate protocol.
Quantitative bands come from [calibration/targets.md](calibration/targets.md)
once measured; until then the *structure* of each gate stands and the numbers
are placeholders.

### G-SPIKE (milestone M3.5) — blocking, the go/no-go on the central bet

The crude dictionary from the 6 clean piedmont tiles, reconstructed over a
synthetic base.

- **Must pass:** reconstructed residual lands in band on `spectral_slope_beta`,
  `variogram_range/sill`, and the curvature pair against held-out real; P2 on
  close-range patch crops at reviewer accuracy ≤ 75% (relaxed — 6 tiles);
  no PSD peak at the patch pitch.
- **Not judged on:** biome variety (one biome), amplitude correctness (crude
  buckets), budget.
- **Fail ⇒ stop.** The fallback conversation (richer basis, quilting, or a
  return to simulation) happens *before* the corpus campaign is paid for.

### S0 — Site & biome draw

- **Must pass:** over 10 000 seeds — every biome and window class appears at
  its fitted rate; joint descriptor correlations match the envelope;
  multi-modality survives sampling (population is measurably multi-modal, not
  collapsed to the mean); exemplar pools differ between seeds.
- **Qualitative:** read 20 `spec.json`s side by side — do they describe
  *different sites*? (Cheap, and catches a collapsed envelope faster than any
  statistic.)
- **Not judged on:** anything visual. S0 makes config.
- **Leading indicator for downstream:** class-conditioned descriptor spreads —
  if escarpment-face piedmont and interfluve piedmont draw near-identical
  descriptors, the categorical variety is cosmetic and S1 will prove it.

### S1 — Macro structure

- **Must pass:** all six window classes yield structurally distinct C1 fields
  (pairwise field distance between classes ≫ within class); discontinuities
  step `hardness`/`accommodation` across the curve; tilt monotone to base
  level; DR in range on `relief` amplitude.
- **Qualitative — the class legibility test:** P1 on the implied-terrain
  hillshade (the stage-lab pattern already built for v1 framing), then a blind
  round: **reviewer names the window class from the hillshade alone, ≥ 80%**.
  If the class is not legible in the macro fields, nothing downstream will
  make it legible — this is S1's entire contribution, tested directly.
- **Not judged on:** realism of any kind. C1 is predisposition, not terrain.
- **Leading indicator:** run S2 on fixtures from every class × biome; S1 is
  good when no class produces a degenerate network.

### G-SKELETON (after S2) — blocking

- **Must pass:**
  - **Shared invariants in band on the base surface**: `dist_to_channel_p50`
    (the 104–120 m law — this *is* S2's plausibility test and it is fully
    measurable now), `drainage_density`, `slope_area_theta`,
    `hypsometric_integral`, Horton ratios. Cross-check: these do **not**
    differ significantly between biomes, because they do not differ between
    real ones.
  - `network_connectivity` separates heathland; sandhills yields zero channels;
    both without special cases.
  - **Conditioning-space overlap** — the leading indicator that predicts S3's
    success before S3 exists: the joint distribution of the conditioning
    vector (flow-distance-norm × hillslope position × slope × TPI ×
    relief-position) over generated cells overlaps the same distribution over
    real corpus cells (measured via `surfaces.py`). Overlap ≥ ~90% means the
    dictionary will have real patches for every cell S3 asks about;
    a miss means S3 will sample under-populated buckets — found now, not
    after the dictionary is fitted.
  - **Variety:** between-seed channel-map IoU low (planforms genuinely
    differ); DR in range on relief amplitude.
  - **Golfability proxy in regime:** candidate windows on the base surface
    already score within the real-course band on slope-cap fraction and
    contiguous area. Texture moves this a little; it should not have to
    rescue it.
- **Qualitative:** P1 — network + divides over the hillshade. Dendritic, not
  synthetic; junctions acute-ish; divides where the surface says they are;
  discontinuities visibly deflect the network. P4 on two seeds: different
  networks, same statistics.
- **Must NOT be judged on:** texture, roughness, variograms, β, "does it look
  real". **Candle-wax interfluves are expected and correct here.** Anyone
  reviewing S2 output must know this before looking.

### G-TERRAIN (after S3 + S4) — blocking, the realism gate

The first point where "does it look like real terrain" is a fair question.

- **Must pass:**
  - **Discriminants in band per biome**: β, variogram range/sill, curvature
    pair, anisotropy (tracking the grain axis), TPI fractions, bimodalities
    where owned. Energy distance vs held-out real **beats the v1 baseline of
    2.35**.
  - **Shared invariants still in band** after amplification —
    `skeleton_agreement` ≥ 0.9 says texture decorated the structure rather
    than replacing it.
  - Within-course conditioning contrast (river valley: floodplain vs valley
    side measurably different residual statistics).
  - No PSD peak at patch pitch or harmonics (seams).
  - S4 regression: zero axis-aligned channels across flats; water counts per
    biome in band (heathland kettles present, sandhills empty, great plains
    often empty).
  - **Variety:** DR 0.7–1.3 on all amplitude + discriminant metrics;
    P4 passes.
- **Qualitative — the full protocol battery:** P2 blind A/B at course scale
  *and* at close range (≤ 65%); P3 name-the-biome (≥ 80%); P1 water overlay
  (water sits in the terrain's own low places).
- **Not judged on:** golf-specifics beyond the proxy; hole-scale detail (S9's
  band is not generated yet — reviewers should expect 0.5 m-scale blandness).

### G-SITE (after S5)

- **Must pass:** siting arithmetic asserted (600 m, ±450 m, margin ≥ 750 m);
  chosen window beats the centred window on the fitted score across 1 000
  seeds, and beats the *median* random window by a stated margin; all C2
  invariants; golfability proxy of the chosen window inside the real-course
  band for ≥ 95% of seeds.
- **Qualitative:** P5 golf-eye on the chosen window; and a panel render of the
  top-3 windows — a human agrees the winner is defensible (not necessarily
  optimal; defensible).
- **Not judged on:** routing quality — S6 does not exist. The proxy stands in.

### Terrain-complete (the milestone this document exists for)

G-TERRAIN + G-SITE passing together, for **piedmont and heathland** (goldens)
plus sandhills (the empty case), is the definition of "the terrain is done" —
the declared precondition for starting routing/hole work. At that point:

- every claim about plausibility is a measured number against real ground,
- every claim about variety is a dispersion ratio plus a passed P4,
- every claim about golfability is the proxy, calibrated on real courses,
- and the qualitative record is a set of passed protocols, not adjectives.

## Gate discipline

- **Blocking gates:** G-SPIKE, G-SKELETON, G-TERRAIN, G-SITE. The next
  milestone is not claimed until the gate's report (metrics + gallery) is
  reviewed. Everything else is advisory and continuous.
- A gate report is an artifact: the metric table, the DR table, the gallery
  PNGs, and the protocol outcomes, committed alongside the goldens.
- **Failing a gate is a finding, not a delay.** The architecture explicitly
  reserves its hardest fallback decisions (dictionary insufficient; heathland
  needs a second kernel) for gate failures — that is the moment the evidence
  exists.
- Protocol reviewers: currently the project owner. When more than one person
  reviews, P2/P3 results are per-reviewer and the worst result counts.