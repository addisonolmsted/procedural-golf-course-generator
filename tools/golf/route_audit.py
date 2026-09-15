"""Terrain audit of routed courses from batch records, before and after.

  python3 tools/golf/route_audit.py <dump_dir> <a.jsonl> [<b.jsonl>] [--holes out.json] [--prefix m_]

Every hole of every routed record is measured on its own tile (the 2 m
height grid and the water surface in <dump_dir>), the way the 2026-09-14
routing audit did (scratch `routeq/metrics.py`, promoted here):

  per hole    length, net dz, above chord (PROFILE_STEP_M samples along the
              spine, PROFILE_SMOOTH_N-sample smoothing), climb and climb per
              100 m, landing-zone slope / relief position / |tpi200| / water
              distance, green slope / relief position / tpi200 / surround
              (p90 - p10 of the 2 m ground in a 25-90 m annulus), the green's
              signed distance to the play window (negative outside), the five
              tee-box slopes (sampled here, the same for a v1 and a v2 record),
              carries (record-v2 spine bridges with a span in
              [CARRY_MIN_M, CARRY_MAX_M]; walk bridges read from `walk.bridges`), and the green-in-play cases: this
              green against every OTHER hole's spine (distance and arc
              position), exempting only the next hole's first
              GIP_EXEMPT_ARC_M of arc (and hole 1's against the last green: the loop junction).
  per course  coverage (share of the window's 8 m cells within 100 / 120 m of
              a spine), the sequence signatures (hole 2 a par 3, hole 9 a par
              5), the mix, back-to-back 3s / 5s, per-par length ranges and the
              "one par 3 <= 150 m" flag, total, walk, crossings, seconds.

With two jsonl the before/after table is printed (all / aeolian / fluvial,
medians or shares) with the real-corpus references where the plan lists
them; `--holes out.json` writes the per-hole and per-course records, which
`route_before_after.py` also computes in-process by importing this module.

Records may be v1 (rs_v1.jsonl) or v2 (`rec_v: 2`, extra optional keys);
the v2 keys are read when present and every metric degrades to the v1
value when they are absent (carries 0, crossings from `n_crossings`).

Measurement only; nothing here changes a route.
"""
import sys, json, math, pathlib
import numpy as np
from scipy import ndimage as ndi
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                      # noqa: E402

C8 = 8.0                     # analysis grid (m): the 2 m tile decimated by 4, as the audit did
PROFILE_STEP_M = 2.0         # spine profile sample spacing (m)
PROFILE_SMOOTH_N = 3         # samples in the moving mean over the profile
COVER_STEP_M = 4.0           # spine rasterisation step for coverage (m)
GIP_EXEMPT_ARC_M = 60.0      # the next hole's first 60 m of arc is the junction, not "in play"
GIP_NEAR_M, GIP_FAR_M = 35.0, 50.0
CARRY_MIN_M, CARRY_MAX_M = 15.0, 70.0   # a spine bridge of this span is a forced carry (plan item 8)
EDGE_NEAR_M = 60.0           # green "near the edge" band
LZ_FLAT = 0.02               # dead-flat landing zone: slope < 2 %
LZ_FEAT_MIN = 0.5            # a v2 record's terms.lz_interest40 (the router's interest raster maxed over 40 m) at or above this = "a feature in reach"
UPLAND_RP = 0.7              # upland: relief position > 0.7
WATER_NEAR_M = 40.0          # a hole is "near water" when its spine passes within 40 m
HAZARD_M = 60.0              # an LZ / green within 60 m of water (the viewer's blue rings)
WATER_TILE_FRAC = 0.002      # a "water course": wet share of the tile > 0.2 % (the audit's 71 of 250)
TEE_STEEP, TEE_CLIFF = 0.15, 0.30
PAR3_PAIR_M = 20.0           # both par 3s within 20 m of each other
PAR3_SHORT_M = 150.0
MIX_252 = (2, 5, 2)

# Real references (tools/golf/corpus/out/hole_profiles.json, 5,201 holes; features.jsonl, 9,119 greens)
REAL = {"par3_med": 163.0, "par4_med": 351.0, "par5_med": 474.0,
        "above50": 0.08, "above90": 1.67, "above99": 5.6,
        "g_sur": "6.1-6.8", "g_upland": "25 %"}   # real relief_pos p75 = 0.67 -> ~25 % above 0.7


