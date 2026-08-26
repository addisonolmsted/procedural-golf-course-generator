#!/usr/bin/env python3
"""Does this terrain hold golf? A playability instrument.

Every gate in this project so far has been a FIDELITY gate: does the tile
look like the corpus in a top-down hillshade. That is a geomorphologist's
question, and the answer to it turns out to be a poor proxy for the thing we
actually want — terrain that routes good holes. A thalweg crease matters a
great deal to the first question and not at all to the second.

So this measures the second directly. Nothing here compares against the
corpus; it asks whether ground of a given shape can carry golf.

    green sites   a green needs ~30 m of ground under 4-5% with the local
                  relief inside about a metre, dry, and not perched on a
                  knife ridge. Slope is the binding constraint: a putting
                  surface over ~5% will not hold a ball.
    tee sites     smaller and more forgiving, under ~6%.
    holes         a PACKING count, which is the only version of this
                  question worth asking. Counting flat discs gave 283
                  "green sites" on a 3 km tile: on gentle ground almost
                  everything is flat enough for a green, so the count
                  measured area, not golf. A course is 18 holes on ~60 ha,
                  so the real question is how many NON-OVERLAPPING holes the
                  tile carries. Holes are packed greedily, each consuming
                  its own corridor, until no more fit. Straight corridors
                  only — real holes dogleg, so this under-counts, which is
                  the safe direction for a gate.
    slope bands   the histogram behind all of it, in golf's own terms
                  rather than degrees.

Run:
    python3 tools/golf/playability.py <dir> <prefix>...
"""
import sys
import pathlib
import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/macro_campaign"))
from macro_campaign import cgrid  # noqa: E402

# Slope bands, in percent. Chosen from how golf ground actually behaves, not
# from the terrain literature.
BANDS = [
    ("green-able", 0.0, 5.0),      # a ball will sit; a putting surface works
    ("fairway", 5.0, 12.0),        # comfortable stance and a running lie
    ("rough", 12.0, 25.0),         # playable but awkward; sidehill lies
    ("severe", 25.0, 1e9),         # unplayable / earthworks territory
]

GREEN_R = 16.0      # a green is ~30 m across
TEE_R = 9.0
GREEN_SLOPE = 5.0
GREEN_RELIEF = 1.4  # metres across the pad
TEE_SLOPE = 6.5
HOLE_MIN, HOLE_MAX = 230.0, 460.0
CORRIDOR_HW = 28.0  # half-width of the landing corridor
# Holes are counted inside a COURSE-SIZED window rather than the whole tile.
# An 18-hole course occupies roughly 60 ha; a 3 km tile is 900 ha, so a
# whole-tile count answers "how much land is there", which we already know.
# 1500 m square is 225 ha — comfortably more than a course needs, so the
# count is bounded by the terrain rather than by the window or by a cap.
WINDOW_M = 1500.0


def load(d, pref):
    z, (_, _, c) = cgrid.read_f32(d / f"{pref}.cgrid")
    wp = d / f"{pref}.water.cgrid"
    wet = np.zeros(z.shape, bool)
    if wp.exists():
        w, _ = cgrid.read_f32(wp)
        wet = ~np.isnan(w)
    return z.astype(float), float(c), wet


def slope_pct(z, c):
    gy, gx = np.gradient(z, c)
    return np.hypot(gx, gy) * 100.0


def pads(z, c, wet, radius, max_slope, max_relief=None):
    """Cells where a disc of `radius` is flat enough, dry, and low-relief."""
    n = max(1, int(round(radius / c)))
    k = np.ones((2 * n + 1, 2 * n + 1), bool)
    yy, xx = np.mgrid[-n:n + 1, -n:n + 1]
    k &= (xx * xx + yy * yy) <= n * n
    s = slope_pct(z, c)
    ok = s <= max_slope
    ok = ndimage.minimum_filter(ok.astype(np.uint8), footprint=k).astype(bool)
    dry = ndimage.minimum_filter((~wet).astype(np.uint8), footprint=k).astype(bool)
    ok &= dry
    if max_relief is not None:
        hi = ndimage.maximum_filter(z, footprint=k)
        lo = ndimage.minimum_filter(z, footprint=k)
        ok &= (hi - lo) <= max_relief
    return ok


def peak_sites(mask, z, c, min_sep, cap=400):
    """Representative sites, at least min_sep apart, best-first.

    Scored by how much room the pad has around it, so a broad shelf beats a
    marginal one; ties broken by position for determinism.
    """
    if not mask.any():
        return []
    room = ndimage.distance_transform_edt(mask, sampling=c)
    ys, xs = np.where(room > 0)
    order = np.argsort(-room[ys, xs])
    keep = []
    for i in order:
        y, x = ys[i] * c, xs[i] * c
        if all((y - q[0]) ** 2 + (x - q[1]) ** 2 > min_sep ** 2 for q in keep):
            keep.append((y, x))
            if len(keep) >= cap:
                break
    return keep


