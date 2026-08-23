# The declared measurement policy

**M0 deliverable.** Attempt 4 is about to make network statistics a
**construction target** rather than an outcome. `docs/calibration/parameter-provenance.md`
warns exactly against that: the old authored-era bands *"were achievable only
because the previous engine constructed those quantities directly."* So the
policy has to be declared once, up front, and the ruler has to stay independent
of the construction.

---

## 1. The conflict, and its resolution

Two Horton numbers were in circulation and they disagreed by a factor of two:

- `docs/calibration/parameter-provenance.md` records corpus bands
  **Rb 1.7–4.5 / Rl 0.5–1.9**, from real tiles measuring Rb 2.14–3.08.
- The network-round-1 attribution measured real **Rb 5.20 / Rl 2.14** and
  concluded that real networks branch *more* than ours.

**Neither is wrong. They are different rulers**, differing in four ways at once:

| | Policy **A** — `tools/metrics/metrics/core.py::horton_ratios` | Policy **B** — `tools/macro_campaign/horton_real.py` |
|---|---|---|
| accumulation threshold | 6e4 m² | 1.2e5 m² |
| resolution | tile-native (**2 m**) | downsampled to **8 m** |
| what is counted | stream **heads** per order | **reaches** — maximal constant-order runs between confluences |
| how the ratio is fitted | log-linear **regression** over all orders, `Rb = exp(-slope)` | **median of successive order-pair ratios** |

Both were re-run over the *same* tiles to confirm the disagreement is
definitional and not a bug (`tools/macro_campaign/horton_policy_compare.py`). **Both prior numbers
reproduced**, which is the evidence that closes the question:

| biome | A: Rb | A: Rl | A: Ω | B: Rb | B: Rl | B: Ω |
|---|---|---|---|---|---|---|
| piedmont | 4.10 | 1.89 | 3 | 2.40 | 0.91 | 3 |
| great_plains | 4.58 | 2.02 | 3 | 2.98 | 1.19 | 3 |
| river_valley | 5.55 | 1.59 | 3 | 2.90 | 0.85 | 2 |
| hill_country | 3.88 | 2.19 | 4 | 2.14 | 1.11 | 3 |
| heathland | 4.58 | 1.97 | 3 | 2.50 | 0.94 | 3 |
| sandhills | 4.80 | 1.83 | 3 | 2.30 | 1.08 | 3 |
| **across biomes** | **3.88–5.55** | **1.59–2.19** | | **2.14–2.98** | **0.85–1.19** | |

*(pilot, n=8 tiles per biome; superseded by the full-corpus table in §4)*

Policy A's spread brackets the round-1 figure of 5.20 (river_valley 5.55).
Policy B reproduces the provenance doc's 2.14–3.08 / 0.92–1.16 almost exactly.

Worth noting in passing: under **both** policies the ratios are nearly
biome-invariant — which is what the metric battery's "shared invariant"
classification predicts, and is independent confirmation that the classification
is right.

---

## 2. The declared policy: **A**

Three reasons, in order of weight:

1. **It is the ruler the headline gate already uses.** Policy A is
   `horton_bifurcation_ratio` / `horton_length_ratio` in the 52-key feature
   vector that pooled energy distance is computed over. Constructing to
   Policy B while Policy A drifts would degrade ED invisibly.
2. **It measures at native 2 m.** Attempt 4's network is grown to ~2 m.
   Policy B downsamples to 8 m *and* doubles the threshold, discarding
   precisely the scales this attempt adds. A ruler that cannot see the change
   cannot judge it.
3. **It resolves more orders** (Ω 3–4), so the regression has more to fit.

Policy B is **retained as a secondary diagnostic**, so the old S2 band stays
interpretable and cross-attempt comparisons remain possible. It is not a target.

### Why this is not circular

The provenance doc's warning is about bands you can only hit because you built
them in. That does not apply here, and the reason is structural:

