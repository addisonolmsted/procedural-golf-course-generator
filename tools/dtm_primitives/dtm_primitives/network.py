"""Drainage-network extraction: coarse flow accumulation (frozen v1 kernels),
trunk + tributary tracing, OSM waterway seeding/validation, and centerline
smoothing. OSM only SELECTS among flow-derived candidates — it never invents
geometry.

Branch paths are ordered upstream -> downstream (s = 0 at the upstream end,
matching the Valley floor_z0 convention).
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from scipy.ndimage import gaussian_filter1d
from scipy.spatial import cKDTree

from metrics import core as v1core

from . import frame

SIGMA_PER_L = 0.18739  # half-amplitude Gaussian cutoff (Stage-1 constant)


@dataclass
class Flow:
    zb: np.ndarray        # block-mean filled-input surface (row0=north)
    vb: np.ndarray        # block validity
    cell_b: float
    filled: np.ndarray    # depression-filled surface
    area_m2: np.ndarray   # D8 accumulation x cell area
    rec: np.ndarray       # linear receiver index, -1 = pit/outlet
    donors_idx: np.ndarray    # CSR: donor cell linear indices
    donors_start: np.ndarray  # CSR: start offsets per cell


@dataclass
class Branch:
    path_rc: np.ndarray       # (N,2) block (row, col), upstream->downstream
    path_xy: np.ndarray       # (N,2) local coords of block centers
    parent: int | None        # branch index it joins (None = trunk)
    junction_frac: float      # arc fraction along the parent at the mouth
    mouth_area_m2: float
    osm: dict = field(default_factory=dict)


@dataclass
class Network:
    branches: list[Branch]
    cell_b: float
    osm_lines: list[np.ndarray]


def coarse_flow(nat_nan: np.ndarray, valid: np.ndarray, cell: float,
                cfg: dict) -> Flow:
    k = max(int(round(cfg["block_m"] / cell)), 1)
    zfull = v1core._fill_nearest(nat_nan, valid)
    zb = _block_mean(zfull, k)
    vb = _block_mean(valid.astype(np.float64), k) > 0.5
    cell_b = cell * k
    # OPEN boundary: pad with a deep ring so border cells drain off-grid.
    # Without it, off-grid neighbors read as walls and a tilted window chains
    # its border cells into a fake border-parallel mega-channel that outranks
    # the real trunk.
    ring = float(np.nanmin(zb[vb])) - 1000.0 if vb.any() else -1000.0
    zp = np.pad(zb, 1, constant_values=ring)
    vp = np.pad(vb, 1, constant_values=True)
    filled_p = v1core.fill_depressions(zp, cell_b, vp)
    accum_p, rec_p = v1core.d8_accumulation(filled_p, cell_b)
    ny, nx = zb.shape
    nxp = nx + 2
    filled = filled_p[1:-1, 1:-1]
    area = accum_p[1:-1, 1:-1].astype(np.float64) * cell_b * cell_b
    area[~vb] = 0.0
    # remap padded receivers to cropped indexing; ring receivers -> outlet
    rp = rec_p[1:-1, 1:-1].ravel()
    rec = np.full(rp.shape, -1, np.int64)
    has = rp >= 0
    tr, tc = np.divmod(rp[has], nxp)
    inb = (tr >= 1) & (tr <= ny) & (tc >= 1) & (tc <= nx)
    vals = np.full(has.sum(), -1, np.int64)
    vals[inb] = (tr[inb] - 1) * nx + (tc[inb] - 1)
    rec[has] = vals
    rec = rec.reshape(ny, nx)
    donors_idx, donors_start = _invert_receivers(rec)
    return Flow(zb=zb, vb=vb, cell_b=cell_b, filled=filled, area_m2=area,
                rec=rec, donors_idx=donors_idx, donors_start=donors_start)


def _block_mean(a: np.ndarray, k: int) -> np.ndarray:
    ny, nx = a.shape[0] // k, a.shape[1] // k
    return a[: ny * k, : nx * k].reshape(ny, k, nx, k).mean(axis=(1, 3))


def _invert_receivers(rec: np.ndarray):
    """CSR donor lists: donors of cell i = donors_idx[start[i]:start[i+1]]."""
    r = rec.ravel()
    src = np.where(r >= 0)[0]
    dst = r[src]
    order = np.argsort(dst, kind="stable")
    donors_idx = src[order]
    counts = np.bincount(dst, minlength=r.size)
    donors_start = np.concatenate([[0], np.cumsum(counts)])
    return donors_idx, donors_start


def _donors(flow: Flow, lin: int) -> np.ndarray:
    return flow.donors_idx[flow.donors_start[lin]:flow.donors_start[lin + 1]]


def _trace_upstream(flow: Flow, outlet_lin: int, min_area: float,
                    osm_tree: cKDTree | None, cfg: dict) -> np.ndarray:
    """Walk upstream choosing, among donors of comparable accumulation, the
    one that best CONTINUES the current direction (rivers run straight through
    confluences; pure max-area tracing wanders up whichever side branch has
    the larger in-window basin when the true trunk enters the window with its
    basin truncated). Near the headwater, continue at a reduced threshold
    while the path stays on an OSM waterway. Returns linear indices
    downstream->upstream."""
    nx = flow.zb.shape[1]
    ext_floor = cfg["osm_extend_area_factor"] * min_area
    tol = cfg["osm_match_tol_m"]
    path = [outlet_lin]
    cur = outlet_lin

    def _dir(lin_a: int, lin_b: int):
        (ya, xa), (yb, xb) = divmod(lin_a, nx), divmod(lin_b, nx)
        v = np.array([xb - xa, yb - ya], float)  # rc-space is fine for angles
        n = np.hypot(*v)
        return v / n if n > 0 else None

    def upstream_dir():
        """Unit upstream-travel direction smoothed over ~12 cells (~100 m) —
        a short window reads meander wiggle, not the channel trend."""
        if len(path) < 3:
            return None
        return _dir(path[-min(len(path), 12)], path[-1])

    def lookahead(start: int, skip: int = 4, steps: int = 16):
        """(direction, probe cell) of a candidate's own channel BODY: walk
        max-area donors upstream; read direction from the `skip`-th cell on —
        the first cells sit in the junction tangle (tributary mouths are
        authored to overshoot along the trunk). The probe cell (~6 cells up)
        sits clear of the junction for the width measurement."""
        chain = [start]
        c = start
        for _ in range(steps):
            d2 = _donors(flow, c)
            if d2.size == 0:
                break
            c = int(d2[np.argmax(flow.area_m2.ravel()[d2])])
            chain.append(c)
        a = chain[min(skip, len(chain) - 1)] if len(chain) > 1 else chain[0]
        return _dir(a, chain[-1]) if chain[-1] != a else _dir(start, chain[-1])

    while True:
        dn = _donors(flow, cur)
        if dn.size == 0:
            break
        areas = flow.area_m2.ravel()[dn]
        # a REAL junction = >=2 channel-scale donors; only there does the
        # keyed contest run (width/continuity outrank raw area — in-window
        # basins mislead when the true trunk enters the window truncated,
        # sometimes to nearly nothing, e.g. a corner-clipped main stem).
        # Everywhere else: plain max-area steepest path.
        cand = dn[areas >= cfg["junction_donor_floor_m2"]]
        u = upstream_dir()
        if u is not None and cand.size > 1:
            # lexicographic: COARSE-binned continuity first (rivers run
            # straight through confluences), accumulation second — never CSR
            # index order. Continuity outranks area because in-window basins
            # mislead when the true trunk enters the window truncated.
            def keyed_pick(pool):
                cos_of = {}
                for c in pool:
                    v = lookahead(int(c))
                    cos_of[int(c)] = float(v @ u) if v is not None else -2.0
                # a HUGELY smaller donor (junction-tangle rill) may only
                # compete on continuity when its direction advantage is
                # DECISIVE — a 0.1-cos edge must not beat a 40x area ratio
                amax_p = max(float(flow.area_m2.ravel()[c]) for c in pool)
                big_cos = max(cos_of[int(c)] for c in pool
                              if flow.area_m2.ravel()[c] >= amax_p / 25.0)
                keyed = []
                for c in pool:
                    a_c = float(flow.area_m2.ravel()[c])
                    if a_c < amax_p / 25.0 \
                            and cos_of[int(c)] < big_cos + 0.35:
                        continue
                    keyed.append((round(cos_of[int(c)], 1), a_c, int(c)))
                return max(keyed)[2]

            best = keyed_pick(cand)
            # never TERMINATE the trace on a sub-threshold pick while a
            # continuable candidate exists — junction tangles offer
            # straight-looking rills, and choosing one cuts the river in two
            if flow.area_m2.ravel()[best] < min_area:
                big = cand[flow.area_m2.ravel()[cand] >= min_area]
                if big.size:
                    best = keyed_pick(big)
        else:
            best = int(dn[np.argmax(areas)])
        a = float(flow.area_m2.ravel()[best])
        if a >= min_area:
            path.append(best)
            cur = best
            continue
        if a >= ext_floor and osm_tree is not None:
            y, x = divmod(best, nx)
            bx, by = _rc_to_xy(np.array([y]), np.array([x]), flow)
            d, _ = osm_tree.query([[bx[0], by[0]]])
            if float(d[0]) <= tol:
                path.append(best)
                cur = best
                continue
        break
    return np.asarray(path, np.int64)


def _rc_to_xy(rows, cols, flow: Flow):
    return frame.rc_to_local(rows, cols, flow.cell_b, flow.zb.shape[0])


def _path_xy(flow: Flow, lins: np.ndarray) -> np.ndarray:
    ny, nx = flow.zb.shape
    rows, cols = np.divmod(lins, nx)
    x, y = _rc_to_xy(rows, cols, flow)
    return np.stack([x, y], axis=1)


def _osm_agreement(path_xy: np.ndarray, osm_pts: np.ndarray | None,
                   tol: float) -> dict:
    if osm_pts is None or len(osm_pts) == 0:
        return {"present": False, "osm_within_frac": None,
                "branch_osm_frac": None, "median_dist_m": None}
    t_branch = cKDTree(path_xy)
    d_osm, _ = t_branch.query(osm_pts)
    t_osm = cKDTree(osm_pts)
    d_br, _ = t_osm.query(path_xy)
    return {"present": True,
            "osm_within_frac": float((d_osm <= tol).mean()),
            "branch_osm_frac": float((d_br <= tol).mean()),
            "median_dist_m": float(np.median(d_br))}


def _resample_osm(osm_lines: list[np.ndarray], step_m: float = 10.0):
    from .transects import resample_polyline
    pts = [resample_polyline(ln, step_m) for ln in osm_lines if len(ln) >= 2]
    return np.concatenate(pts, axis=0) if pts else None


def extract_network(nat_nan: np.ndarray, valid: np.ndarray, cell: float,
                    osm_lines: list[np.ndarray], cfg: dict) -> Network | None:
    flow = coarse_flow(nat_nan, valid, cell, cfg)
    a_min = cfg["accum_min_area_m2"]
    rec = flow.rec.ravel()
    area = flow.area_m2.ravel()
    cand = np.where((rec < 0) & (area >= a_min) & flow.vb.ravel())[0]
    if cand.size == 0:
        return None
    cand = cand[np.argsort(area[cand])[::-1][:8]]  # top outlets by area

    osm_len = sum(transect_len(ln) for ln in osm_lines)
    osm_pts = (_resample_osm(osm_lines)
               if osm_len >= cfg["osm_min_line_len_m"] else None)
    osm_tree = cKDTree(osm_pts) if osm_pts is not None else None

    # trace each candidate; OSM picks among them (lexicographic), never invents
    traces = []
    for c in cand:
        lins = _trace_upstream(flow, int(c), a_min, osm_tree, cfg)
        xy = _path_xy(flow, lins)
        ag = _osm_agreement(xy, osm_pts, cfg["osm_match_tol_m"])
        frac = ag["osm_within_frac"] if ag["present"] else 0.0
        traces.append((round(frac or 0.0, 1), float(area[c]), lins, ag))
    traces.sort(key=lambda t: (t[0], t[1]), reverse=True)
    _frac, _a, trunk_lins, trunk_ag = traces[0]
    if trunk_ag["present"] and (trunk_ag["osm_within_frac"] or 0.0) \
            < cfg["osm_divergent_frac"]:
        trunk_ag["osm_divergent"] = True

    # upstream->downstream ordering
    trunk_lins = trunk_lins[::-1]
    if transect_len(_path_xy(flow, trunk_lins)) < cfg["trib_min_len_m"]:
        return None  # no channel-scale trunk in this window
    trunk = Branch(path_rc=_lin_rc(flow, trunk_lins),
                   path_xy=_path_xy(flow, trunk_lins), parent=None,
                   junction_frac=float("nan"),
                   mouth_area_m2=float(area[trunk_lins[-1]]), osm=trunk_ag)
    branches = [trunk]
    _find_tribs(flow, branches, 0, osm_pts, osm_tree, cfg, depth=1)
    # cap total tributaries by mouth area
    if len(branches) - 1 > cfg["max_tributaries"]:
        tribs = sorted(branches[1:], key=lambda b: -b.mouth_area_m2)
        keep = set(id(b) for b in tribs[:cfg["max_tributaries"]])
        kept = [branches[0]] + [b for b in branches[1:]
                                if id(b) in keep or _has_kept_child(
                                    b, branches, keep)]
        branches = _reindex(kept, branches)
    return Network(branches=branches, cell_b=flow.cell_b, osm_lines=osm_lines)


def transect_len(pts: np.ndarray) -> float:
    seg = np.diff(pts, axis=0)
    return float(np.hypot(seg[:, 0], seg[:, 1]).sum())


def _lin_rc(flow: Flow, lins: np.ndarray) -> np.ndarray:
    ny, nx = flow.zb.shape
    rows, cols = np.divmod(lins, nx)
    return np.stack([rows, cols], axis=1)


def _has_kept_child(b: Branch, branches: list[Branch], keep: set) -> bool:
    i = branches.index(b)
    return any(id(c) in keep for c in branches
               if c.parent is not None and branches[c.parent] is b)


def _reindex(kept: list[Branch], old: list[Branch]) -> list[Branch]:
    idx = {id(b): i for i, b in enumerate(kept)}
    for b in kept:
        if b.parent is not None:
            b.parent = idx[id(old[b.parent])]
    return kept


def _find_tribs(flow: Flow, branches: list[Branch], parent_idx: int,
                osm_pts, osm_tree, cfg: dict, depth: int) -> None:
    if depth > cfg["max_depth"]:
        return
    parent = branches[parent_idx]
    ny, nx = flow.zb.shape
    plins = parent.path_rc[:, 0] * nx + parent.path_rc[:, 1]
    on_parent = set(int(v) for v in plins)
    area = flow.area_m2.ravel()
    seg = np.diff(parent.path_xy, axis=0)
    cum = np.concatenate([[0.0], np.cumsum(np.hypot(seg[:, 0], seg[:, 1]))])
    total = max(cum[-1], 1e-9)
    for i, lin in enumerate(plins):
        for dn in _donors(flow, int(lin)):
            dn = int(dn)
            if dn in on_parent or area[dn] < cfg["trib_min_area_m2"]:
                continue
            lins = _trace_upstream(flow, dn, cfg["trib_min_area_m2"],
                                   osm_tree, cfg)
            xy = _path_xy(flow, lins)
            if transect_len(xy) < cfg["trib_min_len_m"]:
                continue
            lins = lins[::-1]  # upstream->downstream (mouth last)
            br = Branch(path_rc=_lin_rc(flow, lins),
                        path_xy=_path_xy(flow, lins), parent=parent_idx,
                        junction_frac=float(cum[i] / total),
                        mouth_area_m2=float(area[dn]),
                        osm=_osm_agreement(_path_xy(flow, lins), osm_pts,
                                           cfg["osm_match_tol_m"]))
            branches.append(br)
            _find_tribs(flow, branches, len(branches) - 1, osm_pts, osm_tree,
                        cfg, depth + 1)
    # order is parent-first by construction (child index > parent index),
    # and deterministic: trunk cells walked upstream->downstream, donors in
    # CSR order. join_trunk < i holds for the MacroConfig valleys array.


def smooth_centerline(path_xy: np.ndarray, cfg: dict) -> np.ndarray:
    """Raster path -> smooth centerline: uniform 5 m resample + Gaussian
    low-pass at lowpass_m half-amplitude cutoff (kills block jaggies,
    preserves meanders), resampled again to uniform 5 m."""
    from .transects import resample_polyline
    step = cfg["resample_m"]
    pts = resample_polyline(path_xy, step)
    sigma = SIGMA_PER_L * cfg["lowpass_m"] / step
    x = gaussian_filter1d(pts[:, 0], sigma, mode="nearest")
    y = gaussian_filter1d(pts[:, 1], sigma, mode="nearest")
    return resample_polyline(np.stack([x, y], axis=1), step)


def extend_centerline(cl: np.ndarray, ext_up_m: float, ext_down_m: float,
                      bounds: tuple[float, float, float, float]
                      ) -> tuple[np.ndarray, float]:
    """Extend along end tangents (end-caps must fall outside the compare
    zone), clamped inside bounds (xlo, xhi, ylo, yhi). Returns
    (extended, actual_upstream_extension)."""
    out = [cl]
    t0 = cl[0] - cl[1]
    t0 = t0 / max(np.hypot(*t0), 1e-9)
    t1 = cl[-1] - cl[-2]
    t1 = t1 / max(np.hypot(*t1), 1e-9)
    up = _max_extension(cl[0], t0, ext_up_m, bounds)
    dn = _max_extension(cl[-1], t1, ext_down_m, bounds)
    if up > 1.0:
        n = max(int(up / 5.0), 2)
        pre = cl[0][None, :] + np.outer(np.linspace(up, 5.0, n), t0)
        out.insert(0, pre)
    if dn > 1.0:
        n = max(int(dn / 5.0), 2)
        post = cl[-1][None, :] + np.outer(np.linspace(5.0, dn, n), t1)
        out.append(post)
    ext = np.concatenate(out, axis=0)
    return ext, up


def _max_extension(p: np.ndarray, t: np.ndarray, want: float,
                   bounds: tuple[float, float, float, float]) -> float:
    """Longest step along t from p keeping p + s·t inside the bounds box."""
    xlo, xhi, ylo, yhi = bounds
    s = want
    for c, tc, lo, hi in ((p[0], t[0], xlo, xhi), (p[1], t[1], ylo, yhi)):
        if tc > 1e-9:
            s = min(s, (hi - c) / tc)
        elif tc < -1e-9:
            s = min(s, (lo - c) / tc)
    return max(s, 0.0)
