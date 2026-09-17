"""Tune the canopy field's structure against the real tiles, coverage by coverage.

  python3 tools/golf/canopy_calibrate.py out/final250_v2
  python3 tools/golf/canopy_calibrate.py out/final250_v2 --arch sandhills --stage 1

Coverage is not fitted here -- `canopy_gen.coverage_for` already reproduces the
real per-tile distribution exactly. What is fitted is STRUCTURE: given that much
tree cover, does it come as one wood and a scatter of specks, the way the real
land does, or as a uniform spray of same-sized blobs?

Every patch statistic moves hard with coverage. On one aeolian seed the
isolated-tree rate runs 0.22, 0.72, 5.06, 10.8 per square kilometre as cover
goes 0.05, 0.5, 5, 25 percent. Scoring a cohort against the archetype's pooled
median would therefore let a cohort with the wrong coverages score well. So
each of our calibration seeds is given ONE REAL TILE'S coverage, and the
cohort is scored against the matched real cohort. The confound cancels and what
is left is the structure alone.

What is compared is the cohort MEDIAN of each statistic, not each tile against
its own partner. Real tiles at the same coverage disagree with each other
enormously -- two 80 percent-treed sandhills_nc tiles carry p90 patch sizes of
15.6 and 154 cells -- so demanding tile-by-tile agreement would be scoring us
on noise nobody can reproduce, and it would punish a correct generator. The
median is also exactly what the acceptance table compares, so the search
optimises the number it is judged on.

Every real tile of the archetype is a pair, and half our seeds of that mode are
held out of the search entirely so the reported error is honest: a 16-tile
search picked parameters that scored 0.138 on its own tiles and 0.248 on tiles
it had not seen, while the runner-up scored 0.165 and 0.168. That gap is the
whole reason this is structured as fit and holdout rather than one number.

Writes `<dump>/../canopy_calib.json`; prints the block for
`canopy_model.PARAMS`.
"""
from __future__ import annotations

import argparse
import itertools
import json
import pathlib
import sys
import time

import numpy as np
import scipy.ndimage as ndi

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
import canopy_metrics as cm                                        # noqa: E402
import canopy_gen as g                                             # noqa: E402
import canopy_model as model                                       # noqa: E402

KM2 = (300 * model.CELL / 1000.0) ** 2        # 9.00, same as a real tile

# statistic -> (epsilon, how to read it off a stats dict)
# the epsilon stops a treeless or single-patch tile from dominating a log ratio
STATS = {
    "patches_km2": (0.05, lambda s, km2: s["n_patches"] / km2),
    "p50_cells":   (0.50, lambda s, km2: s["p50_patch_cells"]),
    "p90_cells":   (0.50, lambda s, km2: s["p90_patch_cells"]),
    "lone_km2":    (0.05, lambda s, km2: s["isolated_per_km2"]),
    "largest":     (0.01, lambda s, km2: s["largest_frac"]),
    "edge":        (0.01, lambda s, km2: s["edge_density"]),
}

GRID1 = dict(a=[0.0, 0.15, 0.25, 0.35, 0.45, 0.55, 0.70],
             H=[0.3, 0.5, 0.7, 1.0, 1.3, 1.6, 2.0, 2.5],
             sm=[0.0, 20.0, 30.0, 50.0, 80.0],
             grain=[0.0, 0.15, 0.30])


def real_pairs(arch: str):
    """Every real tile of the archetype, coverage-sorted."""
    return sorted(model.TILES[arch], key=lambda r: r["tree_frac"])


def our_seeds(dump: pathlib.Path, mode: str, n: int, hold=False, wet=None):
    """`n` of our seeds of this mode: alternating halves, fit and holdout.

    Taking the even-indexed seeds for the search and the odd for the check
    keeps both halves spread across the whole seed order rather than splitting
    it at a point, which would confound the split with whatever seed order
    encodes.
    """
    idx = g.read_index(dump)
    ss = [s for s in sorted(idx) if idx[s][0] == mode
          and (wet is None or idx[s][2] == wet)]
    half = ss[1::2] if hold else ss[0::2]
    return [half[int(round(i * (len(half) - 1) / (n - 1)))] for i in range(n)]


def cache_seed(seed, dump, place_key, sms):
    """Everything independent of the fractal weights: the filtered model, the octaves.

    The seven octave fields do not depend on H -- only their weights do -- so
    they are built once per seed and every (H, grain) is a reweighting. They are
    kept rather than the 24 combined fields because a full-cohort search over 65
    seeds would otherwise need most of a gigabyte. Holding the noise realisation
    fixed across the whole search makes the comparison paired and the surface
    smooth.
    """
    z2, c2, cls, wet10, _ = g.tile_inputs(seed, dump)
    pr, n = cm.predictors(z2, c2, cls)
    raw = g.logit_field(pr, n, model.FITS[place_key])
    T = {sm: g.zscore(ndi.gaussian_filter(raw, sm / model.CELL) if sm > 0
                      else raw).astype(np.float32) for sm in sms}
    octs = [o.astype(np.float32) for o in g.octave_fields(seed, n)]
    return dict(T=T, octs=octs, wet=wet10[:n, :n], n=n)


def fbm(octs, H, grain):
    w = np.array([L ** H for L in model.OCTAVES_M], float)
    w[0] *= (1.0 + grain * 10.0)
    return g.zscore(sum(wi * f for wi, f in zip(w, octs)))