# ---------------------------------------------------------------- terrain --

class Terrain:
    """One tile: the 2 m ground + wet mask, and the 8 m analysis fields."""

    def __init__(self, dump, prefix, seed):
        dump = pathlib.Path(dump)
        z, (_, _, c) = cgrid.read_f32(str(dump / f"{prefix}{seed}.cgrid"))
        self.z = z.astype(float); self.c = float(c)
        wp = dump / f"{prefix}{seed}.water.cgrid"
        self.wet = np.isfinite(cgrid.read_f32(str(wp))[0]) if wp.exists() else np.zeros(self.z.shape, bool)
        k = max(1, int(round(C8 / self.c)))
        z8 = self.z[::k, ::k]; self.z8 = z8
        gy, gx = np.gradient(z8, C8); self.slope = np.hypot(gx, gy)
        lp = ndi.gaussian_filter(z8, 400.0 / np.pi / C8)
        self.rp = np.argsort(np.argsort(lp.ravel(), kind="stable"), kind="stable").reshape(z8.shape) / (z8.size - 1)
        self.tpi = z8 - ndi.uniform_filter(z8, int(200 / C8) | 1)
        wet8 = self.wet[::k, ::k]; self.wet8 = wet8
        self.dw = ndi.distance_transform_edt(~wet8, sampling=C8) if wet8.any() else np.full(z8.shape, np.inf)
        self.tile_wet = float(wet8.mean())
        self.water_tile = self.tile_wet > WATER_TILE_FRAC

    def s8(self, a, y, x):
        i = int(np.clip(round(y / C8), 0, a.shape[0] - 1)); j = int(np.clip(round(x / C8), 0, a.shape[1] - 1))
        return float(a[i, j])

    def z2(self, y, x):
        i = int(np.clip(y / self.c, 0, self.z.shape[0] - 1)); j = int(np.clip(x / self.c, 0, self.z.shape[1] - 1))
        return float(self.z[i, j])


# ---------------------------------------------------------------- geometry --

def resample(poly, step):
    """Points every `step` m along a polyline (the last vertex included)."""
    sp = np.asarray(poly, float)
    if len(sp) < 2:
        return sp
    pts = []
    for a, b in zip(sp[:-1], sp[1:]):
        n = max(2, int(np.hypot(*(b - a)) / step))
        pts += [a + (b - a) * t for t in np.linspace(0, 1, n, endpoint=False)]
    pts.append(sp[-1])
    return np.array(pts)


def point_to_polyline(poly, p):
    """(distance, arc position from poly[0]) of the point of `poly` nearest `p`."""
    sp = np.asarray(poly, float); p = np.asarray(p, float)
    best, s = (float("inf"), 0.0), 0.0
    for a, b in zip(sp[:-1], sp[1:]):
        ab = b - a; L2 = float(ab @ ab); L = math.sqrt(L2)
        t = 0.0 if L2 == 0 else float(np.clip((p - a) @ ab / L2, 0, 1))
        d = float(np.hypot(*(p - (a + t * ab))))
        if d < best[0]:
            best = (d, s + t * L)
        s += L
    return best


def edge_distance(g, window):
    """Signed distance of a point to the play window's edge: positive inside, negative outside."""
    wy, wx, hh, ww = window
    dy = max(wy - g[0], g[0] - (wy + hh), 0.0); dx = max(wx - g[1], g[1] - (wx + ww), 0.0)
    if dx > 0 or dy > 0:
        return -float(np.hypot(dx, dy))
    return float(min(g[0] - wy, wy + hh - g[0], g[1] - wx, wx + ww - g[1]))


def bridges(h):
    """(spine bridges, walk bridges) of a v2 hole: `bridges` by kind, plus `walk.bridges`; ([], []) on a v1 count."""
    b = h.get("bridges") if isinstance(h.get("bridges"), list) else []
    w = h["walk"].get("bridges", []) if isinstance(h.get("walk"), dict) else []
    spine = [x for x in b if x.get("kind", "spine") != "walk"]
    walk = [x for x in b if x.get("kind", "spine") == "walk"] + list(w)
    return spine, walk


