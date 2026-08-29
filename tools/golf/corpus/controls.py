"""Matched control points: the negatives of the green-site fit.

K controls per real green, uniform inside the SAME course polygon (never the
greens bbox — score_real_greens.py's bbox controls under-sample exactly the
boundary-adjacent ground), water-rejected, and MATCHED on d_boundary_norm:
each control's normalized boundary distance must land within +/-0.03 of its
green's. Without the matching the fit learns the trivial "greens are
interior"; with it, d_boundary is uninformative in the primary fit and every
other coefficient measures terrain character. An UNMATCHED control set is
sampled alongside for the diagnostic fit (B) that feeds the window study.

Deterministic: rng seeded from the course id.
"""

from __future__ import annotations

import hashlib

import numpy as np

from . import geo

K_MATCHED = 4
K_UNMATCHED = 2
MATCH_TOL = 0.03      # in d_boundary_norm units (dist / sqrt(course area))
MAX_TRIES = 4000


def _rng(course_id: str) -> np.random.Generator:
    seed = int(hashlib.sha256(course_id.encode()).hexdigest()[:8], 16)
    return np.random.default_rng(seed)


def _sample_inside(rng, ring_loc, wet2, cell2, n, want_d=None, sqrt_area=None):
    """Uniform points inside the polygon (tile-local metres); optionally
    matched to a target d_boundary_norm within MATCH_TOL."""
    lo = ring_loc.min(0)
    hi = ring_loc.max(0)
    out = []
    tries = 0
    while len(out) < n and tries < MAX_TRIES:
        tries += 1
        p = lo + rng.random(2) * (hi - lo)
        if not geo.point_in_poly(p, ring_loc):
            continue
        yi, xi = int(p[0] / cell2), int(p[1] / cell2)
        if not (0 <= yi < wet2.shape[0] and 0 <= xi < wet2.shape[1]):
            continue
        if wet2[yi, xi]:
            continue
        if want_d is not None:
            d = geo.dist_to_ring(p, ring_loc) / sqrt_area
            if abs(d - want_d) > MATCH_TOL:
                continue
        out.append(p)
    return out


def controls_for(rec: dict, wet2: np.ndarray, cell2: float
                 ) -> tuple[np.ndarray, np.ndarray]:
    """(matched, unmatched) control points in tile-local metres."""
    t = rec["tile"]
    ring = np.array(rec["osm"]["ring_ll"], float)
    ring_m = geo.ll_to_m(ring[:, 0], ring[:, 1], rec["utm"]["zone"])
    ring_loc = ring_m - np.array([t["n0"], t["e0"]])
    sqrt_area = float(np.sqrt(rec["osm"]["area_m2"]))
    rng = _rng(rec["course_id"])
    matched, unmatched = [], []
    for g in rec["greens"]:
        p = np.array([g["yx_utm"][0] - t["n0"], g["yx_utm"][1] - t["e0"]])
        want = geo.dist_to_ring(p, ring_loc) / sqrt_area
        got = _sample_inside(rng, ring_loc, wet2, cell2, K_MATCHED,
                             want_d=want, sqrt_area=sqrt_area)
        if len(got) < K_MATCHED:   # tight polygons: relax to unmatched fill
            got += _sample_inside(rng, ring_loc, wet2, cell2,
                                  K_MATCHED - len(got))
        matched += got
        unmatched += _sample_inside(rng, ring_loc, wet2, cell2, K_UNMATCHED)
    return (np.array(matched) if matched else np.zeros((0, 2)),
            np.array(unmatched) if unmatched else np.zeros((0, 2)))
