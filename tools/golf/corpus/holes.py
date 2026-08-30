"""OSM golf=hole lines + DEM = real longitudinal hole profiles.

OSM maps the LINE OF PLAY: `golf=hole` ways run tee -> green by mapping
convention, usually carrying ref= and par= tags. Sampled against the 2 m
corpus tiles they give the ground-truth height profile along real holes --
exactly the quantity the router's spine chooses. Orientation is verified
per hole against the course's assigned greens (the endpoint nearer a mapped
green is the green end); ways whose ends can't be disambiguated are kept
but flagged.
"""

from __future__ import annotations

import json
import pathlib
import sys

import numpy as np

from . import config, geo, net, registry

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
if str(_TOOLS / "macro_campaign") not in sys.path:
    sys.path.insert(0, str(_TOOLS / "macro_campaign"))
from macro_campaign import cgrid  # noqa: E402


def _holes_query(s, w, n, e) -> str:
    return ("[out:json][timeout:180];("
            f'way["golf"="hole"]({s},{w},{n},{e});'
            ");out geom tags;")


def fetch_hole_ways() -> dict[int, dict]:
    """All golf=hole ways across the corpus footprint (cached client)."""
    ways: dict[int, dict] = {}
    for (tag, la, lo, h) in config.SEARCH_BOXES:
        for (qa, qo) in ((la - h / 2, lo - h / 2), (la - h / 2, lo + h / 2),
                         (la + h / 2, lo - h / 2), (la + h / 2, lo + h / 2)):
            try:
                d = net.overpass(_holes_query(qa - h / 2, qo - h / 2,
                                              qa + h / 2, qo + h / 2))
            except net.FetchError as ex:
                print(f"  [net] holes {tag}: {ex}", flush=True)
                continue
            for el in d.get("elements", []):
                if el.get("type") == "way" and len(el.get("geometry", [])) >= 2:
                    ways[el["id"]] = el
    for r in registry.all_records():
        if r["region_tag"] != "other" or r["status"] != "keeper":
            continue
        ring = np.array(r["osm"]["ring_ll"], float)
        pad = 0.01
        try:
            d = net.overpass(_holes_query(ring[:, 0].min() - pad,
                                          ring[:, 1].min() - pad,
                                          ring[:, 0].max() + pad,
                                          ring[:, 1].max() + pad))
        except net.FetchError as ex:
            print(f"  [net] holes {r['course_id']}: {ex}", flush=True)
            continue
        for el in d.get("elements", []):
            if el.get("type") == "way" and len(el.get("geometry", [])) >= 2:
                ways[el["id"]] = el
    return ways


def profile_stats(z2, cell2, pts_m: np.ndarray, step_m: float = 5.0):
    """Height stats along a tee->green polyline in tile-local metres."""
    segs = np.diff(pts_m, axis=0)
    seglen = np.hypot(segs[:, 0], segs[:, 1])
    L = float(seglen.sum())
    if L < 60.0:
        return None
    n = max(int(L / step_m), 4)
    # arc-length resample
    cum = np.r_[0.0, np.cumsum(seglen)]
    ss = np.linspace(0.0, L, n + 1)
    ys = np.interp(ss, cum, pts_m[:, 0])
    xs = np.interp(ss, cum, pts_m[:, 1])
    # OFF-TILE REJECTION (2026-08-30): np.clip snapped out-of-tile samples
    # to the border cells, and holes running past the tile edge printed +70 m
    # phantom cliffs into the mean profile (climb p50 read 67 m -- not golf).
    # A hole partially outside its course tile yields no profile at all.
    ext_y = z2.shape[0] * cell2
    ext_x = z2.shape[1] * cell2
    if (ys.min() < 0 or xs.min() < 0 or ys.max() >= ext_y
            or xs.max() >= ext_x):
        return None
    yi = (ys / cell2).astype(int)
    xi = (xs / cell2).astype(int)
    z = z2[yi, xi].astype(float)
    if not np.isfinite(z).all():
        z = np.where(np.isfinite(z), z, np.nanmedian(z))
    # smooth ~15 m so 2 m texture doesn't read as climb. EDGE-REPLICATED:
    # np.convolve(mode="same") zero-pads, which divided the endpoint
    # elevations by the kernel width and printed a +70 m phantom cliff at
    # both ends of every profile (climb p50 read 67 m -- the absolute
    # elevation leaking through the pad, not terrain).
    from scipy.ndimage import uniform_filter1d
    k = max(int(15.0 / step_m), 1)
    zs = uniform_filter1d(z, k, mode="nearest")
    dz = np.diff(zs)
    net_dz = float(zs[-1] - zs[0])            # green minus tee
    return dict(
        length_m=L,
        net_dz=net_dz,
        total_climb=float(dz[dz > 0].sum()),
        total_drop=float(-dz[dz < 0].sum()),
        max_above_chord=float((zs - np.linspace(zs[0], zs[-1], len(zs))).max()),
        min_below_chord=float((zs - np.linspace(zs[0], zs[-1], len(zs))).min()),
        mean_abs_grade=float(np.abs(dz).sum() / L),
        z_rel=[float(v) for v in
               np.interp(np.linspace(0, 1, 21), np.linspace(0, 1, len(zs)),
                         zs - zs[0])],
    )


