"""Tile-battery runner (workplan B6): any CGRID1 → one JSON row.

The promotion of smoke.py into a real tool: whole-tile battery + residual-only
battery (S3's fit-target space) + the golfability proxy, per tile, as JSONL.
The same measurement path serves real corpus tiles and generated surfaces —
one battery, one feature space.

Usage:
  python3 tools/spike/measure.py TILE.cgrid [TILE2.cgrid ...]   > rows.jsonl
  python3 tools/spike/measure.py --clean-tiles                  > corpus.jsonl
      (every non-excluded tile in macro_campaign/out/tiles)
"""
from __future__ import annotations

import json
import os
import sys

import numpy as np

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
for p in ("tools/macro_campaign", "tools/metrics", "tools/dtm_metrics",
          "tools/golf_proxy"):
    sys.path.insert(0, os.path.join(REPO, p))

import yaml  # noqa: E402
from macro_campaign import cgrid  # noqa: E402
from metrics import features  # noqa: E402
from dtm_metrics import surfaces  # noqa: E402
import proxy  # noqa: E402  (tools/golf_proxy)

L_MACRO = 400.0

RESIDUAL_KEYS = [
    "spectral_slope_beta", "variogram_range", "variogram_sill",
    "rms_roughness_small", "rms_roughness_mid",
    "mean_abs_profile_curv", "plan_curv_p90", "anisotropy_ratio",
]


def clean_tiles() -> list[str]:
    """Every tile on disk minus the exclusion cull, as absolute paths."""
    base = os.path.join(REPO, "tools/macro_campaign/out")
    ex = json.load(open(os.path.join(base, "exclude.json")))
    excluded = {(t["archetype"], t["tile"]) for t in ex["tiles"]}
    out = []
    tiles = os.path.join(base, "tiles")
    for arch in sorted(os.listdir(tiles)):
        d = os.path.join(tiles, arch)
        if not os.path.isdir(d):
            continue
        for f in sorted(os.listdir(d)):
            if f.endswith(".cgrid") and (arch, f[:-6]) not in excluded:
                out.append(os.path.join(d, f))
    return out


def measure(path: str, cfg, axes, thresholds) -> dict:
    z, (_ox, _oy, cell) = cgrid.read_f32(path)
    z = z.astype(np.float64)
    valid = np.isfinite(z)
    scalars, _ = features.compute(z, cell, valid, cfg, axes)
    S = surfaces.build(z, valid, cell, L=L_MACRO, tpi_window_m=L_MACRO)
    resid_scalars, _ = features.compute(S.s2, cell, valid, cfg, axes)
    row = {
        "tile": os.path.basename(path)[:-6],
        "archetype": os.path.basename(os.path.dirname(path)),
        "cell_m": cell,
        "valid_frac": float(valid.mean()),
        "residual_std_m": float(S.s2[valid].std()),
    }
    row.update({k: scalars.get(k) for k in features.SCALAR_COLUMNS})
    row.update({f"resid_{k}": resid_scalars.get(k) for k in RESIDUAL_KEYS})
    if thresholds is not None:
        row.update({f"proxy_{k}": v
                    for k, v in proxy.score_surface(z, cell, thresholds).items()})
    return row


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    paths = clean_tiles() if args == ["--clean-tiles"] else args
    cfg = yaml.safe_load(open(os.path.join(REPO, "tools/metrics/metrics/config.yaml")))
    axes = features.build_axes(cfg)
    try:
        thresholds = proxy.load_thresholds()
    except OSError:
        thresholds = None
    for p in paths:
        row = measure(p, cfg, axes, thresholds)
        print(json.dumps(row, default=float))
        print(f"  {row['archetype']}/{row['tile']}  beta={row['spectral_slope_beta']:.2f}  "
              f"resid_std={row['residual_std_m']:.2f} m  "
              f"proxy={row.get('proxy_score', float('nan')):.2f}",
              file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
