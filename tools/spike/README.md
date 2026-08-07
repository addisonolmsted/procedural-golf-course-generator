# M3.5 — the dictionary spike

The de-risk workspace for the project's central bet: **can a conditioned patch
dictionary fitted from real lidar residuals reproduce archetype texture?**
([../../docs/stages/stage-03-amplification.md](../../docs/stages/stage-03-amplification.md),
gate **G-SPIKE** in
[../../docs/03-success-indicators.md](../../docs/03-success-indicators.md).)

The spike runs on the **6 clean piedmont tiles already on disk** — no corpus
campaign, no new fetching:

```
t03520_07988  t03525_07978  t03528_07995
t03538_07999  t03544_07992  t03550_08002
```

(14 piedmont tiles exist under `../macro_campaign/out/tiles/piedmont/`; 8 are
excluded by `out/exclude.json`.)

## Status

**Prerequisite complete (2026-08-07).** The measurement chain the spike depends
on runs end-to-end — verified by `smoke.py`:

- `macro_campaign.cgrid` loads the tiles (1500² f32 @ 2 m),
- `dtm_metrics.surfaces.build` decomposes them (`s2 = s1 − lowpass` at
  L = 400 m) and emits the conditioning fields,
- `metrics.features.compute` scores both the whole tile and the residual
  (46/49 scalars finite; the 3 missing are the canopy/water family, which
  needs masks not passed here).

First real numbers for the fit-target space (tile `t03520_07988`, residual at
L = 400 m): std **1.95 m**, β **3.59**, variogram range **181 m**, sill
**3.7 m²**, short-lag RMS **0.44 m**. These are what the dictionary must
reproduce.

Note: `tools/metrics`' 15-test suite passes; the parkland-atlas cache turned
out to be **unnecessary** for the spike and for v2 generally — see
`MIGRATION.md`.

## The spike itself (next)

1. Extract residual patches + conditioning vectors from 5 of the 6 tiles
   (hold one out).
2. Bucket coarsely; fit a small PCA basis + coefficient distribution per
   bucket.
3. Reconstruct residual over a synthetic base (or the held-out tile's own
   lowpass — the cheapest honest test).
4. Score against the held-out tile: β, variogram range/sill, curvature pair;
   check the PSD for a peak at the patch pitch.
5. Render side-by-side hillshade crops for the blind A/B (P2, relaxed to
   ≤ 75% reviewer accuracy for the 6-tile spike).

**Go:** metrics in band and no visible tiling ⇒ proceed to the corpus
campaign (M4) with the design validated.
**No-go:** the fallback conversation (richer basis, quilting, or
reconsidering simulation) happens now, before ~300 tiles are fetched.
