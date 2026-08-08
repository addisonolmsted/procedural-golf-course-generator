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

## Spike executed (2026-08-07) — findings

Seven variants run (`spike.py`; two-level dictionary, mid band 64–400 m @ 8 m
+ fine band < 64 m @ 2 m, conditioned on lp_slope × tpi × relief_pos,
per-band budget closer). The story, in order:

| # | Variant | Metrics | Eye (P1) |
|---|---|---|---|
| 1 | PCA basis, independent coefficient draws | **8/8 in band** | **FAIL** — crumpled paper, no connected structure |
| 2 | PCA, correlated coefficient fields | 6/8 | FAIL — reinforces the patch lattice (checkerboard) |
| 3 | Quilt (real patches), Hann 50% overlap | 8/8* | plausible but mushy — blending cancels amplitude |
| 4 | Quilt, 25% overlap + amplitude floor | 6/8 | crisper, visible rectangular seams |
| 5 | Quilt, min-error-boundary cut | 6/8 | creases where the cut crosses height offsets |
| 6 | **Quilt, gradient-domain + Poisson + band-limit + spectral equalizer** | 5/8 (β −0.7% from band edge; range dead-on; curvature ~1.5× hot) | **best** — seamless, natural, terrain-like |
| B | Real mid band + dictionary fine band only | — | **nearly indistinguishable from real** |

### The four load-bearing findings

1. **Metrics alone cannot gate this stage.** Variant 1 passed every scalar
   while being obviously fake at a glance — the concrete proof of the
   03-success-indicators premise. Gate reports must always include renders.
2. **Drawn-coefficient synthesis is falsified** (variants 1–2, two distinct
   failure modes). Real structure requires real patch content.
3. **The fine band (< 64 m) works now** (variant B). The battleground is the
   mid band (64–400 m) — connected ravines and spur ridges. In production S2's
   skeleton supplies drainage organization there; the spike's lowpass base had
   none, so this is the dictionary's *hardest* case, not its typical one.
4. **Gradient-domain quilting is the right mechanism** (variant 6): compose
   patch gradients along min-error seams, Poisson-integrate, re-band-limit,
   true up the spectrum with a calibrated radial equalizer, close amplitude
   per band. Remaining gap is quantified and narrow: curvature ~1.5× hot
   (finest-scale sharpness) and β 0.7% shallow — tuning items (equalizer
   high-frequency rolloff, seam feather width), not unknowns.

### Recommendation

**GO**, with stage-03's mechanism revised from "PCA basis + coefficient
distributions" to **gradient-domain exemplar quilting** (+ equalizer +
closer). Asset implication: the dictionary ships real patches (i16-quantized)
rather than a basis — size grows, mitigable by patch count discipline;
re-estimate against the < 15 MB target during Phase F.

### G-SPIKE: PASSED (2026-08-07)

Reviewer verdict on the texture-isolation set: **"looks good, slightly
grainier than the real tiles."** The grain is the same defect the metrics
flagged (curvature ~1.5× hot, β slightly shallow) — the P2 protocol and the
battery agreeing on one artifact is the gate working as designed.

**Decision: proceed; grain is a named Phase-F tuning item**, with four levers
recorded in `docs/stages/stage-03-amplification.md`: replace the
`QUILT_STD_FLOOR` amplitude floor with amplitude *matching*; equalizer
high-frequency rolloff; a cover/smoothness conditioning axis; sub-patch fill
discipline. Deliberately NOT tuned now: part of the real tiles' smoothness is
classifier artifact (the canopy/despeckle caveats), and the Phase-E
sensitivity analysis establishes the true target — tuning grain against 6
tiles risks calibrating to lidar processing, not ground.

### The P2 blind test (for the record)

**Use `report/fineblind/`, not the original `report/blind/`.** The first
blind set was confounded: real crops carry dendritic drainage STRUCTURE,
which in production comes from S2's skeleton — a reviewer sorts real from
generated on evidence about the wrong stage. The reviewer said exactly this,
and was right.

`textures.py` (run after `spike.py`) builds the texture-isolation set:

- **`fineblind/fine_a..h.png`** — the fair blind test: identical macro + real
  mid band on both sides; only the < 64 m band differs. Sort real vs
  generated, then check `fineblind/answer_key.json`.
  **Gate: accuracy ≤ 75%.**
- `ramp_fine_…` / `ramp_mid_…` — each band pasted on a uniform 3.5% slope:
  pure fabric, no structure of either origin to key on (real left).
- `curv_fine_…` / `curv_mid_…` — Laplacian curvature maps; curvature is where
  the remaining metric miss lives, so this is the most honest visual.
- `tint_*` — hypsometric-tint × multi-azimuth hillshade composites of the
  full surfaces, for context (real left).

What the isolation renders already show: the real *fine* band is not
homogeneous fabric — it carries creek cuts, field/canopy boundaries, and road
traces (anthropogenic residue + hydrography). The spike quilted from unmasked
patches, so corpus hygiene at patch-extraction time (the develop masks
already exist per tile) is a named Phase-E item. The *mid* band isolated is
close: same scale and amplitude; real forms slightly more elongated/connected
(drainage-organized) — the gap S2's skeleton conditioning is expected to
close.

The original structure-confounded set is kept at `report/blind/` as the
record of the confound.
