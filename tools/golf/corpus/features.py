"""Per-point named features for the green-site fit (Phase 3).

Every feature is a named, physically meaningful quantity — the fit's
coefficients transcribe back into greens.py as plain terms, so nothing here
may be a black box. Reuses the siting/greens instruments wholesale; the only
new primitives are tpi60, plan/profile curvature, backdrop_far (80-160 m) and
the boundary distance.

Persistence is recomputed restricted to the course-polygon bbox (+pad):
siting.build_persistence is hardwired to the 750-2250 m core of a 3 km tile,
which corpus tiles are not.
"""

from __future__ import annotations

import json
import pathlib
import sys

import numpy as np
from scipy import ndimage

from . import config, geo, registry

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
for _p in ("golf", "macro_campaign", "dtm_primitives"):
    if str(_TOOLS / _p) not in sys.path:
        sys.path.insert(0, str(_TOOLS / _p))
import siting                                          # noqa: E402
import greens as G                                     # noqa: E402
from macro_campaign import cgrid                       # noqa: E402

FEATURES = [
    # gates / fit quality
    "pad_green", "confirm", "graded", "resid",
    # local ground
    "slope", "relief_pos", "tpi60", "tpi200", "subgrid_rough",
    "curv_plan", "curv_prof", "aspect_sin", "aspect_cos",
    # setting
    "surround", "room", "d_water_band", "d_boundary_norm",
    "pit", "peak", "saddle",
    # landform class at two scales (grouped one-hots, 'slope' as baseline)
    "cls240_flat", "cls240_convex", "cls240_concave",
    "cls80_flat", "cls80_convex", "cls80_concave",
    # approach
    "vis_best", "vis_mean", "recept_best", "backdrop_best", "backdrop_far",
    "aroom_best", "aroom_mean",
]

_CONVEX = {2, 3, 4, 5}    # peak, ridge, shoulder, spur (geomorphon 10-class)
_CONCAVE = {7, 8, 9, 10}  # hollow, footslope, valley, pit
_FLAT = {1}


def _persistence_bbox(f, i0, i1, j0, j1):
    pit = np.zeros(f.z8.shape)
    peak = np.zeros(f.z8.shape)
    zc = f.z8[i0:i1, j0:j1]
    fill, _ = siting._flood_fill_level(zc, f.cell)
    pit[i0:i1, j0:j1] = fill - zc
    fill_inv, _ = siting._flood_fill_level(-zc, f.cell)
    peak[i0:i1, j0:j1] = fill_inv + zc
    return pit, peak


def _curvatures(z8, cell):
    gy, gx = np.gradient(z8, cell)
    gyy, gyx = np.gradient(gy, cell)
    gxy, gxx = np.gradient(gx, cell)
    p, q = gx, gy
    denom = np.maximum(p * p + q * q, 1e-9)
    plan = -(gxx * q * q - 2 * gxy * p * q + gyy * p * p) / (
        denom * np.sqrt(1 + denom))
    prof = -(gxx * p * p + 2 * gxy * p * q + gyy * q * q) / (
        denom * (1 + denom) ** 1.5)
    return plan, prof


def _backdrop_far(f, y, x) -> float:
    """Best mean rise 80-160 m beyond the green over all bearings — the
    long-range wall behind, distinct from approach_table's 30-80 m term."""
    cell = f.cell
    bs = np.arange(1, int(160.0 / cell) + 1)
    zg = f.z8[y, x]
    best = -1e9
    for k in range(G.N_BEARINGS):
        th = 2.0 * np.pi * k / G.N_BEARINGS
        by = np.clip((y - np.sin(th) * bs).astype(int), 0, f.z8.shape[0] - 1)
        bx = np.clip((x - np.cos(th) * bs).astype(int), 0, f.z8.shape[1] - 1)
        m = bs * cell >= 80.0
        best = max(best, float((f.z8[by, bx] - zg)[m].mean()))
    return best


def course_arrays(rec: dict):
    """Load tile + instruments once per course. Returns None if no tile."""
    t = rec.get("tile")
    if not t or "path" not in t:
        return None
    z2, (_, _, cell2) = cgrid.read_f32(pathlib.Path(t["path"]))
    z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(np.float64)
    wp = pathlib.Path(t["path"]).with_suffix(".water.npy")
    wet2 = np.load(wp) if wp.exists() else np.zeros(z2.shape, bool)
    f = siting.build_fields(z2, cell2, wet2)
    m = siting.build_morphology(f)
    ring = np.array(rec["osm"]["ring_ll"], float)
    ring_m = geo.ll_to_m(ring[:, 0], ring[:, 1], rec["utm"]["zone"])
    # to tile-local metres
    ring_loc = ring_m - np.array([t["n0"], t["e0"]])
    pad = int(200.0 / f.cell)
    i0 = max(0, int(ring_loc[:, 0].min() / f.cell) - pad)
    i1 = min(f.z8.shape[0], int(ring_loc[:, 0].max() / f.cell) + pad)
    j0 = max(0, int(ring_loc[:, 1].min() / f.cell) - pad)
    j1 = min(f.z8.shape[1], int(ring_loc[:, 1].max() / f.cell) + pad)
    pit, peak = _persistence_bbox(f, i0, i1, j0, j1)
    tpi60 = f.z8 - ndimage.uniform_filter(f.z8, int(round(60.0 / f.cell)) | 1)
    curv_plan, curv_prof = _curvatures(f.z8, f.cell)
    sqrt_area = float(np.sqrt(rec["osm"]["area_m2"]))
    return dict(z2=z2, cell2=cell2, wet2=wet2, f=f, m=m, pit=pit, peak=peak,
                tpi60=tpi60, curv_plan=curv_plan, curv_prof=curv_prof,
                ring_loc=ring_loc, sqrt_area=sqrt_area,
                bbox8=(i0, i1, j0, j1))