> **Policy A extracts channels by accumulation threshold from the *synthesised
> surface*, not from the constructed graph.**

So the target is *"the surface we synthesise, measured the corpus way, lands in
the corpus band"* — not *"our graph has these ratios by construction."* The
constructed network is an input to the surface; the ruler reads the surface. It
stays independent, and it can still say no.

This also gives M3 a free integrity check: if the constructed graph and the
network extracted from the synthesised surface disagree, that is a **synthesis
bug**, not a tuning problem. In `heartland` this quantity was a *ceiling* to be
negotiated with; here it is a tautology that either holds or reveals a defect.

---

## 3. The ruler is frozen

`tools/ruler_lock.sh` digest-locks the measurement stack
(`docs/ruler.lock` — moved out of this folder by attempt 5, since the ruler
outlives any one attempt; 28 files: the metrics battery, the corpus-side
flow/structure/netstats/planform scripts, the golf proxy and its thresholds, and
the corpus cull lists).

Note a deliberate departure from allowlist rule 1. Admitted items are normally
**copied** with a provenance comment — but the measurement code is the one thing
that must **not** be copied. `docs/calibration/metric-battery.md`'s purity rule
requires exactly one implementation, *"because the same function must measure a
3DEP tile and generator output; two implementations would drift, and the drift
would be indistinguishable from a generator improvement."* So the ruler is used
in place and locked rather than forked.

```
tools/ruler_lock.sh verify    # fails loudly if anything drifted
tools/ruler_lock.sh write     # re-baseline — a deliberate act, never incidental
```

Both `ruler_lock.sh` and `no_old_deps.sh` were negative-tested on introduction:
each was shown to fail on a planted violation before being trusted. An
instrument that has never been seen to fail is not evidence.

---

## 4. The bands

Corpus: all **203 clean tiles** in `tools/macro_campaign/out/`, per `exclude.json` / `review_v2.json` (both
digest-locked). Full output: `horton_full.txt`.

| biome | n | **A: Rb med** | A: Rb p10–p90 | **A: Rl med** | A: Ω | B: Rb med | B: Rl med |
|---|---|---|---|---|---|---|---|
| piedmont | 27 | **3.92** | 3.42–6.34 | **2.02** | 3 | 2.62 | 1.01 |
| great_plains | 36 | **4.42** | 3.41–6.52 | **2.25** | 3 | 2.28 | 1.09 |
| river_valley | 32 | **4.58** | 3.65–6.70 | **1.68** | 3 | 2.98 | 0.96 |
| hill_country | 27 | **4.18** | 3.50–6.54 | **1.93** | 3 | 2.15 | 0.93 |
| heathland | 37 | **4.00** | 3.49–4.89 | **1.80** | 3 | 2.25 | 0.95 |
| sandhills | 44 | **4.74** | 3.60–6.46 | **1.64** | 3 | 2.38 | 0.98 |
| **across biomes** | 203 | **3.92–4.74** | 3.41–6.70 | **1.64–2.25** | 3 | 2.15–2.98 | 0.93–1.09 |

### The declared targets

> **Rb 3.92–4.74, Rl 1.64–2.25**, biome-invariant, measured by Policy A
> on the synthesised surface.

The pilot's wider spread (Rb 3.88–5.55) was small-n noise: at n=8 river_valley
read 5.55, at n=32 it reads 4.58. **Take the medians from this table, not the
pilot.**

### Two caveats that change how the gate must be written

**1. Ω is 3 in every biome, so Rb comes from a three-point log-linear fit.**
That is inherently noisy, and the measured per-tile spread shows it:
p10–p90 runs 3.41–6.70 against biome medians of only 3.92–4.74. The
per-tile ratio is a weak estimate; the per-biome median over many tiles is a
strong one.

