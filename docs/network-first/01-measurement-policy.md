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
definitional and not a bug (`scratchpad/horton_policy.py`). **Both prior numbers
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
(`docs/network-first/ruler.lock`, 16 files: the metrics battery, the corpus-side
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

Corpus: the 203 clean tiles in `tools/macro_campaign/out/`, per
`exclude.json` / `review_v2.json` (both digest-locked).

<!-- FULL-CORPUS TABLE PENDING -->

---

## 5. Still to declare before M2

These follow the same rule — declare the policy, then measure the corpus with
it, before any of them becomes a construction target:

- `near_par_frac` — `planform.rs` / `real_planform.py` are already a matched
  pair (20 m samples, 60 m band, 90 m junction exemption, corpus traced at 8 m).
  Confirm the pair still agrees under this branch's channel-extraction policy.
- `d2c` / drainage density — `measure_battery` mirrors `structure.py`'s policy
  at 6e4 m², which already matches Policy A. Re-measure and record.
- junction angles — `junction_real.py`, 48 m baseline.
