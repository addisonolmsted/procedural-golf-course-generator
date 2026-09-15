"""Per-SHOT ground profiles + dogleg turns on real golf holes (read-only).

`holes.py` measures the whole-hole longitudinal profile. The router places
individual shots, so what it needs is finer: on a par 4/5, how far does the
ground under the DRIVE (and the second shot of a par 5) dip below that
shot's own chord -- a carry over a swale or gully -- and how sharply and
where does the hole turn. Same corpus, same join, same 5 m / 15 m-boxcar
profile as `holes.py` (the geometry steps are copied verbatim so the two
tools agree cell for cell); this one just slices the profile into legs and
walks the original polyline for turn angles.

Offline by construction: Overpass responses are read from the fetch cache
only; a cache miss is treated as an empty response, never a network call.

Run from the repo root:  python3 tools/golf/corpus/shot_profiles.py
Writes:                  tools/golf/corpus/out/shot_profiles.json
"""

from __future__ import annotations

import json
import pathlib
import sys
import time

import numpy as np

_GOLF = pathlib.Path(__file__).resolve().parents[1]
if str(_GOLF) not in sys.path:
    sys.path.insert(0, str(_GOLF))
from corpus import config, geo, holes, net, registry  # noqa: E402
from corpus.holes import cgrid  # noqa: E402

STEP_M = 5.0
SMOOTH_M = 15.0
DRIVE_M = 220.0          # full-shot leg length
DRIVE_FRAC = 0.63        # drive leg never runs past 63 % of the hole
APPROACH_RESERVE_M = 60.0  # second leg stops this short of the green
MIN_LEN_M = 250.0        # par 4/5 shorter than this are not full-shot holes
MIN_SEG_M = 20.0         # both legs at a vertex must exceed this to count
S_SHAPE_DEG = 10.0


# --------------------------------------------------------------------------
# offline Overpass: cache hits only
# --------------------------------------------------------------------------
def _overpass_cached(query: str, *_a, **_k) -> dict:
    cp = net._cache_path("overpass", query)
    if cp.exists():
        return json.loads(cp.read_text())
    raise net.FetchError("cache miss (offline run; nothing fetched)")


# --------------------------------------------------------------------------
# profile: identical steps to holes.profile_stats, but returns z(s)
# --------------------------------------------------------------------------
def smoothed_profile(z2, cell2, pts_m: np.ndarray, step_m: float = STEP_M):
    """(ss, zs) along a polyline in tile-local metres, or None (as holes.py:
    L < 60 m, or any resample point off the tile)."""
    segs = np.diff(pts_m, axis=0)
    seglen = np.hypot(segs[:, 0], segs[:, 1])
    L = float(seglen.sum())
    if L < 60.0:
        return None
    n = max(int(L / step_m), 4)
    cum = np.r_[0.0, np.cumsum(seglen)]
    ss = np.linspace(0.0, L, n + 1)
    ys = np.interp(ss, cum, pts_m[:, 0])
    xs = np.interp(ss, cum, pts_m[:, 1])
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
    from scipy.ndimage import uniform_filter1d
    k = max(int(SMOOTH_M / step_m), 1)
    zs = uniform_filter1d(z, k, mode="nearest")
    return ss, zs


def leg_stats(ss: np.ndarray, zs: np.ndarray, s0: float, s1: float):
    """Ground vs. the leg's own chord between arc s0 and s1."""
    if s1 - s0 < 2 * STEP_M:
        return None
    z0 = float(np.interp(s0, ss, zs))
    z1 = float(np.interp(s1, ss, zs))
    inside = (ss > s0) & (ss < s1)
    s = np.r_[s0, ss[inside], s1]
    z = np.r_[z0, zs[inside], z1]
    chord = z0 + (z1 - z0) * (s - s0) / (s1 - s0)
    dz = np.diff(z)
    length = s1 - s0
    return dict(
        dip=float(max((chord - z).max(), 0.0)),
        rise=float(max((z - chord).max(), 0.0)),
        net_dz=z1 - z0,
        climb100=float(dz[dz > 0].sum() / length * 100.0),
        len=float(length),
    )


