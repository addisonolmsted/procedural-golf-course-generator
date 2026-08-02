"""NHD truth-channel consumption: cached NHDPlus HR flowlines -> the same
Network/Branch structures the D8 tracer produces, with VAA joined per branch
(drainage area at mouth + along-arc, stream order, reach slope). Branches are
levelpath chains ordered upstream->downstream by pathlength (distance to the
terminal outlet — larger upstream, the unambiguous orderer)."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field

import numpy as np

from dtm_atlas import config as atlas_config
from dtm_atlas import grids as atlas_grids
from dtm_atlas.nhd import flowline_policy, prop

from .network import Branch, Network
from .transects import arc_length


@dataclass
class BranchVAA:
    area_mouth_km2: float
    stream_order: int
    slope_vaa: float
    arc_a: list = field(default_factory=list)   # [(arc_m, A_km2)] downstream ends
    artificial_frac: float = 0.0
    intermittent_frac: float = 0.0


def load_network(key: str, meta: dict) -> tuple[Network | None,
                                                dict[int, BranchVAA]]:
    """Per-tile NHD network in the local frame + per-branch VAA records."""
    ndir = os.path.join(atlas_config.STORE, key, "nhd")
    fl_p = os.path.join(ndir, "flowlines.geojson")
    if not os.path.exists(fl_p):
        return None, {}
    with open(fl_p) as f:
        fc = json.load(f)
    x0, y0, _x1, _y1 = meta["window_utm"]
    epsg = int(str(meta["crs"]).split(":")[1])

    # gather kept features by levelpath
    by_lp: dict[int, list] = {}
    for feat in fc.get("features", []):
        props = feat.get("properties", {})
        pol = flowline_policy(props)
        if not pol["keep"]:
            continue
        lp = int(prop(props, "levelpathi", 0) or 0)
        geom = feat.get("geometry", {})
        if geom.get("type") != "LineString" or len(geom["coordinates"]) < 2:
            continue
        by_lp.setdefault(lp, []).append((props, pol, geom["coordinates"]))
    if not by_lp:
        return None, {}

    def to_local(coords):
        lons = [c[0] for c in coords]
        lats = [c[1] for c in coords]
        xs, ys = atlas_grids.ll_to_utm(lons, lats, epsg)
        return np.stack([np.asarray(xs, float) - x0,
                         np.asarray(ys, float) - y0], axis=1)

    # assemble each levelpath upstream->downstream by pathlength desc
    chains: dict[int, dict] = {}
    for lp, feats in by_lp.items():
        feats.sort(key=lambda t: -float(prop(t[0], "pathlength", 0.0) or 0.0))
        pts_parts = []
        end_idx: list[int] = []      # assembled index of each feature's END
        areas: list[float] = []
        art = 0
        inter = 0
        n_pts = 0
        for props, pol, coords in feats:
            loc = to_local(coords)
            if pts_parts and len(loc) \
                    and np.allclose(pts_parts[-1][-1], loc[0], atol=1.0):
                loc = loc[1:]
            if len(loc):
                pts_parts.append(loc)
                n_pts += len(loc)
            end_idx.append(max(n_pts - 1, 0))
            areas.append(float(prop(props, "totdasqkm", np.nan) or np.nan))
            art += pol["artificial"]
            inter += pol["intermittent"]
        pts = np.concatenate(pts_parts, axis=0) if pts_parts else None
        if pts is None or len(pts) < 2:
            continue
        seg = np.hypot(*np.diff(pts, axis=0).T)
        cum = np.concatenate([[0.0], np.cumsum(seg)])
        arc_a = [(float(cum[i]), a) for i, a in zip(end_idx, areas)]
        p0, _pol0, _c0 = feats[0]
        pn, _poln, _cn = feats[-1]
        chains[lp] = {
            "pts": pts,
            "dn_hydroseq": int(prop(pn, "dnhydroseq", 0) or 0),
            "hydroseqs": {int(prop(p, "hydroseq", 0) or 0)
                          for p, _pl, _c in feats},
            "vaa": BranchVAA(
                area_mouth_km2=float(prop(pn, "totdasqkm", np.nan) or np.nan),
                stream_order=int(prop(pn, "streamorde", 0) or 0),
                slope_vaa=float(np.nanmean([
                    float(prop(p, "slope", np.nan) or np.nan)
                    for p, _pl, _c in feats])),
                arc_a=arc_a,
                artificial_frac=art / max(len(feats), 1),
                intermittent_frac=inter / max(len(feats), 1)),
        }

    # parent wiring: a levelpath's dnhydroseq lands inside the parent chain
    lp_by_seq: dict[int, int] = {}
    for lp, ch in chains.items():
        for seq in ch["hydroseqs"]:
            lp_by_seq[seq] = lp
    order = sorted(chains,
                   key=lambda lp: -chains[lp]["vaa"].area_mouth_km2
                   if np.isfinite(chains[lp]["vaa"].area_mouth_km2) else 0.0)
    idx_of: dict[int, int] = {}
    branches: list[Branch] = []
    vaa_of: dict[int, BranchVAA] = {}
    # parent-first ordering: repeatedly place chains whose parent is placed
    placed: set[int] = set()
    while len(placed) < len(chains):
        progressed = False
        for lp in order:
            if lp in placed:
                continue
            parent_lp = lp_by_seq.get(chains[lp]["dn_hydroseq"])
            if parent_lp is None or parent_lp == lp or parent_lp in placed:
                pi = idx_of.get(parent_lp) if parent_lp not in (None, lp) \
                    else None
                jf = float("nan")
                if pi is not None:
                    from .profiles import polyline_project
                    mouth = chains[lp]["pts"][-1]
                    u, _d, _s, _t = polyline_project(
                        branches[pi].path_xy, [mouth[0]], [mouth[1]])
                    jf = float(u[0])
                b = Branch(path_rc=np.zeros((0, 2), int),
                           path_xy=chains[lp]["pts"],
                           parent=pi, junction_frac=jf,
                           mouth_area_m2=(chains[lp]["vaa"].area_mouth_km2
                                          * 1e6
                                          if np.isfinite(
                                              chains[lp]["vaa"]
                                              .area_mouth_km2) else 0.0),
                           osm={"present": True, "source": "nhd"})
                idx_of[lp] = len(branches)
                branches.append(b)
                vaa_of[len(branches) - 1] = chains[lp]["vaa"]
                placed.add(lp)
                progressed = True
        if not progressed:   # cycle guard: place remaining as parentless
            for lp in order:
                if lp not in placed:
                    b = Branch(path_rc=np.zeros((0, 2), int),
                               path_xy=chains[lp]["pts"], parent=None,
                               junction_frac=float("nan"),
                               mouth_area_m2=0.0,
                               osm={"present": True, "source": "nhd"})
                    idx_of[lp] = len(branches)
                    branches.append(b)
                    vaa_of[len(branches) - 1] = chains[lp]["vaa"]
                    placed.add(lp)
            break
    return Network(branches=branches, cell_b=10.0, osm_lines=[]), vaa_of


def area_at_arc(vaa: BranchVAA, s_m: float) -> float:
    """Drainage area (km2) interpolated at arc position s along the branch."""
    pts = [(a, v) for a, v in vaa.arc_a if np.isfinite(v)]
    if not pts:
        return float("nan")
    arcs = np.array([p[0] for p in pts])
    vals = np.array([p[1] for p in pts])
    return float(np.interp(s_m, arcs, vals))
