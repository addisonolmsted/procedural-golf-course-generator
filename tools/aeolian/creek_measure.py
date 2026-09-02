"""Creek planform measurement -- the instruments, importable.

MEASUREMENT ONLY. `bends`, `bend_skew` and `curv_profile`/`straight_stats`
are COPIED VERBATIM from creek_corpus.py, creek_skew.py and creek_straight.py
(those three execute their fetch at import time, so they cannot be imported).
Do not edit one without the other; `python3 creek_sheet.py --selftest`
compares `bend_skew` here against the importable original.

Same pipeline both sides is the whole point: OSM stream vertices sit 10-30 m
apart, so an unsmoothed curvature measures DIGITIZING noise, not planform.
Every statistic here resamples at 10 m and smooths at 30 m.
"""
import numpy as np

# --- creek_corpus.bends ------------------------------------------------------
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

# --- creek_skew.bend_skew ----------------------------------------------------
def bend_skew(P, resample=10.0, smooth=30.0):
    seg = np.hypot(*np.diff(P, axis=0).T); L = float(seg.sum())
    if L < 400: return []
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, resample)
    Q = np.stack([np.interp(ss, cum, P[:,0]), np.interp(ss, cum, P[:,1])], 1)
    k = max(int(smooth/resample), 1)
    if k > 1:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0], ker, "valid"),
                      np.convolve(Q[:,1], ker, "valid")], 1)
    if len(Q) < 12: return []
    s2 = np.hypot(*np.diff(Q, axis=0).T); c2 = np.r_[0.0, np.cumsum(s2)]
    v1 = np.diff(Q, axis=0)[:-1]; v2 = np.diff(Q, axis=0)[1:]
    cr = v1[:,0]*v2[:,1] - v1[:,1]*v2[:,0]
    n1 = np.hypot(*v1.T); n2 = np.hypot(*v2.T)
    denom = n1*n2*np.hypot(*(v1+v2).T)
    curv = np.where(denom > 1e-9, 2*cr/np.maximum(denom,1e-9), 0.0)
    sg = np.sign(curv); idx = np.where(np.diff(sg) != 0)[0]
    out = []
    for a, b in zip(idx[:-1], idx[1:]):
        if b - a < 3: continue
        arc = c2[b] - c2[a]
        if not (25 < arc < 1500): continue
        seg_c = np.abs(curv[a:b])
        if seg_c.max() <= 0: continue
        pk = a + int(np.argmax(seg_c))
        out.append(float((c2[pk] - c2[a]) / arc))
    return out

# --- creek_straight.curv_profile / straight_stats ----------------------------
R_MIN = [150.0, 300.0]

def curv_profile(P, resample=10.0, smooth=30.0):
    seg = np.hypot(*np.diff(P, axis=0).T); L = float(seg.sum())
    if L < 400: return None, None
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, resample)
    Q = np.stack([np.interp(ss,cum,P[:,0]), np.interp(ss,cum,P[:,1])],1)
    k = max(int(smooth/resample),1)
    if k > 1:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0],ker,"valid"), np.convolve(Q[:,1],ker,"valid")],1)
    if len(Q) < 12: return None, None
    v1 = np.diff(Q,axis=0)[:-1]; v2 = np.diff(Q,axis=0)[1:]
    cr = v1[:,0]*v2[:,1]-v1[:,1]*v2[:,0]
    n1 = np.hypot(*v1.T); n2 = np.hypot(*v2.T); n3 = np.hypot(*(v1+v2).T)
    den = n1*n2*n3
    c = np.where(den>1e-9, 2*np.abs(cr)/np.maximum(den,1e-9), 0.0)
    s2 = np.hypot(*np.diff(Q,axis=0).T)
    return c, s2[:len(c)]

def straight_stats(P):
    c, ds = curv_profile(P)
    if c is None: return None
    out = {}
    for rmin in R_MIN:
        flat = c < (1.0/rmin)
        frac = float((ds*flat).sum()/ds.sum())
        runs = []
        i = 0
        while i < len(flat):
            if flat[i]:
                j = i
                while j < len(flat) and flat[j]: j += 1
                runs.append(float(ds[i:j].sum())); i = j
            else: i += 1
        runs = [r for r in runs if r > 20]
        out[rmin] = (frac, float(np.percentile(runs,90)) if runs else 0.0,
                     float(np.max(runs)) if runs else 0.0)
    return out

