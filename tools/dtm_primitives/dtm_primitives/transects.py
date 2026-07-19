"""The transect engine — stations along a centerline, perpendicular sampling,
mask-aware discards, per-transect floor centering, and reach stacking into
robust median cross-sections. Shared by valley, bluff, and round-trip scoring
(the round-trip runs the IDENTICAL recenter+stack path on synthetic grids).

Sign convention: +d = LEFT of the local tangent (normal = (−ty, tx)), matching
Rust SpineHit.side > 0 ⇒ wall_grad_left.
"""

from __future__ import annotations

import warnings
from dataclasses import dataclass, field

import numpy as np

from . import frame


@dataclass
class Stations:
    s: np.ndarray        # arc position (m) along the centerline, (n,)
    xy: np.ndarray       # station points, (n, 2)
    tangent: np.ndarray  # unit tangents, (n, 2)
    normal: np.ndarray   # unit normals (+d side), (n, 2)


@dataclass
class Transects:
    st: Stations
    d: np.ndarray            # transect offsets (m), (m,)
    z: np.ndarray            # naturalized samples, (n, m); NaN invalid
    inpaint: np.ndarray      # (n, m) bool
    water: np.ndarray        # (n, m) bool
    road: np.ndarray         # (n, m) bool
    valid: np.ndarray        # (n, m) bool
    floor_offset: np.ndarray = field(default=None)  # per-station centering (m)
    z_floor: np.ndarray = field(default=None)       # per-station floor z (m)


@dataclass
class Reach:
    s0: float
    s1: float
    s_mid: float
    d: np.ndarray        # offsets (m)
    z_med: np.ndarray    # median ABSOLUTE-z cross-section (m)
    z_shape: np.ndarray  # median floor-referenced shape (z − z_floor), (m,)
    n_used: int
    station_idx: np.ndarray


def resample_polyline(pts: np.ndarray, step_m: float) -> np.ndarray:
    """Uniform arc-length resampling of an (N,2) polyline."""
    seg = np.diff(pts, axis=0)
    ln = np.hypot(seg[:, 0], seg[:, 1])
    keep = ln > 1e-9
    if not keep.all():
        pts = np.concatenate([pts[:1], pts[1:][keep]])
        seg = np.diff(pts, axis=0)
        ln = np.hypot(seg[:, 0], seg[:, 1])
    cum = np.concatenate([[0.0], np.cumsum(ln)])
    total = cum[-1]
    n = max(int(np.floor(total / step_m)) + 1, 2)
    s = np.linspace(0.0, total, n)
    x = np.interp(s, cum, pts[:, 0])
    y = np.interp(s, cum, pts[:, 1])
    return np.stack([x, y], axis=1)


def arc_length(pts: np.ndarray) -> float:
    seg = np.diff(pts, axis=0)
    return float(np.hypot(seg[:, 0], seg[:, 1]).sum())


def stations(centerline: np.ndarray, spacing_m: float) -> Stations:
    """Stations every spacing_m along an (N,2) polyline (assumed ~uniform
    5 m resampled). Tangents by central difference on the polyline."""
    seg = np.diff(centerline, axis=0)
    ln = np.hypot(seg[:, 0], seg[:, 1])
    cum = np.concatenate([[0.0], np.cumsum(ln)])
    total = cum[-1]
    s = np.arange(0.0, total + 1e-9, spacing_m)
    x = np.interp(s, cum, centerline[:, 0])
    y = np.interp(s, cum, centerline[:, 1])
    # central-difference tangents on the station points themselves
    tx = np.gradient(x)
    ty = np.gradient(y)
    tl = np.hypot(tx, ty)
    tl[tl == 0] = 1.0
    tangent = np.stack([tx / tl, ty / tl], axis=1)
    normal = np.stack([-tangent[:, 1], tangent[:, 0]], axis=1)  # +d = left
    return Stations(s=s, xy=np.stack([x, y], 1), tangent=tangent, normal=normal)


def sample(nat_nan: np.ndarray, masks: dict[str, np.ndarray], cell: float,
           st: Stations, halflen_m: float, step_m: float) -> Transects:
    """Sample the naturalized surface (row0=north, NaN=invalid) and masks
    along each station's normal. masks: {inpaint, water, road, valid} uint8."""
    d = np.arange(-halflen_m, halflen_m + step_m / 2, step_m)
    px = st.xy[:, 0][:, None] + st.normal[:, 0][:, None] * d[None, :]
    py = st.xy[:, 1][:, None] + st.normal[:, 1][:, None] * d[None, :]
    z = frame.bilinear_tif(nat_nan, px.ravel(), py.ravel(), cell).reshape(px.shape)

    def msk(name):
        arr = masks.get(name)
        if arr is None:
            return np.zeros(px.shape, bool)
        return frame.nearest_tif(arr, px.ravel(), py.ravel(), cell,
                                 fill=0).reshape(px.shape).astype(bool)

    return Transects(st=st, d=d, z=z, inpaint=msk("inpaint"),
                     water=msk("water"), road=msk("road"),
                     valid=msk("valid") if "valid" in masks
                     else ~np.isnan(z))


