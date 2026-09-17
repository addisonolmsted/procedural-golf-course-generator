"""How much tree a real golf hole leaves standing, and where.

  python3 tools/golf/clearing_profile.py [--limit 400]

Round 1 measured the cleared corridor's WIDTH. This measures what survives
inside it, which is what the clearing round has to reproduce, because a corridor
mown to bare ground is not what a course looks like.

**Everything is measured against genuinely untouched ground** -- the tree rate
more than 500 m from every hole on the tile -- and in the NEAREST-HOLE frame:
each cell is assigned to the hole whose centre line is closest, and binned by
that distance. Both choices are corrections to the obvious method, and both came
out of a measurement that went wrong first.

The obvious method normalises each hole by its own tree rate 120-160 m out along
the normals. On a real course that band is another fairway: it holds only 0.64
of the untouched rate, so every ratio computed that way is inflated by about
1.6x. Our own routes are sparser and theirs is inflated 1.25x, so the two
denominators are not even wrong in the same proportion. The nearest-hole frame
has no such band -- a cell belongs to its closest hole and nothing else -- and
the far reference is real untouched land.

What that frame shows is clean and slightly surprising: clearing recovers FULLY
by 80 m from the nearest hole line, and there is no property-wide thinning at
all. Between 100 m and 500 m the tree rate sits within 3 % of untouched. A golf
course is a set of corridors, not an opened-up estate.

**Clearing is proportional thinning.** Binning courses by their own cover, the
absolute in-corridor rate varies 2.4x while the normalised profile barely moves,
so the keep rule is a fraction of whatever is there and no local rate has to be
estimated.

**Par 3s are a different animal.** The middle of a par 3 is carry, not a landing
zone, and courses leave several times more tree there. Every table is by par.

Writes `corpus/out/clearing_profile.json`.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time

import numpy as np
import scipy.ndimage as ndi

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                   # noqa: E402
from corpus.shot_profiles import joined                            # noqa: E402
from corridor_width import tile_paths                              # noqa: E402
import clearing_geom as G                                          # noqa: E402

OUT = HERE / "corpus" / "out" / "clearing_profile.json"
CLASS_TREE = 10
MIN_TREED_PCT = 25.0
UNTOUCHED_M = 500.0         # beyond this, no hole has touched the ground
UNTOUCHED_MIN_CELLS = 2000
UNTOUCHED_MIN_RATE = 0.15
DIST = np.arange(0.0, 85.0, 5.0)     # shape support; 1.0 beyond
ZONES = ("tee", "landing", "green")
ST = np.ones((3, 3), int)


def nearest_frame(lines, shape, cell=G.CELL):
    """Distance, hole index and arc fraction of the closest hole, per cell."""
    ys, xs = G.cell_centres(shape, cell)
    d = np.full(shape, np.inf)
    who = np.full(shape, -1, np.int16)
    frac = np.zeros(shape)
    for i, p in enumerate(lines):
        dd, _side, ff = G.project_polyline(p, ys, xs)
        take = dd < d
        d = np.where(take, dd, d)
        frac = np.where(take, ff, frac)
        who = np.where(take, i, who)
    return d, who, frac


def course_rows(tree, lines, pars):
    """One tile's contribution: shape bins, per-hole retention, patches, sides."""
    n = tree.shape[0]
    d, who, frac = nearest_frame(lines, tree.shape)
    far_m = d > UNTOUCHED_M
    if far_m.sum() < UNTOUCHED_MIN_CELLS:
        return None
    unt = float(tree[far_m].mean())
    if unt < UNTOUCHED_MIN_RATE:
        return None
    seg = G.seg_index(frac)
    shape_num, shape_den = {}, {}
    for i, par in enumerate(pars):
        key = int(par) if par else 0
        mine = who == i
        for zi, z in enumerate(ZONES):
            zs = mine & (seg == zi)
            for j in range(len(DIST) - 1):
                b = zs & (d >= DIST[j]) & (d < DIST[j + 1])
                if not b.any():
                    continue
                k = (key, z, j)
                shape_num[k] = shape_num.get(k, 0.0) + float(tree[b].sum())
                shape_den[k] = shape_den.get(k, 0.0) + float(b.sum())
    holes = []
    for i, (p, par) in enumerate(zip(lines, pars)):
        if float(G.arc_cum(p)[-1]) < 180.0:
            continue
        zm = G.corridor_zone_masks(p, tree.shape)
        row = dict(par=int(par) if par else 0, unt=unt)
        for z in ZONES:
            m = zm[z]
            row[f"ret_{z}"] = float(tree[m].mean() / unt) if m.sum() else None
        allm = zm["tee"] | zm["landing"] | zm["green"]
        inc = tree & allm
        lab, nl = ndi.label(inc, structure=ST)
        row["patch_sizes"] = (np.bincount(lab.ravel())[1:].tolist() if nl else [])
        ys, xs = np.where(inc)
        if len(ys):
            _dd, side, _ff = G.project_polyline(p, ys * G.CELL + 5.0, xs * G.CELL + 5.0)
            row["side"] = [int((side > 0).sum()), int((side < 0).sum())]
        else:
            row["side"] = [0, 0]
        holes.append(row)
    return dict(unt=unt, num=shape_num, den=shape_den, holes=holes)


