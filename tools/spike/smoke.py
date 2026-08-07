"""M3.5 spike prerequisite — end-to-end smoke test of the measurement chain.

Proves, on a real clean piedmont tile already on disk, that the stack the
dictionary spike depends on actually runs:

    CGRID1 load  ->  surfaces.build (residual decomposition + conditioning
    fields)      ->  metrics.features.compute (the 49-scalar battery),
    on both the whole tile and the residual alone.

Run:  python3 tools/spike/smoke.py [tile.cgrid]

Passing this is gate zero for G-SPIKE (docs/03-success-indicators.md): if this
script fails, nothing downstream of it is worth debugging.
"""
import os
import sys

import numpy as np

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
sys.path.insert(0, os.path.join(REPO, "tools", "macro_campaign"))
sys.path.insert(0, os.path.join(REPO, "tools", "metrics"))
sys.path.insert(0, os.path.join(REPO, "tools", "dtm_metrics"))

import yaml  # noqa: E402
from macro_campaign import cgrid  # noqa: E402  (CGRID1 codec)
from metrics import features  # noqa: E402  (the battery)
from dtm_metrics import surfaces  # noqa: E402  (residual decomposition)

DEFAULT_TILE = os.path.join(
    REPO, "tools/macro_campaign/out/tiles/piedmont/t03520_07988.cgrid")

# The S1/S2 band boundary: everything below L is the residual the dictionary
# must reproduce (docs/stages/stage-03-amplification.md).
L_M = 400.0

KEY_WHOLE = [
    "relief_p95_p5", "slope_median", "spectral_slope_beta", "variogram_range",
    "variogram_sill", "mean_abs_profile_curv", "anisotropy_ratio",
    "drainage_density", "hypsometric_integral",
]
KEY_RESIDUAL = [
    "spectral_slope_beta", "variogram_range", "variogram_sill",
    "rms_roughness_small", "anisotropy_ratio",
]


def main() -> int:
    tile = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_TILE
    if not os.path.exists(tile):
        print(f"FAIL: tile not found: {tile}")
        print("(the corpus is gitignored/local — fetch via macro_campaign)")
        return 1

    # 1. Load a real tile.
    data, (_ox, _oy, cell) = cgrid.read_f32(tile)
    valid = np.isfinite(data)
    print(f"tile: {os.path.basename(tile)}  shape={data.shape}  cell={cell} m  "
          f"z-range=[{np.nanmin(data):.1f}, {np.nanmax(data):.1f}] m  "
          f"valid={valid.mean():.3f}")

    # 2. Residual decomposition + the conditioning fields.
    S = surfaces.build(data.astype(np.float64), valid, cell, L=L_M,
                       tpi_window_m=L_M)
    res = S.s2[valid]
    print(f"surfaces.build OK  L={S.L} m")
    print(f"  residual: std={res.std():.3f} m  "
          f"p2-p98=[{np.percentile(res, 2):.2f}, {np.percentile(res, 98):.2f}] m")
    print(f"  conditioning: lp_slope mean={S.lp_slope[valid].mean():.4f}  "
          f"tpi_sigma={S.tpi_sigma:.3f} m")

    # 3. The battery on the whole tile.
    cfg = yaml.safe_load(open(os.path.join(REPO, "tools/metrics/metrics/config.yaml")))
    axes = features.build_axes(cfg)
    scalars, dists = features.compute(data.astype(np.float64), cell, valid, cfg, axes)
    n_finite = sum(1 for v in scalars.values()
                   if v is not None and np.isfinite(v))
    print(f"features.compute OK: {len(scalars)} scalars ({n_finite} finite), "
          f"{len(dists)} distribution vectors")
    for k in KEY_WHOLE:
        v = scalars.get(k)
        ok = v is not None and np.isfinite(v)
        print(f"  {k:24s} = {v:.4g}" if ok else f"  {k:24s} = MISSING/NaN")

    # 4. The battery on the residual — the space the dictionary is fitted in.
    scalars_r, _ = features.compute(S.s2, cell, valid, cfg, axes)
    print("residual-only battery (S3's fit-target space):")
    bad = []
    for k in KEY_RESIDUAL:
        v = scalars_r.get(k)
        ok = v is not None and np.isfinite(v)
        print(f"  {k:24s} = {v:.4g}" if ok else f"  {k:24s} = MISSING/NaN")
        if not ok:
            bad.append(k)
    if bad:
        print(f"FAIL: non-finite residual metrics: {bad}")
        return 1

    print("SMOKE TEST PASSED — the spike's measurement chain runs end-to-end.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