def bridge_spans(h):
    """(spine spans, walk spans) in metres."""
    spine, walk = bridges(h)
    return [float(x["span_m"]) for x in spine], [float(x["span_m"]) for x in walk]


# ----------------------------------------------------------------- metrics --

def hole_metrics(t, rec, k):
    h = rec["holes"][k]
    sp = np.asarray(h["spine"], float); g = np.asarray(h["green"], float)
    pts = resample(sp, PROFILE_STEP_M)
    zz = np.array([t.z2(*p) for p in pts])
    zz = ndi.uniform_filter1d(zz, PROFILE_SMOOTH_N, mode="nearest")
    arc = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(pts, axis=0).T))])
    chord = zz[0] + (zz[-1] - zz[0]) * arc / max(arc[-1], 1.0)
    dz = np.diff(zz); climb = float(np.sum(dz[dz > 0]))
    lz = [(t.s8(t.slope, ly, lx), t.s8(t.rp, ly, lx), t.s8(t.tpi, ly, lx), t.s8(t.dw, ly, lx)) for (ly, lx, lr) in h["lzs"]]
    # green surround relief: p90 - p10 of the 2 m ground in the 25-90 m annulus
    r = int(90 / t.c)
    yy, xx = np.mgrid[-r:r + 1, -r:r + 1]; rho = np.hypot(yy, xx) * t.c; ann = (rho >= 25) & (rho <= 90)
    gi, gj = int(g[0] / t.c), int(g[1] / t.c)
    i0, i1 = max(0, gi - r), min(t.z.shape[0], gi + r + 1); j0, j1 = max(0, gj - r), min(t.z.shape[1], gj + r + 1)
    patch = t.z[i0:i1, j0:j1]; annp = ann[i0 - (gi - r):i0 - (gi - r) + (i1 - i0), j0 - (gj - r):j0 - (gj - r) + (j1 - j0)]
    sur = float(np.percentile(patch[annp], 90) - np.percentile(patch[annp], 10)) if annp.any() else 0.0
    spine_spans, walk_spans = bridge_spans(h)
    carries = [s for s in spine_spans if CARRY_MIN_M <= s <= CARRY_MAX_M]
    # tee slopes are measured here on the 8 m grid for every build (the audit's 329 / 39 baseline);
    # a v2 record's own `tee_slope` (the router's cell slope) is kept beside it
    tee_slope = [t.s8(t.slope, ty, tx) for (ty, tx) in h["tees"]]
    g_dw = t.s8(t.dw, *g)
    d_spine_water = float(min(t.s8(t.dw, *p) for p in resample(sp, C8)))
    return dict(
        seed=rec["seed"], mode=rec["mode"], hole=k, par=int(h["par"]), kind=h.get("kind"),
        length=float(h["length_m"]), net_dz=float(zz[-1] - zz[0]), above=float((zz - chord).max()),
        climb=climb, climb100=climb / max(arc[-1], 1.0) * 100.0,
        lz_slope=[v[0] for v in lz], lz_rp=[v[1] for v in lz], lz_tpi=[abs(v[2]) for v in lz], lz_dw=[v[3] for v in lz],
        g_slope=t.s8(t.slope, *g), g_rp=t.s8(t.rp, *g), g_tpi=t.s8(t.tpi, *g), g_dw=g_dw, g_sur=sur,
        g_edge=edge_distance(g, rec["window_m"]), d_spine_water=d_spine_water,
        tee_slope=tee_slope, tee_slope_rec=h.get("tee_slope"), tee_graded=h.get("tee_graded"),
        carries=carries, n_carries=len(carries), spine_spans=spine_spans, walk_spans=walk_spans,
        long_carries=sum(1 for s in spine_spans if s > CARRY_MAX_M),
        water_lz=[bool(v[3] <= HAZARD_M) for v in lz], water_green=bool(g_dw <= HAZARD_M),
        gip=[], gip35=0, gip50=0, terms=h.get("terms"), walk_m=float(h.get("walk_m", 0.0)))