def turns(pts_m: np.ndarray):
    """Signed turn (deg, + = left / CCW in [y,x]) at every interior vertex
    whose adjacent legs both exceed MIN_SEG_M, with arc fraction t."""
    segs = np.diff(pts_m, axis=0)
    seglen = np.hypot(segs[:, 0], segs[:, 1])
    L = float(seglen.sum())
    cum = np.r_[0.0, np.cumsum(seglen)]
    out = []
    for i in range(1, len(pts_m) - 1):
        a, b = seglen[i - 1], seglen[i]
        if a <= MIN_SEG_M or b <= MIN_SEG_M:
            continue
        u, v = segs[i - 1], segs[i]
        c = float(np.dot(u, v) / (a * b))
        ang = float(np.degrees(np.arccos(np.clip(c, -1.0, 1.0))))
        cross = float(u[1] * v[0] - u[0] * v[1])   # [y,x] -> x_u*y_v - y_u*x_v
        sgn = 1.0 if cross > 0 else -1.0 if cross < 0 else 0.0
        out.append((sgn * ang, float(cum[i] / L)))
    return out


# --------------------------------------------------------------------------
# main pass: the holes.run() join, verbatim
# --------------------------------------------------------------------------
def run() -> list[dict]:
    t0 = time.time()
    holes.net.overpass = _overpass_cached
    ways = holes.fetch_hole_ways()
    print(f"  {len(ways)} golf=hole ways from cache", flush=True)
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
    tiles: dict[str, tuple] = {}
    rows = []
    n_join = n_nocourse = n_noprof = n_flip = 0
    joined_ids = set()
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
            n_nocourse += 1
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
            if len(tiles) % 50 == 0:
                print(f"    {len(tiles)} tiles, {len(rows)} rows, "
                      f"{time.time() - t0:.0f} s", flush=True)
        z2, cell2, gl = tiles[cid]
        if len(gl):
            d0 = np.hypot(gl[:, 0] - pts_loc[0, 0],
                          gl[:, 1] - pts_loc[0, 1]).min()
            d1 = np.hypot(gl[:, 0] - pts_loc[-1, 0],
                          gl[:, 1] - pts_loc[-1, 1]).min()
            if d0 < d1 - 15.0:
                pts_loc = pts_loc[::-1]
                n_flip += 1
        prof = smoothed_profile(z2, cell2, pts_loc)
        if prof is None:
            n_noprof += 1
            continue
        n_join += 1
        joined_ids.add(wid)
        par = el.get("tags", {}).get("par")
        par = int(par) if par and par.isdigit() else None
        ss, zs = prof
        L = float(ss[-1])
        if par not in (4, 5) or L < MIN_LEN_M:
            continue
        s1 = min(DRIVE_M, DRIVE_FRAC * L)
        drive = leg_stats(ss, zs, 0.0, s1)
        second = None
        if par == 5:
            s2 = min(s1 + DRIVE_M, L - APPROACH_RESERVE_M)
            second = leg_stats(ss, zs, s1, s2)
        tl = turns(pts_loc)
        if tl:
            k = int(np.argmax([abs(a) for a, _ in tl]))
            dmax, dt = abs(tl[k][0]), tl[k][1]
        else:
            dmax, dt = 0.0, None
        signed = [a for a, _ in tl]
        s_shape = None
        if par == 5:
            big = [a for a in signed if abs(a) > S_SHAPE_DEG]
            s_shape = bool(big and min(big) < 0 < max(big))
        rows.append(dict(
            way_id=wid, course_id=cid, region=rec["region_tag"], par=par,
            length_m=L, drive=drive, second=second,
            dogleg_max_deg=dmax, dogleg_t=dt, turns=signed, s_shape=s_shape,
        ))
    out = config.OUT / "shot_profiles.json"
    out.write_text(json.dumps(rows))
    print(f"  joined {n_join} ways on {len(tiles)} tiles ({n_flip} flipped); "
          f"skipped {n_nocourse} outside every keeper polygon, "
          f"{n_noprof} off-tile or < 60 m  [{time.time() - t0:.0f} s]")
    ref = config.OUT / "hole_profiles.json"
    if ref.exists():
        ref_ids = {r["way_id"] for r in json.loads(ref.read_text())}
        print(f"  coverage vs hole_profiles.json: {len(ref_ids)} ref ways, "
              f"{len(ref_ids & joined_ids)} shared, "
              f"{len(ref_ids - joined_ids)} ref-only, "
              f"{len(joined_ids - ref_ids)} new-only")
    print(f"  wrote {out} ({len(rows)} par-4/5 ways >= {MIN_LEN_M:.0f} m)")
    return rows


# --------------------------------------------------------------------------
# summary
# --------------------------------------------------------------------------
def _p(v, q):
    v = np.asarray(v, float)
    return float(np.percentile(v, q)) if len(v) else float("nan")


def _pct(v, thr):
    v = np.asarray(v, float)
    return 100.0 * float((v >= thr).mean()) if len(v) else float("nan")


