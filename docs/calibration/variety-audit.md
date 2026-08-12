# Macro variety audit — does generated within-biome variety span the corpus?

*2026-08-10. Instruments: `course-skeleton/examples/variety_dump.rs` (20
seeds × 6 biomes, S2 surface decimated to 8 m) + `variety_audit.py`
(same metrics over every kept corpus tile). Metrics, per tile/seed, on
the 400–1600 m macro band: orientation order **A** (power-weighted
2θ vector strength; 0 = isotropic, 1 = perfectly oriented),
power-weighted dominant wavelength **λ**, and band relief (DoG bandpass,
p95−p5). Identical code path both sides.*

Motivating observation (reviewer): about half the real sandhills tiles
are 2–3 large mounds with valleys, not wavy dune trains — and the
generated sandhills never shows this. Question: is that true elsewhere?

## Results (p10 / p50 / p90 per biome)

| biome | src | n | A | λ (m) | band relief (m) |
|---|---|---|---|---|---|
| piedmont | real | 27 | .165 / .307 / .486 | 930 / 1041 / 1159 | 13.2 / 21.4 / 42.5 |
| | gen | 20 | .142 / .366 / .629 | 962 / 1073 / 1221 | 17.6 / 26.3 / 44.8 |
| great_plains | real | 36 | .110 / .308 / .614 | 930 / 1102 / 1282 | 9.5 / 17.1 / **42.7** |
| | gen | 20 | .150 / .289 / .506 | 969 / 1093 / 1170 | 10.3 / 14.8 / **22.9** |
| river_valley | real | 32 | .366 / **.611** / .823 | **709** / 1000 / 1154 | 1.0 / 2.0 / 6.9 |
| | gen | 20 | .154 / **.304** / .582 | **957** / 1027 / 1153 | 1.1 / 2.1 / 8.1 |
| hill_country | real | 27 | .198 / .390 / .556 | 884 / **981** / 1094 | 23.0 / 30.2 / **75.0** |
| | gen | 20 | .133 / .388 / .665 | 997 / **1120** / 1212 | 23.3 / 31.1 / **42.4** |
| heathland | real | 37 | .224 / .410 / **.695** | 917 / 1066 / 1230 | 2.9 / **8.4** / **15.3** |
| | gen | 20 | .109 / .309 / **.488** | 1031 / 1119 / 1192 | 3.0 / **4.4** / **6.6** |
| sandhills | real | 44 | .186 / .400 / **.755** | 961 / **1190** / 1307 | 9.4 / **20.3** / 32.1 |
| | gen | 20 | .086 / .263 / **.461** | 1036 / 1090 / 1175 | 10.8 / **14.7** / 21.7 |

## Findings, ranked

1. **sandhills — wrong scale AND missing megaform.** The real corpus is a
   continuum in orientation order (A 0.06–0.84: ~⅓ strongly-oriented dune
   trains, ~40% weakly-oriented mound fields at 1.0–1.4 km, the rest
   mixed). Generated sandhills covers neither end: the mound megaform is
   absent, and the authored dune trains (aeolian module, 210 m + 340 m
   wavelengths) sit BELOW the macro band entirely — real oriented power is
   at ~1.0–1.3 km, so generated sandhills measures nearly isotropic
   (A p90 0.46 vs real 0.755) exactly where real trains are strongest.
   The module doc even assigns S1 the "≥400 m dune-train envelope" that
   nothing currently emits. Fix = the proposed megaform draw (orientation
   strength drawn from the measured A distribution, trains vs mounds) plus
   re-scaling the train wavelengths into the real band.
2. **river_valley — macro grain far too weak.** Real bottomland is the
   most oriented biome in the corpus (A p50 0.611 — bluff lines, meander
   belts, terraces all share the valley axis); generated rv measures
   0.304. Confirms the E6 outlier (anisotropy 241) from the other
   direction. Also missing the short-λ tail (real p10 709 m — meander-belt
   scale). Fix direction: per-biome wave-field anisotropy (ISO_FRAC /
   grain strength are currently global constants — the S1 spectral fit
   matched POOLED anisotropy only) with rv drawn strongly grain-locked.
3. **relief heavy tails missing (great_plains, hill_country).** Medians
   match (E7 fitted them) but the dissected outliers real land carries are
   compressed away: gp band-relief p90 42.7 real vs 22.9 gen; hc 75.0 vs
   42.4. The E7 fit moved log-MEANS only; envelope spread needs widening
   in the queued consolidated calibration pass.
4. **heathland band relief ×~2 low** (p50 8.4 vs 4.4, p90 15.3 vs 6.6)
   — a larger in-band gap than the 10% total-relief drift already on the
   ledger; treat as the same work item, but fit the 400–1600 m band, not
   just total core relief. Upper orientation tail also missing (.695 vs
   .488).
5. **piedmont — healthy.** Generated spans the real scatter on all three
   axes (mildly wide on A, mildly high on relief median; nothing outside
   the real envelope). great_plains A/λ are near-misses only.

## Standing instrument

Re-run after any S0/S1 envelope or class change:

    cargo run --release -p course-skeleton --example variety_dump <dir>
    python variety_audit.py <dir>      # tools/macro_campaign/variety_audit.py

The audit is the acceptance test for the sandhills megaform work and the
rv anisotropy work; the relief-tail items fold into the consolidated
relief refit.

## Fix pass (same day) — all four findings addressed

Five fit rounds against this audit + the E7 totals. What changed:

