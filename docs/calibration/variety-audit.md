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