# --- new: the routing metric on a POLYLINE -----------------------------------
def measured_line(P, resample=10.0, smooth=30.0):
    """The 10 m / 30 m line every statistic above is computed on."""
    seg = np.hypot(*np.diff(P, axis=0).T); L = float(seg.sum())
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, resample)
    Q = np.stack([np.interp(ss, cum, P[:,0]), np.interp(ss, cum, P[:,1])], 1)
    k = max(int(smooth/resample), 1)
    if k > 1 and len(Q) >= k:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0], ker, "valid"),
                      np.convolve(Q[:,1], ker, "valid")], 1)
    return Q

def _seg_cross_count(a, b, P):
    """How many segments of polyline P the chord a-b crosses."""
    p = P[:-1]; q = P[1:]
    def cr(o, u, v): return (u[...,0]-o[...,0])*(v[...,1]-o[...,1]) - (u[...,1]-o[...,1])*(v[...,0]-o[...,0])
    d1 = cr(a, b, p); d2 = cr(a, b, q)
    d3 = cr(p, q, a[None]); d4 = cr(p, q, b[None])
    return int(((d1*d2 < 0) & (d3*d4 < 0)).sum())

def multi_cross_rate_poly(P, hole_m=350.0, min_angle_deg=35.0, n=3000, seed=3):
    """Share of creek-crossing chords (angled >= min_angle to the local creek
    axis) that cross the creek MORE than once. Random-bearing chords measure
    the sampler, not the creek (docs/calibration/creek-planform.md, trap 2)."""
    Q = measured_line(P)
    if len(Q) < 12: return None
    rng = np.random.default_rng(seed)
    T = np.diff(Q, axis=0); ang = np.arctan2(T[:,1], T[:,0])
    once = multi = 0
    for _ in range(n):
        k = rng.integers(len(ang))
        rel = np.radians(rng.uniform(min_angle_deg, 180.0 - min_angle_deg))
        th = ang[k] + rel * (1 if rng.random() < 0.5 else -1)
        c = Q[k] + rng.normal(0, 8.0, 2)   # centre jitter so chords are not all through a vertex
        u = np.array([np.cos(th), np.sin(th)])
        a = c - u*hole_m/2; b = c + u*hole_m/2
        m = _seg_cross_count(a, b, Q)
        if m >= 1: once += 1
        if m >= 2: multi += 1
    return (multi/once*100.0) if once else None

def amplitude_p95(P):
    """Lateral amplitude about the reach's own axis, detrended at 8 % of length
    (mirrors creek_crossings.amplitude, on a polyline)."""
    Q = measured_line(P)
    if len(Q) < 12: return None
    Pc = Q - Q.mean(0)
    u = np.linalg.svd(Pc, full_matrices=False)[2][0]
    t = Pc @ u; nvec = np.array([-u[1], u[0]]); off = Pc @ nvec
    o = np.argsort(t); o2 = off[o]
    win = max(int(len(o2)*0.08), 5)
    base = np.convolve(o2, np.ones(win)/win, mode="same")
    return float(np.percentile(np.abs(o2-base), 95))

def all_stats(P):
    """Everything the sheet prints under a panel, or None if too short."""
    b = bends(P)
    sk = bend_skew(P)
    st = straight_stats(P)
    return dict(
        bends=(b or {}).get("n_bends"), len_p50=(b or {}).get("lam_med"),
        cv=(b or {}).get("lam_cv"), sinuosity=(b or {}).get("sinuosity"),
        skew_p50=float(np.median(sk)) if sk else None,
        straight150=st[150.0][0]*100 if st else None,
        run_p90=st[150.0][1] if st else None,
        straight300=st[300.0][0]*100 if st else None,
        xcross35=multi_cross_rate_poly(P), amp_p95=amplitude_p95(P))