def cohort_err(gen, pairs):
    """Bias of the cohort median, per statistic, against the matched real cohort."""
    per = {}
    for k, (eps, rd) in STATS.items():
        gv = np.median([rd(s, KM2) for s in gen])
        rv = np.median([rd(r, (r["n"] * model.CELL / 1000.0) ** 2) for r in pairs])
        per[k] = float(abs(np.log((gv + eps) / (rv + eps))))
    return float(np.mean(list(per.values()))), per


def evaluate(cache, pairs, a, H, sm, grain, fcache=None):
    gen = []
    for i, (cs, row) in enumerate(zip(cache, pairs)):
        F = fcache[i] if fcache is not None else fbm(cs["octs"], H, grain)
        s = a * cs["T"][sm].astype(np.float64) + np.sqrt(max(1.0 - a * a, 0.0)) * F
        gen.append(cm.patch_stats(g.threshold(s, cs["wet"], row["tree_frac"])))
    return cohort_err(gen, pairs)


def search(arch, mode, dump, grid, hold=False, cache=None, pairs=None,
           place=None, wet=None):
    """Sweep the grid with (H, grain) outermost so each fractal field is built once.

    `place` may differ from `arch`: the aeolian river seeds are PLACED by the
    sandhills fit, because the sandhills_river fit's water term has the opposite
    sign on 18 tiles and would paint a one-cell ribbon down our synthetic creek,
    but they are SCORED against the sandhills_river tiles, which is the country
    they are meant to look like.
    """
    pairs = pairs or real_pairs(arch)
    if cache is None:
        seeds = our_seeds(dump, mode, len(pairs), hold=hold, wet=wet)
        t0 = time.time()
        cache = [cache_seed(s, dump, place or arch, grid["sm"]) for s in seeds]
        print(f"  cached {len(cache)} seeds in {time.time() - t0:.0f} s", flush=True)
    t0, res = time.time(), []
    for H, gr in itertools.product(grid["H"], grid["grain"]):
        fc = [fbm(cs["octs"], H, gr) for cs in cache]
        for a, sm in itertools.product(grid["a"], grid["sm"]):
            e, per = evaluate(cache, pairs, a, H, sm, gr, fcache=fc)
            res.append(dict(a=a, H=H, sm=sm, grain=gr, err=e, per=per))
        print(f"    H {H:.2f} grain {gr:.2f}  [{time.time() - t0:.0f} s]", flush=True)
    res.sort(key=lambda r: r["err"])
    return res, cache, pairs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dump")
    ap.add_argument("--arch", default=None)
    a = ap.parse_args()
    dump = pathlib.Path(a.dump)
    todo = [("sandhills", "aeolian", None, "dry"),
            ("sandhills_nc", "fluvial", None, None),
            ("sandhills_river", "aeolian", "sandhills", "river")]
    if a.arch:
        todo = [t for t in todo if t[0] == a.arch]
    out = {}
    for arch, mode, place, wet in todo:
        print(f"\n=== {arch} ({mode}) ===", flush=True)
        res, _, pairs = search(arch, mode, dump, GRID1, place=place, wet=wet)
        print("  fit, best 8:")
        for r in res[:8]:
            print(f"    err {r['err']:.3f}  a {r['a']:.2f} H {r['H']:.2f} "
                  f"sm {r['sm']:.0f} grain {r['grain']:.2f}")
        # The holdout decides. A search this bumpy will hand back a lucky cell,
        # so the top candidates are re-scored on seeds the search never saw and
        # the winner is the one that survives that.
        cand = res[:8]
        hg = dict(a=sorted({c["a"] for c in cand}), H=sorted({c["H"] for c in cand}),
                  sm=sorted({c["sm"] for c in cand}),
                  grain=sorted({c["grain"] for c in cand}))
        hseeds = our_seeds(dump, mode, len(pairs), hold=True, wet=wet)
        hcache = [cache_seed(s, dump, place or arch, hg["sm"]) for s in hseeds]
        print("  holdout:")
        scored = []
        for c in cand:
            e, per = evaluate(hcache, pairs, c["a"], c["H"], c["sm"], c["grain"])
            scored.append(dict(c, err_hold=e, per_hold=per))
            print(f"    fit {c['err']:.3f} -> hold {e:.3f}   a {c['a']:.2f} "
                  f"H {c['H']:.2f} sm {c['sm']:.0f} grain {c['grain']:.2f}")
        win = min(scored, key=lambda r: r["err_hold"])
        out[arch] = dict(best=win, fit=[{k: r[k] for k in ("a", "H", "sm", "grain", "err")}
                                        for r in res[:8]], holdout=scored,
                         n_pairs=len(pairs))
        print(f"  --> {arch}: a={win['a']} H={win['H']} sm={win['sm']} "
              f"grain={win['grain']}  fit {win['err']:.3f} hold {win['err_hold']:.3f}")
        print("      holdout residual: " +
              "  ".join(f"{k}={v:.2f}" for k, v in win["per_hold"].items()))
    p = dump.parent / "canopy_calib.json"
    p.write_text(json.dumps(out, indent=1))
    print(f"\nwrote {p}\n\nPARAMS block:")
    for arch, r in out.items():
        b = r["best"]
        print(f'    "{arch}": dict(a={b["a"]}, H={b["H"]}, sm={b["sm"]}, '
              f'grain={b["grain"]}, r_edge=20.0, density=...),')


if __name__ == "__main__":
    main()
