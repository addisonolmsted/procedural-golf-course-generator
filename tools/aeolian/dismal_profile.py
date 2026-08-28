#!/usr/bin/env python3
"""What does the Dismal River actually look like? (round 10)

The round-9 gorge was reverted because it read as a smooth-walled trough. The
reference is the Dismal reach through Bessey NF: a *narrow* incised inner gorge
with thin, sharp, dendritic tributary catenas biting into its walls, sitting
inside a much wider and gentler apron that grades up into untouched dunes.

    python3 tools/aeolian/dismal_profile.py real
    python3 tools/aeolian/dismal_profile.py gen <dir>
    python3 tools/aeolian/dismal_profile.py compare <gen_dir>

WHY TRANSECTS AND NOT HAND. The first cut of this tool used height-above-
nearest-drainage. That is wrong here and the numbers it produced were garbage
(a 2.2 km "gorge"). The Sandhills drainage is non-integrated: most of the
landscape sits in closed interdune basins that never reach the river. A
priority-flood fill hands every one of those basins an outlet, so HAND anchors
each cell to its local swale rather than to the river, and interdune lows sit
only a few metres above their own swale. Everything the river actually does is
invisible to that measure. So: extract the trunk, and measure the terrain
against the TRUNK, along perpendicular transects.

Definitions, all measured rather than assumed:

  trunk       the longest flow path with upslope area > TRUNK_AREA_KM2,
              verified by overlay against the hillshade before use.
  axis        the trunk smoothed at AXIS_SIGMA_M, so transects run across the
              valley rather than across a meander bend.
  upland      median elevation in the far window |d| in [FAR0, FAR1] -- the
              dune surface the river has not touched.
  hw(f)       half-width: distance from the axis at which the surface first
              reaches fraction f of the way from the channel up to upland.
              hw(0.25) is the incised inner gorge, hw(0.90) the full apron.
  crenulation std/mean of hw(0.5) along the trunk after removing a long-wave
              trend -- the sawtooth amplitude the tributaries cut into the rim.
  notch pitch dominant wavelength of that residual: tributary spacing.
"""
from __future__ import annotations

import pathlib
import sys

import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
for p in ("macro_campaign", "metrics"):
    sys.path.insert(0, str(ROOT / "tools" / p))

from macro_campaign import cgrid, flow          # noqa: E402
from metrics import core                        # noqa: E402

REAL_DIR = ROOT / "tools" / "macro_campaign" / "out" / "tiles" / "sandhills_river"
DISMAL = ["t04174_10120", "t04176_10100", "t04179_10075", "t04179_10060",
          "t04181_10045", "t04183_10025", "t04184_10014"]

TRUNK_AREA_KM2 = 1.0     # verified by overlay: traces the incised gorge
AXIS_SIGMA_M = 300.0     # smooth the trunk into a valley axis
STEP_M = 20.0            # spacing of transects along the axis
HALF_M = 1400.0          # transect reach either side
DS_M = 4.0               # sampling across the transect
FAR0, FAR1 = 1100.0, 1400.0
DETREND_SIGMA_M = 400.0  # long-wave trend removed before crenulation


# ---------------------------------------------------------------- trunk

def trunk_polyline(z, cell):
    """Longest high-accumulation flow path, as (y, x) in cells."""
    zf = core.fill_depressions(z, cell)
    rec, _ = flow.receivers(zf, cell)
    acc = flow.accumulate(rec) * cell * cell
    chan = acc > TRUNK_AREA_KM2 * 1e6
    if chan.sum() < 20:
        return None
    r, a = rec.ravel(), acc.ravel()
    idx = int(np.argmax(np.where(chan.ravel(), a, 0)))
    path, seen = [], set()
    while idx not in seen:
        seen.add(idx); path.append(idx)
        j = int(r[idx])
        if j < 0:
            break
        idx = j
    up = {}
    for i in np.flatnonzero(chan.ravel()):
        j = int(r[i])
        if j >= 0 and (j not in up or a[i] > a[up[j]]):
            up[j] = int(i)
    idx = path[0]
    while idx in up and up[idx] not in seen:
        idx = up[idx]; seen.add(idx); path.insert(0, idx)
    ys, xs = np.unravel_index(np.array(path), z.shape)
    return np.stack([ys, xs], 1).astype(float)