def _leg_block(name: str, legs: list[dict], full: bool = True) -> None:
    dip = [g["dip"] for g in legs]
    print(f"  {name} (n={len(legs)}):")
    print(f"    dip   p50 {_p(dip, 50):.2f}  p75 {_p(dip, 75):.2f}  "
          f"p90 {_p(dip, 90):.2f}  p99 {_p(dip, 99):.2f} m")
    print(f"    share dip >= 1.0 m {_pct(dip, 1.0):.1f} %   >= 1.5 m "
          f"{_pct(dip, 1.5):.1f} %   >= 3.0 m {_pct(dip, 3.0):.1f} %")
    big = [d for d in dip if d >= 1.5]
    print(f"    among dip >= 1.5 m (n={len(big)}): dip p50 {_p(big, 50):.2f}"
          f"  p90 {_p(big, 90):.2f} m")
    if not full:
        return
    rise = [g["rise"] for g in legs]
    ndz = [g["net_dz"] for g in legs]
    cl = [g["climb100"] for g in legs]
    print(f"    rise  p50 {_p(rise, 50):.2f}  p90 {_p(rise, 90):.2f} m")
    print(f"    net_dz p10 {_p(ndz, 10):+.2f}  p50 {_p(ndz, 50):+.2f}  "
          f"p90 {_p(ndz, 90):+.2f} m")
    print(f"    climb100 p50 {_p(cl, 50):.2f}  p90 {_p(cl, 90):.2f} m/100 m")


def _dogleg_line(label: str, rows: list[dict]) -> None:
    allv = [r["dogleg_max_deg"] for r in rows]
    bent = [r["dogleg_max_deg"] for r in rows if r["dogleg_t"] is not None]
    print(f"    {label:8s} n={len(rows):5d}  max turn p50 {_p(allv, 50):5.1f}"
          f"  p90 {_p(allv, 90):5.1f} deg (all, straight=0)  |  "
          f"p50 {_p(bent, 50):5.1f}  p90 {_p(bent, 90):5.1f} deg (bent only)")


def summarize(rows: list[dict]) -> None:
    p4 = [r for r in rows if r["par"] == 4]
    p5 = [r for r in rows if r["par"] == 5]
    print("\n=== shot profiles: summary ===")
    print(f"  n ways {len(rows)}  (par 4: {len(p4)}, par 5: {len(p5)})")
    _leg_block("drive leg, all par 4/5", [r["drive"] for r in rows])
    _leg_block("second leg, par 5",
               [r["second"] for r in p5 if r["second"]], full=True)
    print("  dogleg:")
    _dogleg_line("par 4", p4)
    _dogleg_line("par 5", p5)
    _dogleg_line("all", rows)
    dm = [r["dogleg_max_deg"] for r in rows]
    print(f"    share max turn > 15 deg: {_pct(dm, 15.0 + 1e-9):.1f} % "
          f"(par 4 {_pct([r['dogleg_max_deg'] for r in p4], 15.0 + 1e-9):.1f} %,"
          f" par 5 {_pct([r['dogleg_max_deg'] for r in p5], 15.0 + 1e-9):.1f} %)")
    tt = [r["dogleg_t"] for r in rows if r["dogleg_t"] is not None]
    print(f"    t of max turn p50 {_p(tt, 50):.2f}  (p25 {_p(tt, 25):.2f}, "
          f"p75 {_p(tt, 75):.2f})")
    straight = [r for r in rows if r["dogleg_t"] is None]
    print(f"    straight (no qualifying turn): {len(straight)} = "
          f"{100.0 * len(straight) / max(len(rows), 1):.1f} % "
          f"(par 4 {100.0 * sum(r['dogleg_t'] is None for r in p4) / max(len(p4), 1):.1f} %,"
          f" par 5 {100.0 * sum(r['dogleg_t'] is None for r in p5) / max(len(p5), 1):.1f} %)")
    if p5:
        ns = sum(bool(r["s_shape"]) for r in p5)
        print(f"    par-5 s_shape: {ns} = {100.0 * ns / len(p5):.1f} %")
    print("  by region:")
    regions = sorted({r["region"] for r in rows})
    for reg in regions:
        rr = [r for r in rows if r["region"] == reg]
        dip = [r["drive"]["dip"] for r in rr]
        dg = [r["dogleg_max_deg"] for r in rr]
        print(f"    {reg:14s} n={len(rr):4d}  drive dip p50 {_p(dip, 50):.2f} m"
              f"  >=1.5 m {_pct(dip, 1.5):4.1f} %  dogleg p50 {_p(dg, 50):4.1f}"
              f"  p90 {_p(dg, 90):4.1f} deg")


if __name__ == "__main__":
    summarize(run())
