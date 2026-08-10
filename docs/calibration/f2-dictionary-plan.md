# F2 — the patch dictionary: plan (2026-08-10)

The realism bet, productionized from the spike's proven mechanism. One
sentence: **harvest band-split residual patches from the 203-tile corpus,
curate them into conditioning buckets, bake to a small deterministic
asset, and let S3 quilt patch GRADIENTS along min-error seams into the
skeleton's surface.**

## What the spike already proved (tools/spike)

- Drawn-coefficient PCA synthesis is falsified (passed 8/8 metrics while
  visually fake) — the dictionary stores REAL PATCHES, not a basis.
- The working mechanism: quilt patch gradients (not heights) along
  min-error seams → Poisson-integrate (DCT) → re-band-limit → radial
  spectral equalizer (clip 4.0) → per-band amplitude closer.
- Two levels: mid band 64–400 m at 8 m (256 m patches), fine band < 64 m
  at 2 m (64 m patches).

## Inputs (all staged by E5/E6)

- `out/extract_v2/<biome>/<tile>.npz`: `mid8`, `fine` (f16 residuals),
  `cond8` (slope, TPI, relief-pos, aspect, dist-to-channel), `clean`
  (valid ∧ ¬lake ∧ ¬developed ∧ ¬agri — the re-mask this plan waited on).
- Exemplar pools per biome in the envelope (tile ids per biome).

## Build steps

1. **Harvest** (Python, `tools/dictionary/`): per biome, per level, slide
   the patch window (50% overlap) over each kept tile; keep patches whose
   footprint is ≥ 85% clean-mask; store the patch, its conditioning
   vector (mean over footprint), source tile id, and location.
2. **Bucket**: conditioning quantized on the corpus's own quantiles —
   slope × TPI × relief-pos × log-dist (coarse: ~4×3×3×3 per biome).
   D3 measured 100% generated-conditioning coverage, so every runtime
   lookup will land in a real bucket.
3. **Curate per bucket**: cap ~64 patches; dedup by normalized cross-
   correlation (drop > 0.92 similarity); require ≥ 5 distinct source
   tiles per bucket (the workplan's diversity floor) — buckets that fail
   borrow from conditioning-adjacent buckets with a recorded provenance
   flag; record per-bucket amplitude stats + radial-PSD equalizer target.
4. **Bake** (`course-calibration` or Python → Rust-readable): i16-
   quantized patch gradients, zstd container, per-biome index, blake3
   fingerprint. Budget target < 40 MB total (re-estimated from the < 15 MB
   basis-era figure; patches are bigger — measure and report at bake).
5. **Loader** (`course-amplify`): mmap/lazy, fingerprint interlock like
   the envelope's.

## Runtime contract (S3, next phase — recorded here for shape)

Position-seeded patch choice (never sequence-seeded: layout order must
not matter), conditioning lookup from the skeleton's own fields
(flow_distance_norm, hillslope_position, lp-slope, TPI), overlap-add
quilting with min-error seams on GRADIENTS, near-channel amplitude taper
(the skeleton's carved profile must survive), Poisson integrate, re-band-
limit, equalize, close amplitude to the biome band.

## QA (F3, gates the asset)

- Held-out reconstruction: rebuild each biome's held-out tiles' residual
  bands from the dictionary; score on the E6 discriminants (fine_std,
  mid_std, slope_median, local reliefs) — must land inside the real
  per-biome bands.
- Energy distance vs the v1 baseline (2.35) on held-out tiles.
- Seam detector: variogram range + spectral slope anomalies.
- QUARANTINE RESOLVED (2026-08-10, post-re-mask): with leveled ground
  excluded (rv clean fraction 56%), river_valley's clean-masked fine_std
  is 0.20 m vs 0.19 valid-masked - the low texture was REAL, not
  contamination: natural bottomland floor is genuinely quiet. Clean-
  masked per-biome fine_std targets (medians): piedmont 0.46, sandhills
  0.61, great_plains 0.36, river_valley 0.20, hill_country 0.73,
  heathland 0.32 m. The harvester uses clean cells by construction.

## Re-mask outcome (2026-08-10, measured)

OSM landuse coverage is EMPTY across the rural corpus (zero landuse tags
of any kind on the Louisiana bottomland bbox tested) - the OSM agri mask
is kept but cannot be relied on. The workhorse is a PHYSICAL screen:
local fine-band sigma (30 m window) < 4 cm => leveled ground, excluded
from the clean mask (fields measure 3-9 cm vs >= 15 cm p25 natural; the
threshold keeps >= 95% of natural cells). Additionally the harvester's
curation step must carry a per-patch rectilinearity rejector (axis-
aligned line-energy check) for field-boundary berms that survive the
smoothness screen.

## Order of work

1. (running) agri masks for all tiles → re-extract v2 (version 2) →
   recompute rv/heathland texture stats; lift the rv quarantine.
2. Harvester + bucketing + curation report (patch counts, bucket
   coverage, diversity failures per biome).
3. Bake + fingerprint + loader skeleton.
4. F3 held-out QA report → dictionary certified → S3 begins.