def resample(pl, cell, step_m):
    d = np.r_[0.0, np.cumsum(np.hypot(*np.diff(pl, axis=0).T)) * cell]
    if d[-1] < step_m * 3:
        return None
    s = np.arange(0, d[-1], step_m)
    return np.stack([np.interp(s, d, pl[:, 0]), np.interp(s, d, pl[:, 1])], 1), s


# ---------------------------------------------------------------- transects

def transects(z, cell, axis_pl):
    """z sampled on perpendicular transects. Returns (n_s, n_d) and offsets."""
    sm = ndimage.gaussian_filter1d(axis_pl, AXIS_SIGMA_M / STEP_M, axis=0, mode="nearest")
    t = np.gradient(sm, axis=0)
    t /= np.maximum(np.hypot(t[:, 0], t[:, 1]), 1e-9)[:, None]
    nrm = np.stack([-t[:, 1], t[:, 0]], 1)              # unit normal, cells
    d = np.arange(-HALF_M, HALF_M + DS_M, DS_M)
    ys = sm[:, 0][:, None] + nrm[:, 0][:, None] * d[None, :] / cell
    xs = sm[:, 1][:, None] + nrm[:, 1][:, None] * d[None, :] / cell
    ok = (ys >= 0) & (ys <= z.shape[0] - 1) & (xs >= 0) & (xs <= z.shape[1] - 1)
    v = ndimage.map_coordinates(z, [np.clip(ys, 0, z.shape[0] - 1),
                                    np.clip(xs, 0, z.shape[1] - 1)], order=1)
    return np.where(ok, v, np.nan), d


ABS_M = (5.0, 15.0, 30.0)


def halfwidths(prof, d, fracs):
    """Per-transect half-width at each height fraction, both sides averaged.

    Also returns half-widths at ABSOLUTE heights above the channel, which is
    what the eye actually reads as "the trench" -- a fraction of a very deep
    valley can sit 20 m up the wall and land far out on the apron.
    """
    far = (np.abs(d) >= FAR0) & (np.abs(d) <= FAR1)
    out = {f: [] for f in fracs}
    absw = {a: [] for a in ABS_M}
    depth, wall = [], []
    for row in prof:
        if np.isnan(row[far]).mean() > 0.6:
            continue
        up = np.nanmedian(row[far])
        c = np.nanmin(row[np.abs(d) <= 60.0])
        rng = up - c
        if not np.isfinite(rng) or rng < 4.0:
            continue
        depth.append(rng)
        g = np.abs(np.gradient(row, DS_M)) * 100.0
        wall.append(np.nanpercentile(g[np.abs(d) <= 400.0], 90))
        def reach(lvl):
            w = []
            for sgn in (-1, 1):
                m = (d * sgn) > 0
                dd, vv = np.abs(d[m]), row[m]
                o = np.argsort(dd); dd, vv = dd[o], vv[o]
                hit = np.flatnonzero(np.isfinite(vv) & (vv >= lvl))
                w.append(dd[hit[0]] if hit.size else np.nan)
            return np.nanmean(w)
        for f in fracs:
            out[f].append(reach(c + f * rng))
        for a in ABS_M:
            absw[a].append(reach(c + a) if rng > a else np.nan)
    return ({f: np.array(v) for f, v in out.items()},
            {a: np.array(v) for a, v in absw.items()},
            np.array(depth), np.array(wall))


def crenulation(hw, step_m):
    """Sawtooth amplitude and pitch of the rim, trend removed."""
    v = hw[np.isfinite(hw)]
    if v.size < 16:
        return float("nan"), float("nan"), float("nan")
    tr = ndimage.gaussian_filter1d(v, DETREND_SIGMA_M / step_m, mode="nearest")
    r = v - tr
    amp = float(np.std(r))
    idx = amp / max(np.mean(v), 1e-6)
    f = np.fft.rfftfreq(r.size, step_m)
    p = np.abs(np.fft.rfft(r - r.mean())) ** 2
    band = (f > 1.0 / 900.0) & (f < 1.0 / 60.0)
    pitch = 1.0 / f[band][int(np.argmax(p[band]))] if band.any() else float("nan")
    return idx, pitch, amp


# ---------------------------------------------------------------- measure

FRACS = (0.25, 0.50, 0.90)