def run() -> dict:
    ways = fetch_hole_ways()
    print(f"  {len(ways)} golf=hole ways fetched")
    # course lookup structures
    courses = []
    for r in registry.all_records():
        if r["status"] != "keeper" or not r.get("tile", {}).get("path"):
            continue
        ring = np.array(r["osm"]["ring_ll"], float)
        courses.append(dict(
            rec=r, ring_ll=ring,
            bbox=(ring[:, 0].min(), ring[:, 1].min(),
                  ring[:, 0].max(), ring[:, 1].max()),
            ring_m=geo.ll_to_m(ring[:, 0], ring[:, 1], r["utm"]["zone"]),
            area=r["osm"]["area_m2"]))
    out_rows = []
    tiles: dict[str, tuple] = {}
    n_flip = n_orient_unknown = 0
    for wid, el in sorted(ways.items()):
        gpts = np.array([[g["lat"], g["lon"]] for g in el["geometry"]], float)
        mid = gpts[len(gpts) // 2]
        best = None
        for c in courses:
            b = c["bbox"]
            if not (b[0] - 0.003 <= mid[0] <= b[2] + 0.003
                    and b[1] - 0.003 <= mid[1] <= b[3] + 0.003):
                continue
            zone = c["rec"]["utm"]["zone"]
            pm = geo.ll_to_m(gpts[:, 0], gpts[:, 1], zone)
            if geo.point_in_poly(pm[len(pm) // 2], c["ring_m"]):
                if best is None or c["area"] < best[0]["area"]:
                    best = (c, pm)
        if best is None:
            continue
        c, pm = best
        rec = c["rec"]
        t = rec["tile"]
        pts_loc = pm - np.array([t["n0"], t["e0"]])
        cid = rec["course_id"]
        if cid not in tiles:
            z2, (_, _, cell2) = cgrid.read_f32(pathlib.Path(t["path"]))
            z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(float)
            gl = np.array([[g["yx_utm"][0] - t["n0"], g["yx_utm"][1] - t["e0"]]
                           for g in rec["greens"]], float)
            tiles[cid] = (z2, cell2, gl)
        z2, cell2, gl = tiles[cid]
        # orientation: the end nearer an assigned green is the GREEN end
        if len(gl):
            d0 = np.hypot(gl[:, 0] - pts_loc[0, 0],
                          gl[:, 1] - pts_loc[0, 1]).min()
            d1 = np.hypot(gl[:, 0] - pts_loc[-1, 0],
                          gl[:, 1] - pts_loc[-1, 1]).min()
            if d0 < d1 - 15.0:
                pts_loc = pts_loc[::-1]
                n_flip += 1
            elif abs(d0 - d1) <= 15.0:
                n_orient_unknown += 1
        st = profile_stats(z2, cell2, pts_loc)
        if st is None:
            continue
        par = el.get("tags", {}).get("par")
        st.update(course_id=cid, region=rec["region_tag"], way_id=wid,
                  par=int(par) if par and par.isdigit() else None)
        out_rows.append(st)
    (config.OUT / "hole_profiles.json").write_text(json.dumps(out_rows))
    print(f"  {len(out_rows)} profiles on {len(tiles)} courses "
          f"({n_flip} flipped to tee->green, {n_orient_unknown} ambiguous)")
    return dict(n=len(out_rows), courses=len(tiles))
