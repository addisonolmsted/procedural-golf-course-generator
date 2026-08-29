"""Geometry primitives: lat/lon -> course-local UTM metres, polygon measures.

Reuses macro_campaign.utm.to_utm (sub-metre, no pyproj). All polygon math is
plain numpy on (N,2) [y,x]-metre arrays, row 0 = south convention.
"""

from __future__ import annotations

import pathlib
import sys

import numpy as np

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
if str(_TOOLS / "macro_campaign") not in sys.path:
    sys.path.insert(0, str(_TOOLS / "macro_campaign"))
from macro_campaign.utm import to_utm  # noqa: E402


def utm_zone(lon: float) -> int:
    return int((lon + 180.0) // 6.0) + 1


def ll_to_m(lat, lon, zone: int) -> np.ndarray:
    """(lat, lon) arrays -> (N,2) [northing, easting] metres."""
    lat = np.atleast_1d(np.asarray(lat, float))
    lon = np.atleast_1d(np.asarray(lon, float))
    pts = np.array([to_utm(a, o, zone) for a, o in zip(lat, lon)])
    return np.stack([pts[:, 1], pts[:, 0]], 1)  # -> [y=northing, x=easting]


def shoelace(poly: np.ndarray) -> tuple[float, np.ndarray]:
    """Signed area (m^2) and AREA centroid of a closed-or-open ring.

    The area centroid, not the vertex mean — a vertex mean drifts toward
    densely-noded arcs (fetch_real_greens.py's known bias)."""
    p = np.asarray(poly, float)
    if not np.allclose(p[0], p[-1]):
        p = np.vstack([p, p[0]])
    y, x = p[:, 0], p[:, 1]
    cross = x[:-1] * y[1:] - x[1:] * y[:-1]
    a = 0.5 * cross.sum()
    if abs(a) < 1e-9:
        return 0.0, p[:-1].mean(0)
    cy = ((y[:-1] + y[1:]) * cross).sum() / (6.0 * a)
    cx = ((x[:-1] + x[1:]) * cross).sum() / (6.0 * a)
    return a, np.array([cy, cx])


def point_in_poly(pt: np.ndarray, poly: np.ndarray) -> bool:
    """Ray-cast on [y,x] metres."""
    y, x = pt
    p = np.asarray(poly, float)
    y0, x0 = p[:, 0], p[:, 1]
    y1, x1 = np.roll(y0, -1), np.roll(x0, -1)
    cond = (y0 > y) != (y1 > y)
    with np.errstate(divide="ignore", invalid="ignore"):
        xs = x0 + (y - y0) / (y1 - y0) * (x1 - x0)
    return bool(np.count_nonzero(cond & (x < xs)) % 2)


def dist_to_ring(pt: np.ndarray, poly: np.ndarray) -> float:
    """Distance to the polygon boundary (segment-wise, exact)."""
    p = np.asarray(poly, float)
    a = p
    b = np.roll(p, -1, axis=0)
    ab = b - a
    ap = pt[None, :] - a
    denom = (ab * ab).sum(1)
    t = np.clip((ap * ab).sum(1) / np.maximum(denom, 1e-12), 0.0, 1.0)
    proj = a + t[:, None] * ab
    return float(np.hypot(*(pt[None, :] - proj).T.tolist()).min()) if False else \
        float(np.sqrt(((pt[None, :] - proj) ** 2).sum(1)).min())


def pca_box(poly: np.ndarray) -> tuple[float, float, float]:
    """(long_m, short_m, axis_deg) of the PCA-oriented bounding box."""
    p = np.asarray(poly, float)
    c = p.mean(0)
    d = p - c
    cov = d.T @ d / len(d)
    w, v = np.linalg.eigh(cov)
    ax = v[:, np.argmax(w)]
    ang = float(np.degrees(np.arctan2(ax[0], ax[1])) % 180.0)
    r = d @ v
    ext = r.max(0) - r.min(0)
    long_m, short_m = float(max(ext)), float(min(ext))
    return long_m, short_m, ang


def compactness(area_m2: float, poly: np.ndarray) -> float:
    """Polsby-Popper 4*pi*A/P^2 in (0, 1], 1 = circle."""
    p = np.asarray(poly, float)
    if not np.allclose(p[0], p[-1]):
        p = np.vstack([p, p[0]])
    per = float(np.sqrt((np.diff(p, axis=0) ** 2).sum(1)).sum())
    return float(4.0 * np.pi * abs(area_m2) / max(per * per, 1e-9))
