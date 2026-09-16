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
PROF_FREE_M = 1.7            # a rise above the chord under this is free (corpus p90; route.rs PROF_CHORD_FREE_M)
DOGLEG_MIN_LEG_M = 20.0      # a spine vertex counts as a dogleg only with both legs > 20 m (the corpus measurement's filter)
DOGLEG_BEND_DEG = 15.0       # "the hole bends": max turn > 15 deg (real 57-60 % of par 4/5s)
DIP_CARRY_M = 1.5            # a drive whose ground dips >= 1.5 m below its own chord carries a low point (round 2, item 2)
LEG_M = 220.0                # the drive / second-shot leg length the corpus tool uses (LZ_APPROX_M)
# BLINDNESS, measured the same way on both sides (corpus/blindness.py): an eye
# EYE_M above the ground at the observing station, the target the GROUND at a
# centreline point, the sight line straight in plan, blocked when intermediate
# ground breaks it by more than BLIND_TOL_M. Stations follow the corpus
# convention, since a real hole's landing zone is unknown: drive at
# min(220, 0.63 L), the par-5 second at +220 capped 60 m short of the green.
EYE_M = 1.7
BLIND_TOL_M = 0.3
BLIND_STEP_M = 5.0           # centreline targets
LOS_STEP_M = 2.0             # samples along a sight line
APPROACH_RESERVE_M = 60.0
DRIVE_FRAC = 0.63
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


# Round 2 real references, read from the corpus tools' outputs when present
# (tools/golf/corpus/{shot_profiles,par_positions}.py); blank otherwise.
CORPUS_OUT = HERE / "corpus" / "out"
REAL_PAR_POS = None          # [9][3] shares, par 3/4/5 per slot, from par_positions.json (all par-36 nines)
try:
    _pp = json.load(open(CORPUS_OUT / "par_positions.json"))
    REAL_PAR_POS = _pp["all"]["share"]
    REAL["hole1_par3"] = f"{100 * REAL_PAR_POS[0][0]:.0f} %"; REAL["hole2_par3"] = f"{100 * REAL_PAR_POS[1][0]:.0f} %"
    REAL["hole9_par3"] = f"{100 * REAL_PAR_POS[8][0]:.0f} %"; REAL["hole9_par5"] = f"{100 * REAL_PAR_POS[8][2]:.0f} %"
    REAL["b2b3"] = f"{100 * _pp['all']['b2b3']:.0f} %"; REAL["b2b"] = f"{100 * (_pp['all']['b2b3'] + _pp['all']['b2b5']):.0f} %"
except Exception:
    pass
try:
    _bl = json.load(open(CORPUS_OUT / "blindness.json"))
    REAL["blind_tee"] = f"{100 * np.median([r['blind_tee'] for r in _bl]):.0f} %"
    REAL["blind_appr"] = f"{100 * np.median([r['blind_appr'] for r in _bl]):.0f} %"
    _lz = [r["lz_seen"] for r in _bl if r.get("lz_seen") is not None]
    REAL["lz_blind"] = f"{100 * np.mean([not v for v in _lz]):.0f} %"
    REAL["green_blind"] = f"{100 * np.mean([not r['green_seen'] for r in _bl]):.0f} %"
    _d45 = [r["drive_block_m"] for r in _bl if r.get("drive_block_m") is not None]
    if _d45:
        REAL["deep15"] = f"{100 * np.mean(np.asarray(_d45) > 1.5):.1f}"
        REAL["deep20"] = f"{100 * np.mean(np.asarray(_d45) > 2.0):.1f}"
        _db = [v for v in _d45 if v > 0.3]
        REAL["block50"] = f"{np.percentile(_db, 50):.2f}"
        REAL["block90"] = f"{np.percentile(_db, 90):.2f}"
        _fr = [r["drive_block_frac"] for r in _bl
               if r.get("drive_block_m") is not None and r["drive_block_m"] > 0.3]
        REAL["block_frac"] = f"{np.percentile(_fr, 50):.2f}"
except Exception:
    pass
try:
    _sp = [r for r in json.load(open(CORPUS_OUT / "shot_profiles.json")) if r.get("par") in (4, 5)]
    _dog = [r["dogleg_max_deg"] for r in _sp if r.get("dogleg_max_deg") is not None]
    _dip = [r["drive"]["dip"] for r in _sp if r.get("drive")]
    if _dog:
        REAL["dogleg50"] = f"{np.percentile(_dog, 50):.0f}"; REAL["dogleg90"] = f"{np.percentile(_dog, 90):.0f}"
        REAL["dog15"] = f"{100 * np.mean(np.asarray(_dog) > DOGLEG_BEND_DEG):.0f} %"
    if _dip:
        REAL["dip_share"] = f"{100 * np.mean(np.asarray(_dip) >= DIP_CARRY_M):.0f} %"
        _d = [v for v in _dip if v >= DIP_CARRY_M]
        REAL["dip_med"] = f"{np.percentile(_d, 50):.1f}" if _d else ""
    _s5 = [r["s_shape"] for r in _sp if r.get("par") == 5 and "s_shape" in r]
    if _s5:
        REAL["s_share"] = f"{100 * np.mean(_s5):.0f} %"
