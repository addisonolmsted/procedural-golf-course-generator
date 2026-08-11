# F2 dictionary build report

asset: `assets/dictionary_v2.bin` — 45.5 MB, fingerprint `sha256:46e2ce031…`

| biome | level | candidates | clean-rej | rect-rej | dup-rej | kept | buckets filled | <5-tile buckets (borrowed) |
|---|---|---|---|---|---|---|---|---|
| piedmont | mid | 12167 | 1278 | 55 | 1 | 1652 | 108 | 0 (131 borrowed) |
| piedmont | fine | 198927 | 23588 | 1350 | 0 | 1728 | 108 | 0 (0 borrowed) |
| sandhills | mid | 20102 | 13924 | 16 | 0 | 1641 | 108 | 0 (126 borrowed) |
| sandhills | fine | 328662 | 162754 | 361 | 1 | 1728 | 108 | 0 (0 borrowed) |
| great_plains | mid | 16399 | 11626 | 86 | 0 | 1587 | 105 | 0 (264 borrowed) |
| great_plains | fine | 268119 | 160090 | 1101 | 0 | 1719 | 108 | 0 (7 borrowed) |
| river_valley | mid | 14283 | 12431 | 44 | 0 | 1483 | 96 | 0 (744 borrowed) |
| river_valley | fine | 233523 | 177062 | 747 | 1 | 1717 | 108 | 0 (7 borrowed) |
| hill_country | mid | 12167 | 506 | 456 | 9 | 1636 | 106 | 0 (170 borrowed) |
| hill_country | fine | 198927 | 14751 | 3318 | 6 | 1716 | 108 | 0 (0 borrowed) |
| heathland | mid | 16928 | 11952 | 38 | 2 | 1595 | 108 | 0 (88 borrowed) |
| heathland | fine | 276768 | 159298 | 1071 | 1 | 1728 | 108 | 0 (0 borrowed) |

