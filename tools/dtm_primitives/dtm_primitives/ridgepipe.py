"""Ridge extraction: the ENTIRE valley pipeline run on the inverted surface
(crest = floor of −z; prominence = incision; flanks = walls — recentering,
reach stacking, the parametric cross-section fit and lateral centering reuse
verbatim), with geomorphon confirmation filtering the traced divides.

Mapping (inverted-surface valley record -> ridge record):
  floor_z0(−z) = −crest_z0;  incision = prominence;
  floor_halfwidth = crest_halfwidth;  wall_grad_l/r = flank_grad_l/r
  (left/right swap under inversion is IDENTITY: the transect d-axis is
  geometric, not elevation-signed).
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from . import geomorphons as G
from .valleypipe import BranchResult, extract_valleys


@dataclass
class RidgeRecord:
    centerline: np.ndarray          # (N,2) local coords
    crest_z0: float                 # at s=0 of the station arc
    fall_gradient: float            # crest drop per meter (may be ~0)
    prominence_med: float
    crest_hw_med: float
    flank_l: float
    flank_r: float
    crest_round: float
    length_m: float
    geomorphon_frac: float
    reaches: list = field(default_factory=list)


def _confirmed_points(cl: np.ndarray, cls10: np.ndarray, cell10: float,
                      radius_m: float) -> np.ndarray:
    """Per-centerline-point bool: within radius of a ridge-class cell."""
    from scipy.ndimage import distance_transform_edt
    ridge = G.ridge_mask(cls10)
    if not ridge.any():
        return np.zeros(len(cl), bool)
    d = distance_transform_edt(~ridge) * cell10
    nrows = cls10.shape[0]
    cols = np.clip((cl[:, 0] / cell10 - 0.5).round().astype(int), 0,
                   cls10.shape[1] - 1)
    rows = np.clip((nrows - 1) - (cl[:, 1] / cell10 - 0.5).round().astype(int),
                   0, nrows - 1)
    return d[rows, cols] <= radius_m


def _longest_run(ok: np.ndarray, close_gap: int = 5) -> tuple[int, int]:
    """(start, end) indices of the longest True run after closing small gaps
    (a divide trace crosses short saddles); (0, -1) when none."""
    from scipy.ndimage import binary_closing
    if not ok.any():
        return 0, -1
    closed = binary_closing(ok, structure=np.ones(close_gap, bool))
    best = (0, -1)
    i = 0
    n = len(closed)
    while i < n:
        if closed[i]:
            j = i
            while j + 1 < n and closed[j + 1]:
                j += 1
            if j - i > best[1] - best[0]:
                best = (i, j)
            i = j + 1
        else:
            i += 1
    return best


def extract_ridges(z_nan: np.ndarray, valid: np.ndarray, cell: float,
                   cfg: dict, cls10: np.ndarray | None = None,
                   cell10: float = 10.0
                   ) -> tuple[list[RidgeRecord], list[BranchResult]]:
    """Trace + fit ridges. cls10 = geomorphon class raster at 10 m (computed
    here if not supplied)."""
    rcfg = cfg["ridgefit"]
    if cls10 is None:
        k = int(round(cell10 / cell))
        ny, nx = z_nan.shape[0] // k, z_nan.shape[1] // k
        import warnings
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)
            z10 = np.nanmean(z_nan[: ny * k, : nx * k]
                             .reshape(ny, k, nx, k), axis=(1, 3))
        cls10 = G.classify(z10, cell10, lookup_m=rcfg["lookup_m"],
                           flat_deg=rcfg["flat_deg"])

    neg = np.where(valid, -z_nan, np.nan)
    results, _net = extract_valleys(neg, valid,
                                    {"valid": valid.astype(np.uint8)},
                                    cell, [], cfg)
    ridges: list[RidgeRecord] = []
    kept_results: list[BranchResult] = []
    from .transects import arc_length, resample_polyline
    from .valleyfit import weighted_median
    for r in results:
        if r.valley is None or r.longfit is None:
            continue
        # a divide trace runs past the real crest across plains/saddles —
        # TRIM to the longest geomorphon-confirmed run, then gate on length
        pts = resample_polyline(r.centerline, 5.0)
        okpts = _confirmed_points(pts, cls10, cell10,
                                  rcfg["confirm_radius_m"])
        i0, i1 = _longest_run(okpts)
        if i1 <= i0:
            continue
        trimmed = pts[i0:i1 + 1]
        length = arc_length(trimmed)
        if length < rcfg["min_len_m"]:
            continue
        s_lo, s_hi = i0 * 5.0, i1 * 5.0
        frac = float(okpts[i0:i1 + 1].mean())
        pairs = [(cf, rc) for cf, rc in zip(r.crossfits, r.reaches)
                 if cf.ok and s_lo <= rc.s_mid <= s_hi]
        if not pairs:
            continue
        n_used = [rc.n_used for _cf, rc in pairs]
        # record the OBSERVED crest maximum along the trimmed line (a
        # Theil-Sen line through an emphasis-tapered crest arch would
        # under-read it; the sampler consumes prominence + local base, so
        # the observed max is the honest absolute anchor)
        from .frame import bilinear_tif
        zline = bilinear_tif(z_nan, trimmed[:, 0], trimmed[:, 1], cell)
        import warnings as _w
        with _w.catch_warnings():
            _w.simplefilter("ignore", RuntimeWarning)
            z_max = float(np.nanmax(zline)) if np.isfinite(zline).any() \
                else float("nan")
        ridges.append(RidgeRecord(
            centerline=trimmed,
            crest_z0=z_max,
            fall_gradient=abs(r.longfit.fall_raw),
            prominence_med=weighted_median([cf.inc for cf, _ in pairs],
                                           n_used),
            crest_hw_med=weighted_median([cf.hw for cf, _ in pairs], n_used),
            flank_l=weighted_median([cf.ml for cf, _ in pairs], n_used),
            flank_r=weighted_median([cf.mr for cf, _ in pairs], n_used),
            crest_round=weighted_median([cf.r for cf, _ in pairs], n_used),
            length_m=length,
            geomorphon_frac=round(frac, 3),
            reaches=[{"s_mid": rc.s_mid, "prominence": cf.inc, "hw": cf.hw,
                      "fl": cf.ml, "fr": cf.mr, "ok": cf.ok}
                     for cf, rc in pairs],
        ))
        kept_results.append(r)
    return ridges, kept_results


def spacing_stats(ridges: list[RidgeRecord]) -> dict:
    """Median nearest-neighbor spacing between ridge centerlines."""
    if len(ridges) < 2:
        return {"ridge_spacing_m": float("nan"), "n_ridges": len(ridges)}
    from scipy.spatial import cKDTree
    dists = []
    for i, r in enumerate(ridges):
        others = np.concatenate([o.centerline for j, o in enumerate(ridges)
                                 if j != i])
        tree = cKDTree(others)
        d, _ = tree.query(r.centerline[::4])
        dists.append(float(np.median(d)))
    return {"ridge_spacing_m": float(np.median(dists)),
            "n_ridges": len(ridges)}
