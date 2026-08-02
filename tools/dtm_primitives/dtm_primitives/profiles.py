"""Exact numpy mirrors of the Stage-2 (golf-landform) surface forms, so that
least-squares parameters map 1:1 onto MacroConfig fields and the cross-language
frame check can compare grids analytically.

Mirrored from golf-landform/src/ease.rs, valley.rs, bluff.rs, lib.rs (Tilt) and
golf-core/src/spline.rs (Profile). NOTE ease.rs's smoothstep comment says
"quintic" but the implementation is the CUBIC t²(3−2t) — we mirror the
implementation.
"""

from __future__ import annotations

import numpy as np


# ---- ease.rs ---------------------------------------------------------------

def smoothstep(e0, e1, x):
    t = np.clip((np.asarray(x, float) - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def smin(a, b, k: float):
    """Polynomial smooth minimum; exact min once |a−b| >= k; C1."""
    a = np.asarray(a, float)
    b = np.asarray(b, float)
    if k <= 0.0:
        return np.minimum(a, b)
    h = np.clip(0.5 + 0.5 * (b - a) / k, 0.0, 1.0)
    return b + (a - b) * h - k * h * (1.0 - h)


def smax(a, b, k: float):
    return -smin(-np.asarray(a, float), -np.asarray(b, float), k)


def ramp(x, m, r):
    """C1 rounded ramp: 0 for x<=0, slope m beyond r, quadratic ease between.
    Vectorized in x; m may be an array (per-sample wall side)."""
    x = np.asarray(x, float)
    m = np.asarray(m, float)
    r = float(r)
    if r <= 0.0:
        return np.where(x <= 0.0, 0.0, m * (x - 0.5 * max(r, 0.0)))
    return np.where(
        x <= 0.0, 0.0,
        np.where(x >= r, m * (x - 0.5 * r), 0.5 * m * x * x / r))


def eased_step(x, w):
    return smoothstep(-0.5 * w, 0.5 * w, x)


def end_taper(s, taper: float):
    if taper <= 0.0:
        return np.ones_like(np.asarray(s, float))
    t = min(taper, 0.5)
    return smoothstep(0.0, t, s) * smoothstep(1.0, 1.0 - t, s)


# ---- lib.rs Tilt -----------------------------------------------------------

def tilt_eval(x, y, base: float, extent: float, gx: float, gy: float,
              curve: float):
    c = 0.5 * extent
    dx = np.asarray(x, float) - c
    dy = np.asarray(y, float) - c
    r2c = 2.0 * c * c
    return base + gx * dx + gy * dy + curve * (1.0 - (dx * dx + dy * dy) / r2c)


# ---- golf-core Profile -----------------------------------------------------

def profile_sample(knots: list[tuple[float, float]], u):
    """Clamped piecewise-linear sample, mirroring Profile::sample."""
    u = np.asarray(u, float)
    ss = np.array([k[0] for k in knots], float)
    vs = np.array([k[1] for k in knots], float)
    return np.interp(u, ss, vs)  # np.interp clamps outside — same semantics


# ---- polyline projection (Spine::project for a polyline) -------------------

def polyline_project(pts: np.ndarray, qx, qy):
    """Project query points onto a polyline (N,2). Returns (u, d, side):
    u = normalized arc of the closest point, d = Euclidean distance,
    side = +1 left of local tangent, −1 right (0 at zero distance).
    Mirrors Spine::project semantics for the analytic frame check and for
    transect bookkeeping (exactness only needed for straight test paths)."""
    qx = np.atleast_1d(np.asarray(qx, float))
    qy = np.atleast_1d(np.asarray(qy, float))
    a = pts[:-1]
    b = pts[1:]
    ab = b - a
    seg_len = np.hypot(ab[:, 0], ab[:, 1])
    keep = seg_len > 0
    a, b, ab, seg_len = a[keep], b[keep], ab[keep], seg_len[keep]
    cum = np.concatenate([[0.0], np.cumsum(seg_len)])
    total = cum[-1]
    # (Q, S) pairwise
    px = qx[:, None] - a[None, :, 0]
    py = qy[:, None] - a[None, :, 1]
    t = np.clip((px * ab[None, :, 0] + py * ab[None, :, 1])
                / (seg_len[None, :] ** 2), 0.0, 1.0)
    cx = a[None, :, 0] + t * ab[None, :, 0]
    cy = a[None, :, 1] + t * ab[None, :, 1]
    d2 = (qx[:, None] - cx) ** 2 + (qy[:, None] - cy) ** 2
    j = np.argmin(d2, axis=1)
    rows = np.arange(len(qx))
    tj = t[rows, j]
    d = np.sqrt(d2[rows, j])
    u = (cum[j] + tj * seg_len[j]) / total
    # side: sign of cross(tangent, q − closest)
    tx = ab[j, 0] / seg_len[j]
    ty = ab[j, 1] / seg_len[j]
    vx = qx - cx[rows, j]
    vy = qy - cy[rows, j]
    side = np.sign(tx * vy - ty * vx)
    return u, d, side, total


# ---- valley / bluff observable models --------------------------------------

def valley_surface(u, d, side, arc_total, floor_z0: float, grad: float,
                   hw_knots, wl: float, wr: float, round_m: float):
    """The swept valley surface (valley.rs ResolvedValley::surface), given a
    projection: floor_z(u) + ramp(d − hw(u), wall_side, round)."""
    floor_z = floor_z0 - grad * np.clip(u, 0.0, 1.0) * arc_total
    hw = profile_sample(hw_knots, u)
    wall = np.where(side > 0.0, wl, wr)
    return floor_z + ramp(d - hw, wall, round_m)


def valley_cross_model(d, zf, inc, gl, gr, hw, ml, mr, r, k):
    """The per-reach observable cross-section used for fitting:
    smin(T(d), zf + ramp(|d|−hw, m_side, r), k) with hinged upland trend
    T(d) = (zf+inc) + gl·min(d,0) + gr·max(d,0). +d = left ⇒ mL for d>0."""
    d = np.asarray(d, float)
    t = (zf + inc) + gl * np.minimum(d, 0.0) + gr * np.maximum(d, 0.0)
    wall = np.where(d > 0.0, ml, mr)
    v = zf + ramp(np.abs(d) - hw, wall, r)
    return smin(t, v, k)


def bluff_cross_model(d, base, g, h, face_grad, sign):
    """Observable bluff cross-section: base + g·d + h·eased_step(sign·d, w),
    w = h/face_grad (bluff.rs apply, taper handled by station selection)."""
    d = np.asarray(d, float)
    w = max(abs(h) / max(face_grad, 0.02), 1.0)
    return base + g * d + h * eased_step(sign * d, w)