def point_features(arr: dict, ym: float, xm: float) -> dict | None:
    """Feature dict for one tile-local metre point, or None if off-grid."""
    f = arr["f"]
    y, x = int(ym / f.cell), int(xm / f.cell)
    if not (0 <= y < f.z8.shape[0] and 0 <= x < f.z8.shape[1]):
        return None
    ok, grad, resid, build = G.confirm_2m(arr["z2"], arr["cell2"],
                                          arr["wet2"], ym, xm)
    tab = G.approach_table(f, y, x, grad)
    gy, gx = grad
    gmag = float(np.hypot(gy, gx))
    c240 = int(arr["m"].cls240[y, x])
    c80 = int(arr["m"].cls80[y, x])
    d_bnd = geo.dist_to_ring(np.array([ym, xm]), arr["ring_loc"])
    inside = geo.point_in_poly(np.array([ym, xm]), arr["ring_loc"])
    return dict(
        pad_green=float(f.pad_green[y, x]),
        confirm=float(ok), graded=float(build == "graded"),
        resid=float(resid),
        slope=float(f.slope[y, x]),
        relief_pos=float(f.relief_pos[y, x]),
        tpi60=float(arr["tpi60"][y, x]), tpi200=float(f.tpi200[y, x]),
        subgrid_rough=float(f.subgrid_rough[y, x]),
        curv_plan=float(np.clip(arr["curv_plan"][y, x], -0.05, 0.05)),
        curv_prof=float(np.clip(arr["curv_prof"][y, x], -0.05, 0.05)),
        aspect_sin=float(gy / gmag) if gmag > 1e-6 else 0.0,
        aspect_cos=float(gx / gmag) if gmag > 1e-6 else 0.0,
        surround=G.surround_relief(f.z8, f.cell, y, x),
        room=float(min(f.room[y, x], 60.0)),
        d_water_band=float(min(f.d_water[y, x], 400.0)),
        d_boundary_norm=float((d_bnd if inside else -d_bnd) / arr["sqrt_area"]),
        pit=float(np.log1p(min(arr["pit"][y, x], 10.0))),
        peak=float(np.log1p(min(arr["peak"][y, x], 20.0))),
        saddle=float(arr["m"].saddle[y, x]) if hasattr(arr["m"], "saddle") else 0.0,
        cls240_flat=float(c240 in _FLAT),
        cls240_convex=float(c240 in _CONVEX),
        cls240_concave=float(c240 in _CONCAVE),
        cls80_flat=float(c80 in _FLAT),
        cls80_convex=float(c80 in _CONVEX),
        cls80_concave=float(c80 in _CONCAVE),
        vis_best=float(tab[:, 0].max()), vis_mean=float(tab[:, 0].mean()),
        recept_best=float(np.clip(tab[:, 1], -0.05, 0.05).max() / 0.05),
        backdrop_best=float(np.clip(tab[:, 2], 0, 8).max() / 8.0),
        backdrop_far=float(np.clip(_backdrop_far(f, y, x), -10, 10)),
        aroom_best=float(tab[:, 3].max()), aroom_mean=float(tab[:, 3].mean()),
    )


def greens_local(rec: dict) -> np.ndarray:
    t = rec["tile"]
    return np.array([[g["yx_utm"][0] - t["n0"], g["yx_utm"][1] - t["e0"]]
                     for g in rec["greens"]], float)


def extract_course(rec: dict, controls_yx: np.ndarray) -> list[dict]:
    """Rows for one course: real greens (label 1) + controls (label 0)."""
    arr = course_arrays(rec)
    if arr is None:
        return []
    rows = []
    for label, pts in ((1, greens_local(rec)), (0, controls_yx)):
        for (ym, xm) in pts:
            ft = point_features(arr, ym, xm)
            if ft is None:
                continue
            ft.update(label=label, course_id=rec["course_id"],
                      region=rec["region_tag"],
                      fame=rec.get("fame_tier", 1))
            rows.append(ft)
    return rows