**2. So Horton must NOT be gated pointwise.** `docs/calibration/metric-battery.md`
classes process metrics as "match pointwise, within a narrow band" — for this
metric, at this corpus, the band is *not* narrow and a pointwise gate would
either reject good seeds or accept anything. The gate is:

- **median over the seed battery vs the corpus median**, per biome, and
- **dispersion ratio 0.7–1.3** on the per-seed spread against the per-tile
  spread above.

This is the same shape the D5 battery already uses for `d2c` (medians must land
in band and spread under 30 m), and it is the honest reading of the data.

### Where "real 5.20" sits

Round 1's figure is above every biome median in this table but inside the
per-tile p10–p90 everywhere. It is consistent with Policy A and not with Policy
B, which is what identified the ruler — but it is **not** the corpus median, and
aiming the network stage at 5.20 would aim it high by roughly 10–30 %. The
targets above supersede it.

---

## 5. The planform family — declared and measured

**Policy (planform family):** reaches traced on an **8 m** grid at
`extract_v2.CHANNEL_AREA_M2` = **6 e4 m²** — the same threshold Policy A
declares, at the coarser grid the matched generated-side twin
(`planform.rs`) routes on. Tracing the corpus at its native 2 m resolves the
same catchment into 464 reaches against our ~100, which changes both the pair
count and the junction exemption rate, so the grid is part of the policy.

`near_par_frac` is **pooled across tiles** (`near_len / near_total`, matching
`real_planform.py:264`), not a median of per-tile ratios. The two differ and
only one is the declared number.

Measured over all 203 clean tiles — `tools/macro_campaign/pattern_survey.py`,
raw output `pattern_full.txt`:

| biome | n | near_par % | d2c m | density km/km² | junction p50 | >80° |
|---|---|---|---|---|---|---|
| piedmont | 27 | 1.29 | 104.3 | 2.36 | 40.6° | 10.7% |
| great_plains | 36 | 3.06 | 115.9 | 2.21 | 39.8° | 8.5% |
| river_valley | 32 | 3.04 | 96.2 | 2.60 | 36.9° | 13.0% |
| hill_country | 27 | 2.94 | 106.7 | 2.30 | 45.0° | 11.8% |
| heathland | 37 | 1.38 | 97.3 | 2.58 | 39.8° | 13.4% |
| sandhills | 44 | 1.95 | 104.3 | 2.39 | 41.6° | 12.4% |
| **band** | 203 | **1.3–3.1** | **96–116** | **2.21–2.60** | **37–45°** | **8.5–13.4%** |

Two confirmations that the instrument is the same one the corpus record used:
piedmont `near_par` reads 1.29 % against the recorded 1.4 %, great_plains 3.06 %
against 3.5 %. `d2c` 96–116 and density 2.21–2.60 reproduce the known
shared-invariant bands.

**Junction angles supersede the old 3 % T-junction band.** That band came from
the authored engine, which enforced a 30–62° mouth angle by construction. The
corpus measures 8.5–13.4 % above 80° at the 6-cell baseline — real networks make
far more orthogonal confluences than the old gate allowed.

### Derangement is NOT measurable this way

`fill_depressions` runs before routing, so heathland's and sandhills' rows above
describe a **phantom** network on filled ground. They remain the correct
shared-invariant targets — the corpus measures every biome this way — but
derangement must be gated on `network_connectivity` with pits kept (0.04–0.06
for these two against 1.00 integrated), never on `d2c` or density.

---

## 6. Still to declare before M2

- Valley cross-section (`w10/w50`) — `heartland` closed this question with
  `scratchpad/vratio.py`; re-derive under this branch's policy.
- `network_connectivity` with pits kept, for the two deranged biomes.
- Relief per biome is already measured here and matters more than expected:
  p50 runs hill_country 74 m, piedmont 48 m, great_plains 45 m, sandhills 38 m,
  heathland 18 m, **river_valley 4.6 m** — see `02-drainage-patterns.md` §5,
  which is a corpus problem, not a measurement one.