- **S1 aeolian macro (sandhills)**: new always-drawn transcript block in
  `course-primitives` — a train-vs-mound continuum (shared course scalar
  `RunIdentity::AEOLIAN_SALT`, smoothstep-mapped) blending two beating
  transverse crest waves (λ 1100–1500 m, along-crest swing) against a
  10-wave isotropic mound field (λ 1050–1550 m), at 0.30 relief share.
- **S2 aeolian module rework**: the 210/340 m two-sinusoid corduroy
  (right variance, wrong spectrum) became a 14-wave mid-band field
  (λ 120–400 m, ±28° around the wind axis), SplitMix-expanded from the
  same 5-draw transcript; amplitude calibrated to the real mid-band
  (64–400 m) residual std 3.58 m; intensity coupled to the SAME
  continuum scalar so mound-field courses keep small dunes, not
  train-strength ridges (measured 2.05–4.07 m across seeds vs real
  ~2.9–4.3 m).
- **river_valley grain**: envelope `wave_iso_frac` 0.12 + new
  `grain_lock` = 1.0 (the drawn grain axis rotates onto the base-edge
  axis BEFORE the waves consume it — the wave-VECTOR axis perpendicular
  to the valley direction; the transposed version measurably collapsed
  orientation, round-4 lesson).
- **great_plains / hill_country tails**: relief chol σ widened (gp ×3.7
  net, hc ×2.0 net) + gp log-mean +0.12.
- **heathland**: `wave_share` 0.46, `wave_iso_frac` 0.45, log-mean +0.09.
- **sandhills totals**: log-mean −0.10 compensates the added macro.

Post-fix audit (gen, 20 seeds) vs real:

| biome | A p10/50/90 | λ p50 | band relief p10/50/90 | total (target) |
|---|---|---|---|---|
| piedmont | .142/.366/.629 (real .165/.307/.486) | 1073 (1041) | 17.6/26.3/44.8 (13.2/21.4/42.5) | 36.1 (35.7) |
| great_plains | .214/.357/.533 (.110/.308/.614) | 1065 (1102) | 6.1/12.9/38.9→42.6 (9.5/17.1/42.7) | 34.0 (33.8) |
| river_valley | .247/.522/.632 (.366/.611/.823) | 1082 (1000) | 1.1/2.2/9.3 (1.0/2.0/6.9) | 3.6 (3.6) |
| hill_country | .132/.334/.605 (.198/.390/.556) | 1109 (981) | 20.9/28.8/50.5 (23.0/30.2/75.0) | 67.6 (63.9) |
| heathland | .253/.446/.653 (.224/.410/.695) | 1061 (1066) | 5.9/9.2/14.5 (2.9/8.4/15.3) | 14.1 (14.1) |
| sandhills | .162/.371/.616 (.186/.400/.755) | 1125 (1190) | 14.0/19.8/26.6 (9.4/20.3/32.1) | 32.2 (32.6) |

**Documented residuals** (accepted, not bugs): hill_country's band-relief
p90 (75 real vs ~51) is dissection, not S1 amplitude — widening σ moved
it not at all across three rounds; it belongs to S2 carving depth + S3
texture. river_valley's extreme top-end orientation (real p90 .823) and
sandhills' very strongest trains (real p90 .755) are compressed ~20%.
great_plains' low-end A is slightly high (.214 vs .110).

**C1 legibility note**: the S1 synthesis changed for rv/heathland/gp
(orientation, share) and sandhills (new macro) — per C1_REVIEW.md, a
fresh blind session is warranted when convenient; sandhills is not a
window class so the five-class gate is likely unaffected.

## Addendum 2026-08-11 — position-dependent amplitude ("should the floor
be quieter than the flanks?")

Reviewer question at C1: is equal wave amplitude on low vs high ground
realistic, or should some regions suppress it? Measured three ways (the
dictionary's own per-relief-pos bucket amplitudes; macro-band 400–1600 m
local envelope vs relief-position tercile on real tiles; the same on
generated S1+S2 surfaces). The real answer has TWO regimes:

- **Depositional lows are quieter** — heathland outwash flats 0.59
  low/high, sandhills interdune 0.75. The existing wave_mult suppression
  (valley_floor keeps 60%, basin pocket 25%) is the right mechanism and
  measures correctly: generated heathland 0.68 vs real 0.59, gp 0.86 vs
  0.92.
- **Erosional/fluvial lows are LOUDER, not quieter** — piedmont 1.17,
  hill_country 1.45, river_valley 1.63: at 400–1600 m the "noise" in the
  lows IS drainage form (incised valleys, meander belts, oxbows). That
  energy belongs to S2's carving and the trunk-river module, never to
  the S1 wave field — no suppression rule should be inverted for it.

Generated vs real ratios: piedmont 1.05/1.17 ✓, gp 0.86/0.92 ✓,
heathland 0.68/0.59 ✓; three recorded gaps: **river_valley 0.85 vs 1.63**
(the biggest — real bottomland lows carry the meander-belt's own macro
topography: scars, oxbows, belt ridges; our trunk-river carving is too
plain — an S2 module enrichment, natural to do alongside the E7 transect
fit), **hill_country 0.99 vs 1.45** (dissection energy in the lows —
same family as the known hc dissection-tail residual; transect-fit item),
**sandhills 0.93 vs 0.75** (the crest/floor asymmetry already noted in
D6-W; the dictionary's relief-pos conditioning carries the corrective
texture at S3 — its sandhills buckets measure low/high 0.79–0.84).