except Exception:
    pass


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


# --------------------------------------------------------------- blindness --

def line_pts(poly, step):
    """(points, cumulative arc) along a polyline."""
    q = resample(np.asarray(poly, float), step)
    d = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(q, axis=0).T))])
    return q, d


def at_arc(q, d, s):
    return np.array([np.interp(s, d, q[:, 0]), np.interp(s, d, q[:, 1])])


def obstruction(t, a, b):
    """How badly the ground at `b` is hidden from an eye EYE_M above `a`:
    `(depth_m, frac, blocked_len_m)` -- the worst excess of ground over the
    sight line, where it sits as a fraction of the leg, and how much of the
    line is blocked. depth 0 means visible. The depth is the cut an
    earthmover would have to take to open the shot, which is the quantity
    `route::VIS_TIER_M` is set from (corpus/blindness.py::block_depth)."""
    dist = float(np.hypot(*(np.asarray(b, float) - np.asarray(a, float))))
    if dist < LOS_STEP_M:
        return 0.0, 0.0, 0.0
    n = max(2, int(dist / LOS_STEP_M))
    u = np.linspace(0.0, 1.0, n + 1)
    ys = a[0] + (b[0] - a[0]) * u; xs = a[1] + (b[1] - a[1]) * u
    yi = np.clip((ys / t.c).astype(int), 0, t.z.shape[0] - 1)
    xi = np.clip((xs / t.c).astype(int), 0, t.z.shape[1] - 1)
    z = t.z[yi, xi].astype(float)
    los = (z[0] + EYE_M) + (z[-1] - z[0] - EYE_M) * u
    over = z - los
    over[0] = over[-1] = -np.inf
    k = int(np.argmax(over))
    if not np.isfinite(over[k]) or over[k] <= 0.0:
        return 0.0, 0.0, 0.0
    return float(over[k]), float(u[k]), float((over > BLIND_TOL_M).sum() * dist / n)


def blocked(t, a, b):
    """Is the ground at `b` hidden from an eye EYE_M above the ground at `a`?"""
    return obstruction(t, a, b)[0] > BLIND_TOL_M


