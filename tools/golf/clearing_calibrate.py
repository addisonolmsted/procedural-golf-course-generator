"""Tune the two clearing dials against the real corpus, fit and holdout.

  python3 tools/golf/clearing_calibrate.py out/canopy250

Only two things are searched. The keep profile and the per-hole retention
distribution are measured and exact by construction, and putting either in the
search would only let it launder a structure error into a rate error.

  H_CLEAR     how the retention field clumps; higher = fewer, bigger survivors
  SIDE_SIGMA  how hard one flank of a hole thins relative to the other

What they have to buy is the shape of what survives: 2.53 patches per hole with
a specific size spread, the heavier flank carrying 82 % of the corridor's trees,
a third of holes with every survivor on one side, and a fifth with none at all.

Fit on half our fluvial seeds and choose on the other half. That split is not
ceremony -- the canopy round's own 16-tile search handed back a cell scoring
0.138 on its own tiles and 0.248 on tiles it had not seen.
"""
from __future__ import annotations

import argparse
import itertools
import json
import pathlib
import sys
import time

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
import canopy_clear as C                                           # noqa: E402
import clearing_profile as CP                                      # noqa: E402
import clearing_model as model                                     # noqa: E402

GRID = dict(H_CLEAR=[-0.5, 0.0, 0.3, 0.5, 0.7, 1.0, 1.3],
            SIDE_SIGMA=[0.0, 0.25, 0.5, 0.75, 1.0, 1.5])
# statistic -> epsilon, so a hole with nothing in its corridor cannot dominate
STATS = {"per_hole": 0.05, "1 cell": 0.05, "2 cells": 0.05, "3-5": 0.05,
         "6-20": 0.05, "21+": 0.05,
         "heavier": 0.01, "all_one": 0.01, "no_tree": 0.01}


def cohort(recs, n, hold=False):
    ss = [s for s in sorted(recs) if recs[s]["mode"] == "fluvial"]
    half = ss[1::2] if hold else ss[0::2]
    return half[:n]


def measure(ctxs, recs, params, ret):
    holes, unts = [], []
    for ctx in ctxs:
        out, _cl, _kept = C.solve(ctx, params, ret)
        lines = [np.asarray(h["spine"], float) for h in recs[ctx["seed"]]["holes"]]
        r = CP.course_rows(out == C.CLASS_TREE, lines, ctx["pars"])
        if r is None:
            continue
        holes.extend(r["holes"])
        unts.append(r["unt"])
    return stats_of(holes)


def stats_of(holes):
    import scipy.ndimage as ndi                                     # noqa: F401
    n = max(len(holes), 1)
    allp = np.concatenate([np.asarray(h["patch_sizes"], int) for h in holes
                           if h["patch_sizes"]] or [np.array([], int)])
    out = {"per_hole": len(allp) / n}
    for lo, hi, lab in ((1, 1, "1 cell"), (2, 2, "2 cells"), (3, 5, "3-5"),
                        (6, 20, "6-20"), (21, 10 ** 9, "21+")):
        out[lab] = float(((allp >= lo) & (allp <= hi)).sum() / n)
    S = np.array([h["side"] for h in holes], float)
    tot = S.sum(1)
    ok = tot > 0
    out["heavier"] = float((np.maximum(S[ok, 0], S[ok, 1]) / tot[ok]).mean()) if ok.any() else 0.0
    out["all_one"] = float(np.mean(np.minimum(S[ok, 0], S[ok, 1]) == 0)) if ok.any() else 0.0
    out["no_tree"] = float(np.mean(~ok))
    return out


def err(got, target):
    per = {k: float(abs(np.log((got[k] + e) / (target[k] + e))))
           for k, e in STATS.items()}
    return float(np.mean(list(per.values()))), per


def targets():
    t = json.loads((HERE / "corpus" / "out" / "clearing_profile.json").read_text())
    out = dict(t["patch"])
    out.update(t["side"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("natural")
    ap.add_argument("--n", type=int, default=40)
    ap.add_argument("--routes", default="out/route_rs/rs.jsonl")
    a = ap.parse_args()
    nat = pathlib.Path(a.natural)
    recs = C.read_routes(a.routes)
    T = targets()
    print("corpus targets: " + "  ".join(f"{k} {T[k]:.2f}" for k in STATS))
    res = {}
    for tag, hold in (("fit", False), ("hold", True)):
        seeds = cohort(recs, a.n, hold)
        t0 = time.time()
        ctxs = [C.prepare(s, recs[s], nat) for s in seeds]
        print(f"\n{tag}: {len(ctxs)} seeds prepared in {time.time() - t0:.0f} s", flush=True)
        # the retention draw depends only on TEE_OPEN_EXP, which is not searched
        ret = C.retention_draw(recs, dict(model.PARAMS))
        rows = []
        for H, sg in itertools.product(GRID["H_CLEAR"], GRID["SIDE_SIGMA"]):
            p = dict(model.PARAMS, H_CLEAR=H, SIDE_SIGMA=sg)
            e, per = err(measure(ctxs, recs, p, ret), T)
            rows.append(dict(H_CLEAR=H, SIDE_SIGMA=sg, err=e, per=per))
        rows.sort(key=lambda r: r["err"])
        res[tag] = rows
        for r in rows[:6]:
            print(f"  err {r['err']:.3f}  H {r['H_CLEAR']:+.2f}  sigma {r['SIDE_SIGMA']:.2f}")
    byfit = {(r["H_CLEAR"], r["SIDE_SIGMA"]): r["err"] for r in res["fit"]}
    print("\ntop fit cells re-scored on the holdout:")
    best = None
    for r in res["fit"][:6]:
        h = next(x for x in res["hold"]
                 if x["H_CLEAR"] == r["H_CLEAR"] and x["SIDE_SIGMA"] == r["SIDE_SIGMA"])
        print(f"  fit {r['err']:.3f} -> hold {h['err']:.3f}   H {r['H_CLEAR']:+.2f} "
              f"sigma {r['SIDE_SIGMA']:.2f}")
        if best is None or h["err"] < best[1]:
            best = (r, h["err"], h)
    r, he, h = best
    print(f"\n--> H_CLEAR={r['H_CLEAR']}  SIDE_SIGMA={r['SIDE_SIGMA']}   "
          f"fit {byfit[(r['H_CLEAR'], r['SIDE_SIGMA'])]:.3f}  hold {he:.3f}")
    print("    holdout residual: " + "  ".join(f"{k}={v:.2f}" for k, v in h["per"].items()))
    out = pathlib.Path("out/clearing_calib.json")
    out.write_text(json.dumps({k: [{kk: vv for kk, vv in r.items()} for r in v[:10]]
                               for k, v in res.items()}, indent=1))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