def corridors(greens, tees, z, c, wet, s):
    """Greedily PACK non-overlapping holes; return the ones that fit."""
    ny, nx = z.shape
    walk = (s <= 25.0) & (~wet)
    taken = np.zeros(z.shape, bool)      # ground already spent on a hole
    holes = []
    for gi, (gy, gx) in enumerate(greens):
        best = None
        for (ty, tx) in tees:
            d = np.hypot(gy - ty, gx - tx)
            if not (HOLE_MIN <= d <= HOLE_MAX):
                continue
            # sample the corridor: centre line plus two flanks
            n = int(d / 8.0)
            t = np.linspace(0, 1, n)
            cy = ty + (gy - ty) * t
            cx = tx + (gx - tx) * t
            ax = np.array([gy - ty, gx - tx]) / max(d, 1e-9)
            pr = np.array([-ax[1], ax[0]])
            good = 0
            tot = 0
            for off in (-CORRIDOR_HW, 0.0, CORRIDOR_HW):
                yy = ((cy + pr[0] * off) / c).astype(int)
                xx = ((cx + pr[1] * off) / c).astype(int)
                m = (yy >= 0) & (yy < ny) & (xx >= 0) & (xx < nx)
                if m.sum() == 0:
                    continue
                good += walk[yy[m], xx[m]].sum()
                tot += m.sum()
            if tot == 0:
                continue
            frac = good / tot
            if frac < 0.90:
                continue
            # a hole wants some elevation interest but not a cliff
            rise = abs(z[int(gy / c) % ny, int(gx / c) % nx]
                       - z[int(ty / c) % ny, int(tx / c) % nx])
            if rise > 0.09 * d:
                continue
            # the corridor must not reuse ground another hole already has
            yy = (cy / c).astype(int).clip(0, ny - 1)
            xx = (cx / c).astype(int).clip(0, nx - 1)
            if taken[yy, xx].mean() > 0.15:
                continue
            if best is None or frac > best[0]:
                best = (frac, (ty, tx), (gy, gx), d, rise, (yy, xx))
        if best is not None:
            _, _, _, _, _, (yy, xx) = best
            spend = np.zeros(z.shape, bool)
            spend[yy, xx] = True
            r = max(1, int(CORRIDOR_HW / c))
            taken |= ndimage.binary_dilation(spend, iterations=r)
            holes.append(best[:5])
    return holes


def report(d, pref):
    z, c, wet = load(d, pref)
    s = slope_pct(z, c)
    n = int(WINDOW_M / c)
    oy = (z.shape[0] - n) // 2
    ox = (z.shape[1] - n) // 2
    zc = z[oy:oy + n, ox:ox + n]
    wc = wet[oy:oy + n, ox:ox + n]
    sc = s[oy:oy + n, ox:ox + n]
    dry = ~wet
    tot = dry.sum()
    bands = {}
    for name, lo, hi in BANDS:
        bands[name] = 100.0 * ((s >= lo) & (s < hi) & dry).sum() / max(tot, 1)
    g = pads(zc, c, wc, GREEN_R, GREEN_SLOPE, GREEN_RELIEF)
    t = pads(zc, c, wc, TEE_R, TEE_SLOPE)
    greens = peak_sites(g, zc, c, 260.0, cap=90)
    tees = peak_sites(t, zc, c, 130.0, cap=160)
    holes = corridors(greens, tees, zc, c, wc, sc)
    return {
        "tile": pref, "bands": bands,
        "green_area_pct": 100.0 * g.sum() / g.size,
        "win_ha": (WINDOW_M / 100.0) ** 2,
        "green_sites": len(greens), "tee_sites": len(tees),
        "holes": len(holes), "wet_pct": 100.0 * wet.mean(),
        "relief": float(np.percentile(z, 98) - np.percentile(z, 2)),
        "_masks": (g, t), "_holes": holes, "_win": (oy, ox, n), "_c": c,
    }


def main():
    d = pathlib.Path(sys.argv[1])
    hdr = f"{'tile':22s} {'green%':>7s} {'fairway%':>9s} {'rough%':>7s} " \
          f"{'severe%':>8s} {'holes/225ha':>12s} {'relief m':>9s}"
    print(hdr)
    print("-" * len(hdr))
    for pref in sys.argv[2:]:
        r = report(d, pref)
        b = r["bands"]
        print(f"{r['tile']:22s} {b['green-able']:7.1f} {b['fairway']:9.1f} "
              f"{b['rough']:7.1f} {b['severe']:8.2f} {r['holes']:12d} "
              f"{r['relief']:9.1f}")


if __name__ == "__main__":
    main()