def green_in_play(rec, holes):
    """Every (green of A, spine of B != A) case within GIP_FAR_M, on A's record."""
    hs = rec["holes"]; n = len(hs)
    for a in range(n):
        g = hs[a]["green"]; cases = []
        for b in range(n):
            if b == a:
                continue
            d, s = point_to_polyline(hs[b]["spine"], g)
            if s <= 0.0:          # abeam of or behind B's tee: not in B's line of play
                continue
            # the next hole's first 60 m of arc is the junction; hole 1's first
            # 60 m against the last green is the clubhouse loop junction (both
            # pinned to the clubhouse disc), exempt the same way
            if (b == a + 1 or (a == n - 1 and b == 0)) and s < GIP_EXEMPT_ARC_M:
                continue
            if d <= GIP_FAR_M:
                cases.append(dict(other=b, d=d, arc=s, arc_frac=s / max(float(hs[b]["length_m"]), 1.0)))
        holes[a]["gip"] = cases
        holes[a]["gip35"] = sum(c["d"] <= GIP_NEAR_M for c in cases)
        holes[a]["gip50"] = len(cases)


def coverage(t, rec, radii=(100.0, 120.0)):
    """Share of the play window's 8 m cells within each radius of any spine."""
    on = np.zeros(t.z8.shape, bool)
    for h in rec["holes"]:
        p = resample(h["spine"], COVER_STEP_M)
        ii = np.clip(np.round(p[:, 0] / C8).astype(int), 0, on.shape[0] - 1)
        jj = np.clip(np.round(p[:, 1] / C8).astype(int), 0, on.shape[1] - 1)
        on[ii, jj] = True
    d = ndi.distance_transform_edt(~on, sampling=C8) if on.any() else np.full(on.shape, np.inf)
    wy, wx, hh, ww = rec["window_m"]
    i0, i1 = int(wy / C8), int(math.ceil((wy + hh) / C8)); j0, j1 = int(wx / C8), int(math.ceil((wx + ww) / C8))
    win = d[max(0, i0):i1, max(0, j0):j1]
    return {r: float((win <= r).mean()) if win.size else 0.0 for r in radii}


def course_metrics(t, rec, holes):
    pars = [int(p) for p in rec["pars"]]
    lens = {p: [h["length"] for h in holes if h["par"] == p] for p in (3, 4, 5)}
    rng = {p: (max(v) - min(v)) if len(v) > 1 else 0.0 for p, v in lens.items()}
    cov = coverage(t, rec)
    lz_rp = [v for h in holes for v in h["lz_rp"]]
    tee = [v for h in holes for v in h["tee_slope"]]
    cr = rec.get("crossings")
    mix = (pars.count(3), pars.count(4), pars.count(5))
    return dict(
        seed=rec["seed"], mode=rec["mode"], routed=True, pars=pars, mix=mix, mix252=mix == MIX_252,
        cov100=cov[100.0], cov120=cov[120.0], cov_rec=rec.get("coverage"),
        hole2_par3=pars[1] == 3 if len(pars) > 1 else False, hole9_par5=pars[-1] == 5 if len(pars) >= 9 else False,
        b2b3=any(a == b == 3 for a, b in zip(pars[:-1], pars[1:])), b2b5=any(a == b == 5 for a, b in zip(pars[:-1], pars[1:])),
        len_min={p: (min(v) if v else None) for p, v in lens.items()}, len_max={p: (max(v) if v else None) for p, v in lens.items()},
        len_range=rng, par3_pair20=len(lens[3]) >= 2 and rng[3] <= PAR3_PAIR_M, par3_short=any(v <= PAR3_SHORT_M for v in lens[3]),
        total=float(rec["total_length_m"]), walk=float(rec.get("total_walk_m", 0.0)),
        crossings=len(cr) if isinstance(cr, list) else int(rec.get("n_crossings", 0)), crossings_xy=cr,
        seconds=float(rec.get("seconds", 0.0)), score=float(rec.get("score", 0.0)),
        n_gip35=sum(h["gip35"] for h in holes), n_gip50=sum(h["gip50"] for h in holes), gip_rec=rec.get("green_in_play"),
        tee15=sum(v > TEE_STEEP for v in tee), tee30=sum(v > TEE_CLIFF for v in tee), n_tee=len(tee),
        edge60=sum(h["g_edge"] < EDGE_NEAR_M for h in holes), outside=sum(h["g_edge"] < 0 for h in holes),
        g_sur=float(np.median([h["g_sur"] for h in holes])) if holes else 0.0,
        lz_feat=sum(1 for h in holes if h["par"] >= 4 and ((h.get("terms") or {}).get("lz_interest40") or 0.0) >= LZ_FEAT_MIN),
        n_lz_holes=sum(1 for h in holes if h["par"] >= 4),
        n_lz=len(lz_rp), lz_flat=sum(v < LZ_FLAT for h in holes for v in h["lz_slope"]), lz_upland=sum(v > UPLAND_RP for v in lz_rp),
        lz_rp_std=float(np.std(lz_rp)) if len(lz_rp) >= 2 else None,
        g_sur_med=float(np.median([h["g_sur"] for h in holes])), above_max=max(h["above"] for h in holes),
        water_tile=t.water_tile, tile_wet=t.tile_wet, near_water=sum(h["d_spine_water"] <= WATER_NEAR_M for h in holes),
        n_carries=sum(h["n_carries"] for h in holes), long_carries=sum(h["long_carries"] for h in holes),
        n_hazard=sum(h["water_green"] + sum(h["water_lz"]) for h in holes),
        len_fit=sum(abs(h["length"] - REAL[f"par{h['par']}_med"]) for h in holes if f"par{h['par']}_med" in REAL),
        sequence=int(pars[1] == 3 if len(pars) > 1 else 0) + int(pars[-1] == 5 if len(pars) >= 9 else 0),
        terms=rec.get("terms"))