def blind_metrics(t, spine, par):
    """`blind_tee`, `blind_appr`, `lz_seen`, `green_seen` on the corpus convention."""
    q, d = line_pts(spine, BLIND_STEP_M)
    L = float(d[-1])
    if L < 100.0:
        return dict(blind_tee=None, blind_appr=None, lz_seen=None, green_seen=None)
    s_drive = min(LEG_M, DRIVE_FRAC * L)
    s_second = min(s_drive + LEG_M, L - APPROACH_RESERVE_M)
    s_last = 0.0 if par == 3 else (s_drive if par == 4 else (s_second if s_second > s_drive else s_drive))
    tee = at_arc(q, d, 0.0); last = at_arc(q, d, s_last); green = at_arc(q, d, L)
    ss = np.arange(BLIND_STEP_M, L + 1e-9, BLIND_STEP_M)
    tg = [at_arc(q, d, s) for s in ss]
    bt = [blocked(t, tee, p) for p in tg]
    ap = [p for s, p in zip(ss, tg) if s > s_last]
    ba = [blocked(t, last, p) for p in ap]
    drive = obstruction(t, tee, at_arc(q, d, s_drive)) if par >= 4 else None
    leg2 = (obstruction(t, at_arc(q, d, s_drive), at_arc(q, d, s_second))
            if par == 5 and s_second > s_drive else None)
    return dict(
        blind_tee=float(np.mean(bt)) if bt else None,
        blind_appr=float(np.mean(ba)) if ba else (float(np.mean(bt)) if bt else None),
        lz_seen=(None if par == 3 else drive[0] <= BLIND_TOL_M),
        green_seen=bool(not blocked(t, last, green)),
        drive_block_m=(drive[0] if drive else None),
        drive_block_frac=(drive[1] if drive else None),
        drive_block_len_m=(drive[2] if drive else None),
        leg2_block_m=(leg2[0] if leg2 else None))


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
    # round 2: dogleg turns at the spine's interior vertices (both legs > 20 m), signed
    turns = []
    for i in range(1, len(sp) - 1):
        u, v = sp[i] - sp[i - 1], sp[i + 1] - sp[i]
        nu, nv = np.hypot(*u), np.hypot(*v)
        if nu > DOGLEG_MIN_LEG_M and nv > DOGLEG_MIN_LEG_M:
            ang = float(np.degrees(np.arccos(np.clip(u @ v / (nu * nv), -1, 1))))
            turns.append(ang if (u[1] * v[0] - u[0] * v[1]) >= 0 else -ang)
    dogleg = max((abs(a) for a in turns), default=0.0)
    s_shape = bool(h["par"] == 5 and any(a > 10 for a in turns) and any(a < -10 for a in turns))
    # the profile of one leg against ITS OWN chord: (dip below, rise above)
    def leg(a, b):
        q = resample([a, b], PROFILE_STEP_M)
        if len(q) < 4:
            return (0.0, 0.0)
        z = ndi.uniform_filter1d(np.array([t.z2(*p) for p in q]), PROFILE_SMOOTH_N, mode="nearest")
        ch = np.linspace(z[0], z[-1], len(z))
        return (float((ch - z).max()), float((z - ch).max()))
    drive_dip, drive_rise = leg(sp[0], sp[1]) if (h["par"] >= 4 and len(sp) >= 2) else (None, None)
    second_dip = leg(sp[1], sp[2])[0] if (h["par"] == 5 and len(sp) >= 3) else None
    line_above = leg(sp[0], sp[-1])[1]      # the straight tee->green line's rise (what the beam sees)
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
        gip=[], gip35=0, gip50=0, terms=h.get("terms"), walk_m=float(h.get("walk_m", 0.0)),
        dogleg=dogleg, turns=turns, s_shape=s_shape, drive_dip=drive_dip, drive_rise=drive_rise,
        second_dip=second_dip, line_above=line_above, **blind_metrics(t, h["spine"], h["par"]))


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
        hole1_par3=pars[0] == 3, hole9_par3=pars[-1] == 3 if len(pars) >= 9 else False,
        dogleg_med=float(np.median([h["dogleg"] for h in holes if h["par"] >= 4])) if any(h["par"] >= 4 for h in holes) else 0.0,
        n_drive_blind=sum(1 for h in holes if (h.get("drive_block_m") or 0.0) > BLIND_TOL_M),
        n_drive_deep=sum(1 for h in holes if (h.get("drive_block_m") or 0.0) > 2.0),
        n_dip=sum(1 for h in holes if h["drive_dip"] is not None and h["drive_dip"] >= DIP_CARRY_M),
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