def run(limit=None):
    paths, cache = tile_paths(), {}
    num, den, holes = {}, {}, []
    unts, cur, buf = [], None, []
    t0 = time.time()

    def flush(cid, buf):
        if len(buf) < 5 or cache.get(cid) is None:
            return
        r = course_rows(cache[cid] == CLASS_TREE, [b[0] for b in buf],
                        [b[1] for b in buf])
        if r is None:
            return
        for k, v in r["num"].items():
            num[k] = num.get(k, 0.0) + v
            den[k] = den.get(k, 0.0) + r["den"][k]
        holes.extend(r["holes"])
        unts.append(r["unt"])

    for w in joined():
        cid = w["course_id"]
        if cid != cur:
            flush(cur, buf)
            cur, buf = cid, []
            if limit and len(holes) >= limit:
                break
        if cid not in cache:
            base = paths.get(cid)
            cache[cid] = None
            if base is not None:
                wc = base.with_suffix(".canopy.cgrid")
                if wc.exists():
                    cls, _ = cgrid.read_u8(wc)
                    if 100.0 * float((cls == CLASS_TREE).mean()) >= MIN_TREED_PCT:
                        cache[cid] = cls
        if cache[cid] is None:
            continue
        buf.append((np.asarray(w["pts_loc"], float), w["par"]))
    flush(cur, buf)
    print(f"{len(holes)} holes on {len(unts)} wooded courses, untouched reference "
          f"median {100 * np.median(unts):.1f} %  [{time.time() - t0:.0f} s]")
    return num, den, holes, unts


def _shape(num, den, pars, zone, min_cells=400):
    out = []
    for j in range(len(DIST) - 1):
        a = sum(num.get((p, zone, j), 0.0) for p in pars)
        b = sum(den.get((p, zone, j), 0.0) for p in pars)
        out.append(a / b if b >= min_cells else None)
    return out