def audit(dump, prefix, builds, verbose=False):
    """builds: list of record lists. -> per build (holes, courses); terrain loaded once per seed."""
    by_seed = [{r["seed"]: r for r in b if r.get("routed")} for b in builds]
    seeds = sorted(set().union(*[set(d) for d in by_seed]))
    out = [([], []) for _ in builds]
    for n, s in enumerate(seeds):
        t = Terrain(dump, prefix, s)
        for bi, d in enumerate(by_seed):
            rec = d.get(s)
            if rec is None:
                continue
            hs = [hole_metrics(t, rec, k) for k in range(len(rec["holes"]))]
            green_in_play(rec, hs)
            out[bi][0].extend(hs); out[bi][1].append(course_metrics(t, rec, hs))
        if verbose and (n + 1) % 25 == 0:
            print(f"  {n + 1}/{len(seeds)} seeds", file=sys.stderr, flush=True)
    return out


# --------------------------------------------------------------- aggregate --

def pct(xs):
    xs = list(xs)
    return 100.0 * sum(bool(x) for x in xs) / len(xs) if xs else float("nan")


def med(xs, scale=1.0):
    xs = [x for x in xs if x is not None]
    return float(np.median(xs)) * scale if xs else float("nan")


def aggregate(recs, holes, courses, mode="all"):
    """One column of the table: medians and shares over the routed courses of `mode`."""
    if mode != "all":
        recs = [r for r in recs if r["mode"] == mode]; holes = [h for h in holes if h["mode"] == mode]
        courses = [c for c in courses if c["mode"] == mode]
    A = {"n": len(recs), "routed": f"{len(courses)} / {len(recs)}"}
    if not courses:
        return A
    wc = [c for c in courses if c["water_tile"]]
    lz_slope = [v for h in holes for v in h["lz_slope"]]; lz_rp = [v for h in holes for v in h["lz_rp"]]
    above = [h["above"] for h in holes]; tee = [v for h in holes for v in h["tee_slope"]]
    A.update(
        edge60=pct(h["g_edge"] < EDGE_NEAR_M for h in holes), outside=pct(h["g_edge"] < 0 for h in holes),
        cov100=med((c["cov100"] for c in courses), 100.0), cov120=med((c["cov120"] for c in courses), 100.0),
        lz_flat=pct(v < LZ_FLAT for v in lz_slope), lz_upland=pct(v > UPLAND_RP for v in lz_rp),
        lz_std=med(c["lz_rp_std"] for c in courses),
        lz_feat=(100.0 * sum(c["lz_feat"] for c in courses) / max(1, sum(c["n_lz_holes"] for c in courses))),
        g_sur=med(h["g_sur"] for h in holes), g_upland=pct(h["g_rp"] > UPLAND_RP for h in holes),
        above50=float(np.percentile(above, 50)), above90=float(np.percentile(above, 90)), above99=float(np.percentile(above, 99)),
        water_near=f"{sum(c['near_water'] > 0 for c in wc)} / {len(wc)}",
        carries=(sum(c["n_carries"] for c in wc) / len(wc)) if wc else float("nan"),
        carry_share=pct(c["n_carries"] > 0 for c in wc), long_carries=sum(c["long_carries"] for c in courses),
        par3_med=med(h["length"] for h in holes if h["par"] == 3), par4_med=med(h["length"] for h in holes if h["par"] == 4),
        par5_med=med(h["length"] for h in holes if h["par"] == 5),
        par3_pair20=pct(c["par3_pair20"] for c in courses), par3_short=pct(c["par3_short"] for c in courses),
        hole2_par3=pct(c["hole2_par3"] for c in courses), hole9_par5=pct(c["hole9_par5"] for c in courses),
        b2b=pct(c["b2b3"] or c["b2b5"] for c in courses),
        gip35=sum(c["n_gip35"] for c in courses), gip50=sum(c["n_gip50"] for c in courses),
        tee15=f"{sum(v > TEE_STEEP for v in tee)} / {len(tee)}", tee30=f"{sum(v > TEE_CLIFF for v in tee)} / {len(tee)}",
        crossings=sum(c["crossings"] for c in courses), mix252=pct(c["mix252"] for c in courses),
        total=med(c["total"] for c in courses), walk=med(c["walk"] for c in courses),
        sec50=float(np.percentile([c["seconds"] for c in courses], 50)), sec90=float(np.percentile([c["seconds"] for c in courses], 90)),
        secmax=max(c["seconds"] for c in courses), score=med(c["score"] for c in courses))
    return A


