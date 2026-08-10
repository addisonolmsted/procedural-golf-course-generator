# E6 — empirical invariant/discriminant confirmation (2026-08-10)

Corpus: 203 kept tiles, staged scalars from `extract_v2`. Method: one-way
F-ratio (between-biome / within-biome variance) per metric, plus macro-band
organization measurement. This is the data check on the architecture's
central assumption — identity lives in amplitude/texture, not spacing.

## Metric roles, measured

| metric | F | within-CV | verdict |
|---|---|---|---|
| slope_median | 130.1 | 0.35 | DISCRIMINANT |
| local_relief_100m | 108.8 | 0.44 | DISCRIMINANT |
| lake_frac | 95.5 | 0.79 | DISCRIMINANT |
| local_relief_200m | 85.6 | 0.42 | DISCRIMINANT |
| local_relief_400m | 67.8 | 0.41 | DISCRIMINANT |
| mid_std_m (64–400 m texture) | 61.8 | 0.45 | DISCRIMINANT |
| relief_p99_p1_m | 57.9 | 0.39 | DISCRIMINANT |
| channel_frac | 50.0 | 0.14 | DISCRIMINANT |
| core_relief_m | 43.2 | 0.53 | DISCRIMINANT |
| fine_std_m (<64 m texture) | 40.1 | 0.39 | DISCRIMINANT |
| drainage_density (3 thresholds) | 17.5–24.6 | 0.11 | weakly shared |
| dist_to_channel p25–p75 | 9.4–16.8 | 0.10–0.12 | weakly shared |
| dist_to_channel_p90_m | 6.4 | 0.09 | SHARED INVARIANT |
| core_relief_ratio | 2.8 | 0.21 | SHARED INVARIANT (strongest) |

**Verdict: the architecture's split is confirmed.** Amplitude/texture
separates biomes at F = 40–130; the spacing family sits an order of
magnitude lower (F = 6–17) with 9–12% within-biome CVs. Spacing is not
*perfectly* identical across biomes (sandhills 119 m vs heathland 103 m
medians) but cannot carry identity. Nothing needs re-assignment; the
targets battery keeps spacing as a correctness band and the amplitude/
texture family as the identity axis.

## Macro-band organization (≥400 m), 10 tiles/biome

| biome | anisotropy (max/min directional power) | note |
|---|---|---|
| river_valley | **240.9** | the valley IS one oriented megaform |
| heathland | 28.2 | |
| hill_country | 27.4 | |
| piedmont | 26.2 | |
| sandhills | 24.7 | does NOT stand out at this band |
| great_plains | 22.1 | |

Two consequences:
1. **Class-conditioned relief** (implemented with this report): river
   valley's organization is a single megaform whose expression depends on
   where the window sits — descriptor draws now scale by the drawn window
   class (`class_relief_mult`, default 1.0). River_valley's multipliers are
   fitted from the margin campaign (valley-edge tiles fetched 2026-08-10).
2. **Stage-01 OQ0 (sandhills massifs) — deprioritized by data**: measured
   macro anisotropy does not single sandhills out; the massif impression
   likely lives in the S2 aeolian band + amplitude (both now fitted /
   review-tuned). Re-examine after S3 texture rather than extending the
   class vocabulary on impression.