def summarize(num, den, holes, unts):
    out = {"n_holes": len(holes), "n_courses": len(unts),
           "untouched": float(np.median(unts)), "dist": DIST.tolist(),
           "shape": {}, "retention": {}, "patch": {}, "side": {}}
    unt = out["untouched"]
    print("\nKEEP RATE against distance to the nearest hole line, "
          "over untouched ground")
    print(f"{'par':>4s} {'zone':8s}" + "".join(f"{f'{DIST[j]:.0f}':>8s}"
                                               for j in range(0, 16, 2)))
    for par, pars in ((3, (3,)), (4, (4,)), (5, (5,)), ("all", (0, 3, 4, 5))):
        out["shape"][str(par)] = {}
        for z in ZONES:
            c = _shape(num, den, pars, z)
            v = [x / unt if x is not None else None for x in c]
            # fill gaps and clamp: never plant a tree, and hold at 1 once recovered
            filled, last = [], None
            for x in v:
                last = x if x is not None else last
                filled.append(min(last if last is not None else 1.0, 1.0))
            out["shape"][str(par)][z] = filled
            print(f"{str(par):>4s} {z:8s}" + "".join(f"{filled[j]:8.3f}"
                                                     for j in range(0, 16, 2)))
    print("\nPER-HOLE RETENTION inside the tapered corridor "
          "(share of untouched cover)")
    print(f"{'par':>4s} {'zone':8s}{'mean':>8s}{'p25':>8s}{'p50':>8s}{'p75':>8s}"
          f"{'zero':>7s}{'tight':>7s}{'n':>6s}")
    for par, sel in ((3, [h for h in holes if h["par"] == 3]),
                     (4, [h for h in holes if h["par"] == 4]),
                     (5, [h for h in holes if h["par"] == 5]),
                     ("all", holes)):
        out["retention"][str(par)] = {}
        for z in ZONES:
            v = np.array([h[f"ret_{z}"] for h in sel if h.get(f"ret_{z}") is not None])
            if len(v) < 30:
                continue
            out["retention"][str(par)][z] = sorted(float(x) for x in v)
            print(f"{str(par):>4s} {z:8s}{v.mean():8.3f}"
                  + "".join(f"{np.percentile(v, q):8.3f}" for q in (25, 50, 75))
                  + f"{100 * np.mean(v < 0.02):6.0f}%{100 * np.mean(v > 0.6):6.0f}%{len(v):6d}")
    allp = np.concatenate([np.asarray(h["patch_sizes"], int) for h in holes
                           if h["patch_sizes"]] or [np.array([], int)])
    n = len(holes)
    out["patch"] = {"per_hole": float(len(allp) / n)}
    print(f"\nPATCHES inside the corridor: {len(allp) / n:.2f} per hole")
    for lo, hi, lab in ((1, 1, "1 cell"), (2, 2, "2 cells"), (3, 5, "3-5"),
                        (6, 20, "6-20"), (21, 10 ** 9, "21+")):
        out["patch"][lab] = float(((allp >= lo) & (allp <= hi)).sum() / n)
        print(f"  {lab:10s}{out['patch'][lab]:7.2f} per hole")
    # how a hole's three zones go together. Perfectly correlated would leave
    # 39 % of holes with no corridor tree anywhere and independent 7 %; the
    # truth is 22 %, so the dependence has to be carried, not assumed either way
    from scipy.stats import rankdata
    trip = np.array([[h[f"ret_{z}"] for z in ZONES] for h in holes
                     if all(h.get(f"ret_{z}") is not None for z in ZONES)])
    if len(trip) > 100:
        R = np.array([rankdata(trip[:, i]) for i in range(3)])
        out["zone_corr"] = np.corrcoef(R).tolist()
        out["all_zones_bare"] = float((trip < 0.02).all(1).mean())
        print("\nZONE DEPENDENCE (Spearman): " + "  ".join(
            f"{ZONES[i]}-{ZONES[j]} {np.corrcoef(R)[i, j]:.2f}"
            for i, j in ((0, 1), (1, 2), (0, 2)))
            + f"   all three bare {100 * out['all_zones_bare']:.0f} %")
    S = np.array([h["side"] for h in holes], float)
    tot = S.sum(1)
    ok = tot > 0
    out["side"] = {"heavier": float((np.maximum(S[ok, 0], S[ok, 1]) / tot[ok]).mean()),
                   "all_one": float(np.mean(np.minimum(S[ok, 0], S[ok, 1]) == 0)),
                   "no_tree": float(np.mean(~ok))}
    print(f"\nASYMMETRY: heavier side {100 * out['side']['heavier']:.0f} % ; "
          f"all on one side {100 * out['side']['all_one']:.0f} % ; "
          f"no corridor tree {100 * out['side']['no_tree']:.0f} %")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int)
    a = ap.parse_args()
    num, den, holes, unts = run(a.limit)
    s = summarize(num, den, holes, unts)
    OUT.write_text(json.dumps(s, indent=1))
    print(f"\nwrote {OUT}")


if __name__ == "__main__":
    main()