# (label, key, format, real reference) -- the plan's acceptance table, in its order
ROWS = [
    ("greens < 60 m from the window edge, %", "edge60", "{:.0f}", ""),
    ("greens outside the window, %", "outside", "{:.0f}", ""),
    ("window within 100 m of play, p50 %", "cov100", "{:.0f}", ""),
    ("window within 120 m of play, p50 %", "cov120", "{:.0f}", ""),
    ("LZ dead flat (< 2 %), %", "lz_flat", "{:.0f}", ""),
    ("LZ upland (relief pos > 0.7), %", "lz_upland", "{:.0f}", ""),
    ("par 4/5 holes with a feature within 40 m of an LZ, %", "lz_feat", "{:.0f}", ""),
    ("within-course LZ setting std, p50", "lz_std", "{:.2f}", ""),
    ("green surround relief p50, m", "g_sur", "{:.1f}", REAL["g_sur"]),
    ("green upland (relief pos > 0.7), %", "g_upland", "{:.0f}", REAL["g_upland"]),
    ("above chord p50, m", "above50", "{:.2f}", "{:.2f}".format(REAL["above50"])),
    ("above chord p90, m", "above90", "{:.1f}", "{:.1f}".format(REAL["above90"])),
    ("above chord p99, m", "above99", "{:.1f}", "{:.1f}".format(REAL["above99"])),
    ("water courses with a hole within 40 m of water", "water_near", "{}", ""),
    ("carries per water course", "carries", "{:.2f}", ""),
    ("water courses with a carry, %", "carry_share", "{:.0f}", ""),
    ("spine bridges over 70 m", "long_carries", "{}", ""),
    ("par 3 median, m", "par3_med", "{:.0f}", "{:.0f}".format(REAL["par3_med"])),
    ("par 3 pair within 20 m, %", "par3_pair20", "{:.0f}", ""),
    ("courses with a par 3 <= 150 m, %", "par3_short", "{:.0f}", ""),
    ("par 4 median, m", "par4_med", "{:.0f}", "{:.0f}".format(REAL["par4_med"])),
    ("par 5 median, m", "par5_med", "{:.0f}", "{:.0f}".format(REAL["par5_med"])),
    ("total length p50, m", "total", "{:.0f}", ""),
    ("hole 2 a par 3, %", "hole2_par3", "{:.0f}", ""),
    ("hole 9 a par 5, %", "hole9_par5", "{:.0f}", ""),
    ("back-to-back 3s or 5s, %", "b2b", "{:.0f}", ""),
    ("green in play within 35 m, cases", "gip35", "{}", ""),
    ("green in play within 50 m, cases", "gip50", "{}", ""),
    ("tee boxes on > 15 % ground", "tee15", "{}", ""),
    ("tee boxes on > 30 % ground", "tee30", "{}", ""),
    ("routed", "routed", "{}", ""),
    ("play crossings", "crossings", "{}", ""),
    ("(2,5,2) mix, %", "mix252", "{:.0f}", ""),
    ("walk p50, m", "walk", "{:.0f}", ""),
    ("score p50", "score", "{:.1f}", ""),
    ("seconds p50", "sec50", "{:.2f}", ""),
    ("seconds p90", "sec90", "{:.2f}", ""),
    ("seconds max", "secmax", "{:.2f}", ""),
]
MODES = ("all", "aeolian", "fluvial")