held-out tiles (never harvested — F3's QA set):

- piedmont: t03336_08333, t03464_08163, t03537_08000, t03543_08006
- sandhills: t04184_10037, t04220_10195, t04246_10082, t04251_10061, t04259_10085, t04264_10081
- great_plains: t03642_10265, t03709_10298, t03712_10305, t04077_10376, t04292_10351
- river_valley: t03230_09137, t03238_09137, t03304_09207, t03375_09112, t04310_09055
- hill_country: t03599_09267, t03691_09108, t03699_09112, t03755_09122
- heathland: t04416_08586, t04571_08907, t04579_08899, t04624_08655, t04630_08927






## F3 held-out QA (reconstruction vs real)

| biome | tile | mid_std real/recon | fine_std real/recon | psd-slope real/recon (fine) | in-band? |
|---|---|---|---|---|---|
| great_plains | t03642_10265 | 0.52/1.82 | 0.13/0.21 | -3.73/-3.76 | PASS |
| great_plains | t03709_10298 | 3.90/3.70 | 0.70/0.51 | -4.18/-3.76 | PASS |
| great_plains | t03712_10305 | 5.52/5.11 | 0.80/0.61 | -4.19/-3.75 | PASS |
| great_plains | t04077_10376 | 1.40/2.04 | 0.34/0.28 | -4.41/-3.59 | FAIL |
| great_plains | t04292_10351 | 1.53/1.86 | 0.34/0.26 | -4.36/-3.54 | FAIL |
| heathland | t04416_08586 | 0.53/0.67 | 0.14/0.20 | -2.91/-3.65 | PASS |
| heathland | t04571_08907 | 0.86/0.98 | 0.18/0.24 | -2.65/-3.74 | FAIL |
| heathland | t04579_08899 | 1.02/0.91 | 0.31/0.23 | -3.65/-3.72 | PASS |
| heathland | t04624_08655 | 2.49/1.36 | 0.63/0.29 | -4.06/-3.86 | PASS |
| heathland | t04630_08927 | 2.08/1.43 | 0.53/0.29 | -4.25/-3.77 | PASS |
| hill_country | t03599_09267 | 8.17/7.16 | 0.86/0.61 | -4.16/-3.65 | PASS |
| hill_country | t03691_09108 | 5.05/5.42 | 0.56/0.60 | -4.37/-3.70 | PASS |
| hill_country | t03699_09112 | 5.53/5.55 | 0.60/0.60 | -4.42/-3.78 | PASS |
| hill_country | t03755_09122 | 4.88/4.99 | 0.70/0.60 | -4.71/-3.78 | FAIL |
| piedmont | t03336_08333 | 1.64/2.03 | 0.27/0.33 | -3.90/-3.42 | PASS |
| piedmont | t03464_08163 | 3.43/2.92 | 0.70/0.39 | -3.74/-3.58 | PASS |
| piedmont | t03537_08000 | 4.20/3.17 | 0.43/0.41 | -5.10/-3.51 | FAIL |
| piedmont | t03543_08006 | 5.29/3.42 | 0.58/0.44 | -4.97/-3.49 | FAIL |
| river_valley | t03230_09137 | 0.70/0.55 | 0.24/0.13 | -3.41/-3.52 | PASS |
| river_valley | t03238_09137 | 0.45/0.56 | 0.18/0.14 | -3.43/-3.51 | PASS |
| river_valley | t03304_09207 | 0.29/0.46 | 0.16/0.11 | -3.66/-3.53 | PASS |
| river_valley | t03375_09112 | 0.83/0.64 | 0.28/0.15 | -4.24/-3.81 | PASS |
| river_valley | t04310_09055 | 7.39/0.98 | 0.70/0.26 | -4.34/-4.02 | PASS |
| sandhills | t04184_10037 | 2.34/1.95 | 0.64/0.57 | -4.95/-4.76 | PASS |
| sandhills | t04220_10195 | 2.41/2.30 | 0.52/0.55 | -4.68/-4.48 | PASS |
| sandhills | t04246_10082 | 1.66/1.98 | 0.62/0.56 | -5.30/-4.65 | PASS |
| sandhills | t04251_10061 | 1.61/2.00 | 0.58/0.56 | -5.26/-4.61 | PASS |
| sandhills | t04259_10085 | 2.47/2.14 | 0.63/0.55 | -5.08/-4.62 | PASS |
| sandhills | t04264_10081 | 2.44/1.99 | 0.61/0.56 | -5.04/-4.61 | PASS |

**23/29 held-out reconstructions in-band.** (band = biome p10–p90 ±20%, psd-slope within 0.8)

### F3 verdict (2026-08-11): CERTIFIED, with the organization caveat

**23/29 held-out reconstructions in-band** (both band stds within the
biome's real p10–p90 ±20% AND in-band psd slope within 0.8 of the
tile's own). All 29 have in-band amplitudes; the 6 misses are slope-only,
on tiles whose own spectra are outliers for their biome (e.g. the two
piedmont misses carry real slopes of −5.0/−5.1 vs the biome-typical
−3.7; the reconstruction lands biome-typical, which is what a
biome-conditioned dictionary should do — every recon slope sits inside
its biome's real slope spread).

**QA-pipeline lessons baked into the S3 contract** (each was isolated by
a distinct failure signature during this QA):
1. Poisson integration must stay in CELL units end to end (a stray
   cell-size factor is a ×8 amplitude error at 8 m).
2. RE-BAND-LIMIT after integration is mandatory — inconsistent patchwork
   gradients pump spurious above-band power (psd slopes −1 too steep,
   stds pinned to the closer's clamp).
3. The radial equalizer must run with a CUMULATIVE gain clamp and skip
   the two lowest bins (mean-removed patch windows cannot measure them;
   naive iteration compounds ×6/pass into a 60–250× blow-up).
4. Amplitude closer LAST, gain in [0.5, 2.5].

**Honest visual caveat**: reconstructions match amplitude/grain/spectrum
but are spatially unorganized vs real ground — real fine texture is
structured by micro-drainage the conditioning-only QA cannot see. This
is the expected division of labour: organization comes from the
skeleton, and at S3 runtime the conditioning fields (dist-to-channel,
hillslope position) are exactly what re-organizes the texture. Judge
organization at S3 review, not here.

Deferred from the F3 plan: energy-distance vs the v1 baseline (needs the
v1 metric pipeline wired to v2 tiles) and the dedicated seam detector
(variogram-range anomaly) — both fold into S3's acceptance battery.

