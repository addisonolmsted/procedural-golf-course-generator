"""Per-grid valley extraction pipeline — network -> per-branch two-pass
transect fitting -> MacroConfig valley records. Shared verbatim by synthcheck
(preset grids), the fleet extractor (real courses), and the round-trip scorer
(which reuses the station/stack path on synthetic grids)."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from . import network as N
from . import transects as T
from . import valleyfit as V


@dataclass
class BranchResult:
    branch: N.Branch
    centerline: np.ndarray            # recentered, UNextended
    cl_ext: np.ndarray                # extended (Path::Points source)
    ext_up_m: float
    stations: T.Stations
    tr: T.Transects
    keep: np.ndarray
    reaches: list[T.Reach]
    crossfits: list[V.CrossFit]
    longfit: V.LongFit | None
    top_width_m: float
    valley: dict | None = None
    meas: dict = field(default_factory=dict)


def _branch_pass(nat_nan, masks, cell, cl, tcfg, foreign=(), junctions=(),
                 halflen=None, search=None, hw_est=None):
    st = T.stations(cl, tcfg["spacing_m"])
    tr = T.sample(nat_nan, masks, cell, st, halflen or tcfg["halflen_m"],
                  tcfg["sample_step_m"])
    tr = T.recenter(tr, search or tcfg["recenter_search_m"],
                    tcfg["floor_probe_min_m"], hw_est_m=hw_est)
    keep = T.keep_mask(tr, tcfg, foreign=list(foreign))
    for jxy, pad in junctions:
        keep &= np.hypot(st.xy[:, 0] - jxy[0], st.xy[:, 1] - jxy[1]) > pad
    reaches = T.stack(tr, keep, tcfg["reach_len_m"],
                      tcfg["min_transects_per_reach"])
    return st, tr, keep, reaches


def _station_c0(st: T.Stations, reaches: list[T.Reach],
                cfs: list[V.CrossFit]) -> np.ndarray:
    """Per-station lateral center correction from ok reach fits (0 elsewhere)."""
    off = np.zeros(len(st.s))
    for rc, cf in zip(reaches, cfs):
        if cf.ok and np.isfinite(cf.c0):
            off[rc.station_idx] = cf.c0
    return off


def _shift_centerline(cl: np.ndarray, st: T.Stations,
                      off: np.ndarray) -> np.ndarray:
    """Shift the centerline laterally by per-station offsets (interpolated
    along arc), then uniform-resample."""
    if not np.any(off):
        return cl
    seg = np.diff(cl, axis=0)
    cum = np.concatenate([[0.0], np.cumsum(np.hypot(seg[:, 0], seg[:, 1]))])
    o = np.interp(cum, st.s, off)
    nx = np.interp(cum, st.s, st.normal[:, 0])
    ny = np.interp(cum, st.s, st.normal[:, 1])
    moved = cl + np.stack([nx * o, ny * o], axis=1)
    return T.resample_polyline(moved, 5.0)


def _top_width(crossfits: list[V.CrossFit], default: float = 40.0) -> float:
    ws = [cf.hw + cf.inc / max(min(cf.ml, cf.mr), 0.05) + cf.r
          for cf in crossfits if cf.ok]
    return float(np.median(ws)) if ws else default


def extract_valleys(nat_nan: np.ndarray, valid: np.ndarray,
                    masks: dict[str, np.ndarray], cell: float,
                    osm_lines: list[np.ndarray], cfg: dict,
                    network: N.Network | None = None
                    ) -> tuple[list[BranchResult], N.Network | None]:
    """network: an externally supplied truth network (NHD) skips D8 tracing;
    everything downstream (smoothing, transects, fits) is identical."""
    ncfg, ccfg, tcfg, vcfg = (cfg["network"], cfg["centerline"],
                              cfg["transects"], cfg["valleyfit"])
    net = network if network is not None \
        else N.extract_network(nat_nan, valid, cell, osm_lines, ncfg)
    if net is None:
        return [], None
    h, w = nat_nan.shape
    bounds = (0.0, w * cell, 0.0, h * cell)

    # pass A: provisional fits (wide transects) -> widths + centerline
    # correction from BOTH floor-plateau offsets and the fitted lateral
    # center c0 (wide flat floors leave the flow trace off-center where the
    # plateau search cannot see the walls)
    cls = [N.smooth_centerline(b.path_xy, ccfg) for b in net.branches]
    widths = []
    hw_provs = []
    for i, cl in enumerate(cls):
        st, tr, keep, reaches = _branch_pass(nat_nan, masks, cell, cl, tcfg,
                                             halflen=tcfg["halflen_max_m"])
        cfs = [V.fit_cross(rc, vcfg) for rc in reaches]
        widths.append(_top_width(cfs))
        hws = [cf.hw for cf in cfs if cf.ok]
        hw_provs.append(float(np.median(hws)) if hws else 10.0)
        for _ in range(ccfg["recenter_iters"]):
            off = np.clip(tr.floor_offset, -tcfg["recenter_search_m"],
                          tcfg["recenter_search_m"])
            off = np.where(keep, off, 0.0) + _station_c0(st, reaches, cfs)
            moved = st.xy + st.normal * off[:, None]
            cls[i] = N.smooth_centerline(moved, ccfg)

    # BRAID SUPPRESSION: a wide flat floor hosts several D8 threads; the
    # extra threads fit the SAME valley (hw ~ the larger branch's hw) while
    # running inside its top width — phantom duplicates whose foreign
    # corridors would annihilate the real branch's stations. Drop them.
    from . import profiles as P
    braid = set()
    order_by_w = sorted(range(len(cls)), key=lambda i: -widths[i])
    for i in range(len(cls)):
        for j in order_by_w:
            if j == i or widths[j] <= widths[i] or j in braid:
                continue
            if abs(hw_provs[i] - hw_provs[j]) > 0.4 * hw_provs[j]:
                continue
            _u, dist, _s, _t = P.polyline_project(
                cls[j], cls[i][::4, 0], cls[i][::4, 1])
            if np.median(dist) < widths[j]:
                braid.add(i)
                break
    if braid:
        # re-parent children of dropped braids to the braid's parent
        for b in net.branches:
            while b.parent is not None and b.parent in braid:
                b.parent = net.branches[b.parent].parent
    active = [i for i in range(len(cls)) if i not in braid]

    # junction points: each tributary's mouth on its parent
    junctions_of: dict[int, list] = {i: [] for i in range(len(net.branches))}
    for i, b in enumerate(net.branches):
        if b.parent is None or i in braid:
            continue
        mouth = cls[i][-1]
        pad = max(tcfg["junction_pad_m"], widths[b.parent] / 2 + widths[i] / 2)
        junctions_of[b.parent].append((mouth, pad))
        junctions_of[i].append((mouth, pad))

    results: list[BranchResult] = []
    for i, b in enumerate(net.branches):
        if i in braid:
            continue
        # foreign corridor = the OTHER branch's FLOOR halfwidth (+pad in
        # keep_mask) — its walls/upland are legitimately shared terrain that
        # this branch's transects must be allowed to cross
        foreign = [(cls[j], hw_provs[j]) for j in active if j != i]
        halflen = float(np.clip(2.0 * widths[i], tcfg["halflen_m"],
                                tcfg["halflen_max_m"]))
        search = max(tcfg["recenter_search_m"],
                     tcfg["search_frac_hw"] * hw_provs[i])
        st, tr, keep, reaches = _branch_pass(
            nat_nan, masks, cell, cls[i], tcfg,
            foreign=foreign, junctions=junctions_of[i],
            halflen=halflen, search=search, hw_est=hw_provs[i])
        cfs = [V.fit_cross(rc, vcfg) for rc in reaches]
        # residual lateral center from pass B folds into the emitted geometry
        cl_final = _shift_centerline(cls[i], st, _station_c0(st, reaches, cfs))
        z_floor = np.where(keep, tr.z_floor, np.nan)
        longfit = V.fit_longitudinal(st.s, z_floor, cfg["longitudinal"])
        cl_ext, ext_up = N.extend_centerline(
            cl_final, ccfg["extension_m"], ccfg["extension_m"], bounds)
        cls[i] = cl_final
        res = BranchResult(branch=b, centerline=cl_final, cl_ext=cl_ext,
                           ext_up_m=ext_up, stations=st, tr=tr, keep=keep,
                           reaches=reaches, crossfits=cfs, longfit=longfit,
                           top_width_m=_top_width(cfs))
        if longfit is not None and any(cf.ok for cf in cfs):
            join = b.parent if b.parent is not None else None
            res.valley, res.meas = V.build_valley(
                cl_ext, ext_up, cfs, reaches, longfit, join,
                {**ccfg, **vcfg})
        results.append(res)
    return results, net