def fmt(A, key, f):
    v = A.get(key)
    if v is None:
        return "-"
    if isinstance(v, str):
        return v
    if isinstance(v, float) and math.isnan(v):
        return "-"
    return f.format(v)


def columns(recs, holes, courses):
    return {m: aggregate(recs, holes, courses, m) for m in MODES}


def print_table(names, cols, file=sys.stdout):
    """cols: per build {mode: aggregate}."""
    w = 11
    head = f"{'metric':46} {'real':>9}"
    sub = f"{'':46} {'':>9}"
    for nm, cl in zip(names, cols):
        head += f" | {nm[:3 * w + 6]:^{3 * w + 6}}"
        sub += " | " + "  ".join(f"{m + ' (' + str(cl[m]['n']) + ')':>{w}}" for m in MODES)
    print(head, file=file); print(sub, file=file); print("-" * len(sub), file=file)
    for label, key, f, real in ROWS:
        line = f"{label:46} {real:>9}"
        for cl in cols:
            line += " | " + "  ".join(f"{fmt(cl[m], key, f):>{w}}" for m in MODES)
        print(line, file=file)


def html_table(names, cols):
    hd = "".join(f'<th colspan="3">{nm}</th>' for nm in names)
    sub = "".join(f"<th>{m} ({cl[m]['n']})</th>" for cl in cols for m in MODES)
    body = []
    for label, key, f, real in ROWS:
        cells = "".join(f"<td>{fmt(cl[m], key, f)}</td>" for cl in cols for m in MODES)
        body.append(f"<tr><td>{label}</td><td>{real}</td>{cells}</tr>")
    return (f'<table><thead><tr><th></th><th>real</th>{hd}</tr><tr><th></th><th></th>{sub}</tr></thead>'
            f'<tbody>{"".join(body)}</tbody></table>')


def load(path):
    return [json.loads(l) for l in open(path) if l.strip()]


def main():
    a = sys.argv[1:]
    prefix, holes_out = "m_", None
    for k in ("--prefix", "--holes"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--prefix": prefix = v
            else: holes_out = v
    if len(a) < 2:
        print(__doc__); sys.exit(1)
    dump = pathlib.Path(a[0]); paths = a[1:3]
    builds = [load(p) for p in paths]
    res = audit(dump, prefix, builds, verbose=True)
    names = [pathlib.Path(p).stem for p in paths]
    cols = [columns(b, h, c) for b, (h, c) in zip(builds, res)]
    print_table(names, cols)
    if holes_out:
        json.dump({"builds": [dict(path=p, holes=h, courses=c) for p, (h, c) in zip(paths, res)]}, open(holes_out, "w"))
        print(f"wrote {holes_out}")


if __name__ == "__main__":
    main()
