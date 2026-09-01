"""Creek planform measurement — corpus statistics and model comparison.

MEASUREMENT ONLY. Nothing here affects generation; these are the instruments
behind docs/calibration/creek-planform.md, kept so the numbers can be
re-derived rather than trusted. The creek-planform experiment they were built
for was reverted (see the doc); the measurements stand.

Same pipeline both sides is the whole point: OSM stream vertices sit 10-30 m
apart, so an unsmoothed curvature measures DIGITIZING noise, not planform.
Every statistic here resamples at 10 m and smooths at 30 m.
"""
import sys, os, pathlib
sys.path.insert(0, ".")
import numpy as np
from corpus import config, net, geo

QUERY = ('[out:json][timeout:180];('
         'way["waterway"="stream"]({b});'
         ');out geom;')

def bends(P, smooth_m=30.0):
    """Resample, smooth, and split at curvature inflections."""
    seg = np.hypot(*np.diff(P, axis=0).T)
    L = float(seg.sum())
    if L < 800: return None
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, 10.0)
    Q = np.stack([np.interp(ss, cum, P[:,0]), np.interp(ss, cum, P[:,1])], 1)
    k = max(int(smooth_m/10.0), 1)
    if k > 1:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0], ker, "valid"),
                      np.convolve(Q[:,1], ker, "valid")], 1)
    if len(Q) < 12: return None
    v1 = np.diff(Q, axis=0)[:-1]; v2 = np.diff(Q, axis=0)[1:]
    cr = v1[:,0]*v2[:,1] - v1[:,1]*v2[:,0]
    sg = np.sign(cr)
    idx = np.where(np.diff(sg) != 0)[0]
    if len(idx) < 4: return None
    # bend = arc between successive inflections
    s_i = ss[:len(sg)][idx]
    blen = np.diff(s_i)
    blen = blen[(blen > 25) & (blen < 1500)]
    if len(blen) < 4: return None
    seg2 = np.hypot(*np.diff(Q, axis=0).T)
    L2 = seg2.sum(); chord = np.hypot(*(Q[-1]-Q[0]))
    return dict(n_bends=len(blen), lam_med=float(np.median(blen)),
                lam_cv=float(np.std(blen)/np.mean(blen)),
                sinuosity=float(L2/max(chord,1)), length=L2)

rows = []
for (tag, la, lo, h) in config.SEARCH_BOXES:
    for (qa, qo) in ((la-h/2, lo-h/2), (la-h/2, lo+h/2),
                     (la+h/2, lo-h/2), (la+h/2, lo+h/2)):
        b = f"{qa-h/2},{qo-h/2},{qa+h/2},{qo+h/2}"
        try:
            d = net.overpass(QUERY.format(b=b))
        except net.FetchError as e:
            print(f"  [net] {tag}: {e}", flush=True); continue
        for el in d.get("elements", []):
            g = el.get("geometry") or []
            if len(g) < 8: continue
            lat = np.array([q["lat"] for q in g]); lon = np.array([q["lon"] for q in g])
            P = geo.ll_to_m(lat, lon, geo.utm_zone(float(lon.mean())))
            st = bends(P)
            if st: st["region"] = tag; rows.append(st)
    print(f"  {tag}: {len(rows)} creeks so far", flush=True)

import json
json.dump(rows, open("/tmp/creek_corpus.json","w"))
seg_note = ("smoothing 30 m over a 10 m resample, bends >= 25 m, reaches >= 800 m: "
            "OSM stream lines carry vertices every 10-30 m and a 12 m filter "
            "measured the DIGITIZING noise (bend length read 24/24/30 m, "
            "physically impossible for a creek)")
print(" note:", seg_note)
lam = np.array([r["lam_med"] for r in rows])
cv  = np.array([r["lam_cv"] for r in rows])
sin = np.array([r["sinuosity"] for r in rows])
print(f"\n{len(rows)} real creeks (waterway=stream, >=4 bends, >=400 m)")
print(f"  bend length      p25/50/75 = {np.percentile(lam,25):.0f}/"
      f"{np.percentile(lam,50):.0f}/{np.percentile(lam,75):.0f} m")
print(f"  bend-length CV   p25/50/75 = {np.percentile(cv,25):.2f}/"
      f"{np.percentile(cv,50):.2f}/{np.percentile(cv,75):.2f}   <-- IRREGULARITY")
print(f"  sinuosity        p25/50/75 = {np.percentile(sin,25):.3f}/"
      f"{np.percentile(sin,50):.3f}/{np.percentile(sin,75):.3f}")