def recenter(tr: Transects, search_m: float, probe_min_m: float,
             hw_est_m: float | None = None,
             floor_tol_m: float = 0.25) -> Transects:
    """Per-transect floor centering: find the near-minimum PLATEAU of the
    median-5-filtered profile within |d| <= search_m and shift the d-axis so
    d=0 sits at its midpoint (argmin alone is ill-defined on flat floors —
    it picks a plateau edge). Records per-station floor z (p10 within the
    floor probe halfwidth)."""
    n, m = tr.z.shape
    zf = tr.z.copy()
    # median-5 along d (NaN-tolerant)
    pad = 2
    stackv = np.stack([np.roll(zf, k, axis=1) for k in range(-pad, pad + 1)])
    zmed = np.nanmedian(stackv, axis=0)
    zmed[:, :pad] = zf[:, :pad]
    zmed[:, -pad:] = zf[:, -pad:]

    win = np.abs(tr.d) <= search_m
    offsets = np.zeros(n)
    z_floor = np.full(n, np.nan)
    probe = max(probe_min_m, 0.5 * hw_est_m) if hw_est_m else probe_min_m
    zs = np.full_like(tr.z, np.nan)
    for i in range(n):
        zi = zmed[i]
        cand = np.where(win & np.isfinite(zi))[0]
        if cand.size == 0:
            zs[i] = tr.z[i]
            continue
        j = cand[np.argmin(zi[cand])]
        # contiguous near-min plateau around j -> center on its midpoint
        near = np.isfinite(zi) & (zi <= zi[j] + floor_tol_m)
        lo = j
        while lo - 1 >= 0 and near[lo - 1]:
            lo -= 1
        hi = j
        while hi + 1 < m and near[hi + 1]:
            hi += 1
        off = 0.5 * (tr.d[lo] + tr.d[hi])
        offsets[i] = off
        fin = np.isfinite(tr.z[i])
        if fin.sum() >= 4:
            zs[i] = np.interp(tr.d + off, tr.d[fin], tr.z[i][fin],
                              left=np.nan, right=np.nan)
            # NaN outside the original finite support
            lo, hi = tr.d[fin][0], tr.d[fin][-1]
            zs[i][(tr.d + off < lo) | (tr.d + off > hi)] = np.nan
        else:
            zs[i] = tr.z[i]
        near = np.abs(tr.d) <= probe
        vals = zs[i][near]
        vals = vals[np.isfinite(vals)]
        if vals.size:
            z_floor[i] = np.percentile(vals, 10)

    out = Transects(st=tr.st, d=tr.d, z=zs, inpaint=tr.inpaint,
                    water=tr.water, road=tr.road, valid=tr.valid,
                    floor_offset=offsets, z_floor=z_floor)
    return out


def keep_mask(tr: Transects, cfg: dict,
              foreign: list[tuple[np.ndarray, float]] = ()) -> np.ndarray:
    """Per-station keep flags. foreign = [(other centerline pts, top width)]."""
    n, m = tr.z.shape
    inner = np.abs(tr.d) <= tr.d.max() / 2
    floor_zone = np.abs(tr.d) <= cfg["road_floor_halfwidth_m"]
    valid_frac = np.isfinite(tr.z).mean(axis=1)
    inpaint_inner = (tr.inpaint & inner[None, :]).sum(1) / max(inner.sum(), 1)
    road_in_floor = (tr.road & floor_zone[None, :]).any(axis=1)
    keep = ((valid_frac >= cfg["min_valid_frac"])
            & (inpaint_inner <= cfg["inpaint_frac_max"])
            & ~road_in_floor)
    if foreign:
        from . import profiles
        for pts, topw in foreign:
            if len(pts) < 2:
                continue
            _u, dist, _sd, _t = profiles.polyline_project(
                pts, tr.st.xy[:, 0], tr.st.xy[:, 1])
            keep &= dist > (topw + cfg["foreign_corridor_pad_m"])
    return keep


def stack(tr: Transects, keep: np.ndarray, reach_len_m: float,
          min_per_reach: int) -> list[Reach]:
    """Group stations into contiguous reaches and median-stack the kept,
    floor-referenced transects; small reaches merge into the previous one."""
    s = tr.st.s
    total = s[-1] if len(s) else 0.0
    n_reach = max(int(round(total / reach_len_m)), 1)
    edges = np.linspace(0.0, total + 1e-9, n_reach + 1)
    groups: list[np.ndarray] = []
    for i in range(n_reach):
        idx = np.where((s >= edges[i]) & (s < edges[i + 1]) & keep
                       & np.isfinite(tr.z_floor))[0]
        if groups and len(idx) < min_per_reach:
            groups[-1] = np.concatenate([groups[-1], idx])
        else:
            groups.append(idx)
    reaches = []
    for idx in groups:
        if len(idx) < min_per_reach:
            continue
        shapes = tr.z[idx] - tr.z_floor[idx][:, None]
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)  # all-NaN columns
            z_shape = np.nanmedian(shapes, axis=0)
        z_med = z_shape + np.median(tr.z_floor[idx])
        reaches.append(Reach(
            s0=float(s[idx].min()), s1=float(s[idx].max()),
            s_mid=float(np.median(s[idx])), d=tr.d, z_med=z_med,
            z_shape=z_shape, n_used=int(len(idx)), station_idx=idx))
    return reaches