def par_pos_l1(courses):
    """Mean |our slot share - real| over the 9 x 3 par-by-hole table (par_positions.json)."""
    if REAL_PAR_POS is None or not courses:
        return float("nan")
    n = len(courses)
    d = [abs(sum(c["pars"][i] == par for c in courses) / n - REAL_PAR_POS[i][j])
         for i in range(9) for j, par in enumerate((3, 4, 5)) if all(len(c["pars"]) >= 9 for c in courses)]
    return float(np.mean(d)) if d else float("nan")


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
    dog = [h["dogleg"] for h in holes if h["par"] >= 4]
    dips = [h["drive_dip"] for h in holes if h["drive_dip"] is not None]
    dips2 = [h["second_dip"] for h in holes if h["second_dip"] is not None]
    bt = [h["blind_tee"] for h in holes if h["blind_tee"] is not None]
    ba = [h["blind_appr"] for h in holes if h["blind_appr"] is not None]
    lzs_ = [h["lz_seen"] for h in holes if h["lz_seen"] is not None]
    dbk = [h["drive_block_m"] for h in holes if h["drive_block_m"] is not None]
    dbb = [v for v in dbk if v > BLIND_TOL_M]
    dfr = [h["drive_block_frac"] for h in holes
           if h["drive_block_m"] is not None and h["drive_block_m"] > BLIND_TOL_M]
    l2 = [h["leg2_block_m"] for h in holes if h["leg2_block_m"] is not None]
    gs_ = [h["green_seen"] for h in holes if h["green_seen"] is not None]
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
        b2b=pct(c["b2b3"] or c["b2b5"] for c in courses), b2b3=pct(c["b2b3"] for c in courses),
        hole1_par3=pct(c["hole1_par3"] for c in courses), hole9_par3=pct(c["hole9_par3"] for c in courses),
        par_pos_l1=par_pos_l1(courses),
        dogleg50=float(np.percentile(dog, 50)) if dog else float("nan"), dogleg90=float(np.percentile(dog, 90)) if dog else float("nan"),
        dog15=pct(v > DOGLEG_BEND_DEG for v in dog), s_share=pct(h["s_shape"] for h in holes if h["par"] == 5),
        dip_share=pct(v >= DIP_CARRY_M for v in dips), dip_med=float(np.percentile([v for v in dips if v >= DIP_CARRY_M], 50)) if any(v >= DIP_CARRY_M for v in dips) else float("nan"),
        dip2_share=pct(v >= DIP_CARRY_M for v in dips2),
        blind_tee=100.0 * float(np.median(bt)) if bt else float("nan"),
        blind_appr=100.0 * float(np.median(ba)) if ba else float("nan"),
        lz_blind=pct(not v for v in lzs_), green_blind=pct(not v for v in gs_),
        deep15=(100.0 * sum(1 for v in dbk if v > 1.5) / len(dbk)) if dbk else float("nan"),
        deep20=(100.0 * sum(1 for v in dbk if v > 2.0) / len(dbk)) if dbk else float("nan"),
        block50=float(np.percentile(dbb, 50)) if dbb else float("nan"),
        block90=float(np.percentile(dbb, 90)) if dbb else float("nan"),
        block_frac=float(np.percentile(dfr, 50)) if dfr else float("nan"),
        leg2_blind=pct(v > BLIND_TOL_M for v in l2),
        bend_fix=pct(h["above"] < PROF_FREE_M for h in holes if h["par"] >= 4 and h["line_above"] >= PROF_FREE_M),
        line_rise=pct(h["line_above"] >= PROF_FREE_M for h in holes if h["par"] >= 4),
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
    ("hole 1 a par 3, %", "hole1_par3", "{:.0f}", REAL.get("hole1_par3", "")),
    ("hole 2 a par 3, %", "hole2_par3", "{:.0f}", REAL.get("hole2_par3", "")),
    ("hole 9 a par 3, %", "hole9_par3", "{:.0f}", REAL.get("hole9_par3", "")),
    ("hole 9 a par 5, %", "hole9_par5", "{:.0f}", REAL.get("hole9_par5", "")),
    ("back-to-back 3s, %", "b2b3", "{:.0f}", REAL.get("b2b3", "")),
    ("back-to-back 3s or 5s, %", "b2b", "{:.0f}", REAL.get("b2b", "")),
    ("par-by-hole table, mean abs(ours - real) over 27 slots", "par_pos_l1", "{:.3f}", ""),
    ("dogleg p50, deg (par 4/5)", "dogleg50", "{:.0f}", REAL.get("dogleg50", "18-20")),
    ("dogleg p90, deg", "dogleg90", "{:.0f}", REAL.get("dogleg90", "44-46")),
    ("holes bending > 15 deg, %", "dog15", "{:.0f}", REAL.get("dog15", "57-60 %")),
    ("par 5s with an S-shape, %", "s_share", "{:.0f}", REAL.get("s_share", "")),
    ("straight line crosses a >= 1.7 m rise, % of par 4/5", "line_rise", "{:.0f}", ""),
    ("... of those, the placed spine stays < 1.7 m, %", "bend_fix", "{:.0f}", ""),
    ("ground blind from the tee, p50 % of the line", "blind_tee", "{:.0f}", REAL.get("blind_tee", "")),
    ("ground blind from the approach station, p50 %", "blind_appr", "{:.0f}", REAL.get("blind_appr", "")),
    ("landing zone blind from the tee, %", "lz_blind", "{:.0f}", REAL.get("lz_blind", "")),
    ("blind drives needing > 1.5 m of cut, per 100 par 4/5", "deep15", "{:.1f}", REAL.get("deep15", "")),
    ("blind drives needing > 2 m of cut, per 100 par 4/5", "deep20", "{:.1f}", REAL.get("deep20", "")),
    ("drive obstruction depth p50 / p90, m", "block50", "{:.2f}", REAL.get("block50", "")),
    ("   p90", "block90", "{:.2f}", REAL.get("block90", "")),
    ("obstruction position, fraction to the landing zone", "block_frac", "{:.2f}", REAL.get("block_frac", "")),
    ("par-5 second leg blind, %", "leg2_blind", "{:.0f}", ""),
    ("green blind from the approach station, %", "green_blind", "{:.0f}", REAL.get("green_blind", "")),
    ("drives carrying a >= 1.5 m dip, %", "dip_share", "{:.0f}", REAL.get("dip_share", "")),
    ("depth of those dips p50, m", "dip_med", "{:.1f}", REAL.get("dip_med", "")),
    ("par-5 second shots carrying a >= 1.5 m dip, %", "dip2_share", "{:.0f}", ""),
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