def measure(z, cell, name=""):
    pl = trunk_polyline(z, cell)
    r = resample(pl, cell, STEP_M) if pl is not None else None
    if r is None:
        return None
    axis, s = r
    prof, d = transects(z, cell, axis)
    hw, aw, depth, wall = halfwidths(prof, d, FRACS)
    if depth.size < 8:
        return None
    cren, pitch, cren_m = crenulation(hw[0.50], STEP_M)
    cren5, pitch5, cren5_m = crenulation(aw[15.0], STEP_M)
    ys, xs = axis[:, 0], axis[:, 1]
    chord = np.hypot(ys[0] - ys[-1], xs[0] - xs[-1]) * cell
    return dict(
        name=name,
        relief=float(np.percentile(z, 98) - np.percentile(z, 2)),
        trunk_km=float(s[-1] / 1000.0),
        sinuosity=float(s[-1] / max(chord, 1e-6)),
        depth_m=float(np.median(depth)),
        gorge_hw=float(np.nanmedian(hw[0.25])),
        mid_hw=float(np.nanmedian(hw[0.50])),
        apron_hw=float(np.nanmedian(hw[0.90])),
        wall_p90=float(np.median(wall)),
        cren=cren,
        cren_m=cren_m,
        pitch_m=pitch,
        w5=float(np.nanmedian(aw[5.0]) * 2.0),
        w15=float(np.nanmedian(aw[15.0]) * 2.0),
        w30=float(np.nanmedian(aw[30.0]) * 2.0),
        rim_cren_m=cren5_m,
        rim_pitch_m=pitch5,
    )


KEYS = ["relief", "depth_m", "w5", "w15", "w30", "apron_hw", "wall_p90",
        "rim_cren_m", "rim_pitch_m", "cren_m", "sinuosity", "trunk_km"]
HDR = {"relief": "relief", "depth_m": "depth", "w5": "w@+5m", "w15": "w@+15m",
       "w30": "w@+30m", "apron_hw": "apron hw", "wall_p90": "wall p90",
       "rim_cren_m": "rim saw", "rim_pitch_m": "rim pitch",
       "cren_m": "mid saw", "sinuosity": "sinu", "trunk_km": "trunk km"}


def table(rows, title):
    rows = [r for r in rows if r]
    print(f"\n{title}")
    print(f"{'tile':>16s} " + " ".join(f"{HDR[k]:>9s}" for k in KEYS))
    for r in rows:
        print(f"{r['name']:>16s} " + " ".join(f"{r[k]:9.2f}" for k in KEYS))
    if len(rows) > 1:
        m = {k: np.nanmedian([r[k] for r in rows]) for k in KEYS}
        print(f"{'MEDIAN':>16s} " + " ".join(f"{m[k]:9.2f}" for k in KEYS))
    return rows


def load_real():
    out = []
    for t in DISMAL:
        z, (_, _, c) = cgrid.read_f32(REAL_DIR / f"{t}.cgrid")
        out.append(measure(z, c, t))
    return out


def load_gen(d):
    out = []
    for p in sorted(pathlib.Path(d).glob("*.cgrid")):
        if ".water." in p.name:
            continue
        z, (_, _, c) = cgrid.read_f32(p)
        out.append(measure(z, c, p.stem[:16]))
    return out


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "real"
    if mode == "real":
        table(load_real(), "REAL -- Dismal River reach, Bessey NF")
    elif mode == "gen":
        table(load_gen(sys.argv[2]), f"GENERATED -- {sys.argv[2]}")
    elif mode == "compare":
        r = table(load_real(), "REAL -- Dismal River reach")
        g = table(load_gen(sys.argv[2]), f"GENERATED -- {sys.argv[2]}")
        rm = {k: np.nanmedian([x[k] for x in r]) for k in KEYS}
        gm = {k: np.nanmedian([x[k] for x in g]) for k in KEYS}
        print(f"\n{'':>16s} " + " ".join(f"{HDR[k]:>9s}" for k in KEYS))
        print(f"{'real med':>16s} " + " ".join(f"{rm[k]:9.2f}" for k in KEYS))
        print(f"{'gen med':>16s} " + " ".join(f"{gm[k]:9.2f}" for k in KEYS))
        print(f"{'gen/real':>16s} " + " ".join(
            f"{gm[k] / rm[k]:9.2f}" if rm[k] else f"{'--':>9s}" for k in KEYS))
    return 0


if __name__ == "__main__":
    sys.exit(main())
