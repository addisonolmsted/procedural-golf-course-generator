# G-SPIKE report — method: quilt

Train: t03520_07988, t03525_07978, t03528_07995, t03538_07999, t03544_07992  ·  Held out: **t03550_08002**  ·  seed 1

Residual std: real **3.910 m**, recon raw **3.094 m**; per-band budget closer mid ×1.167, fine ×1.574 (the stage-03 closer — in production the target is S0's drawn site-amplitude descriptor).

**Reading the table:** *shape* metrics are scale-invariant — the dictionary must earn them; *amplitude\** metrics are set by the closer and verify self-consistency only; *mixed* = shape given correct amplitude.

| metric (residual) | class | recon | held-out real | 6-tile band | in band |
|---|---|---|---|---|---|
| spectral_slope_beta | shape | 3.492 | 3.558 | [3.516, 4.185] | NO |
| variogram_range | shape | 195.5 | 193.4 | [173.6, 251.2] | **yes** |
| anisotropy_ratio | shape | 1 | 1.243 | [1, 1.391] | **yes** |
| mean_abs_profile_curv | mixed | 0.01115 | 0.007398 | [0.005657, 0.008054] | NO |
| plan_curv_p90 | mixed | 0.316 | 0.1775 | [0.1565, 0.2473] | NO |
| variogram_sill | amplitude* | 13.52 | 15.84 | [3.737, 18.42] | **yes** |
| rms_roughness_small | amplitude* | 0.8392 | 0.8917 | [0.4447, 0.9304] | **yes** |
| rms_roughness_mid | amplitude* | 2.097 | 2.283 | [1.098, 2.331] | **yes** |

## Seam check (PSD ratio recon/real at patch pitch)

- peak_at_32m: ratio 1.11 vs neighborhood 1.13 → excess 0.99 (OK)
- peak_at_128m: ratio 0.51 vs neighborhood 0.52 → excess 0.98 (OK)

## Blind A/B

`report/blind/blind_*.png` — sort each as real or generated;
then check `answer_key.json`. Gate: reviewer accuracy ≤ 75%.
