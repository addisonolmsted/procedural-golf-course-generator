"""Phase 5 gate: does the fitted scorer move proposals onto real greens?

For every held-out keeper course: fit the biome-family window over its
assigned greens (18-holers at 2x area), run the FULL generate-confirm-thin
pipeline under (a) the fitted scorer (current greens.py) and (b) the previous
hand scorer (greens.py from git, imported side-by-side), and measure recall
of in-window real greens within 60 / 100 m of any proposed candidate.

Same window, same seeds, same thinning — only the score differs, so the
delta is the scorer's.
"""

from __future__ import annotations

import importlib.util
import json
import pathlib
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

import numpy as np

from . import config, fit, registry
from .features import greens_local
from .windows import best_cover

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
for _p in ("golf", "macro_campaign", "dtm_primitives"):
    if str(_TOOLS / _p) not in sys.path:
        sys.path.insert(0, str(_TOOLS / _p))
import siting                                          # noqa: E402
import greens as G_new                                 # noqa: E402
from macro_campaign import cgrid                       # noqa: E402

OLD_REF = "854d241"          # last commit before the transcription
DIMS = {"sandhills_ne": (1450.0, 950.0)}
DIMS_DEFAULT = (1000.0, 700.0)


def _load_old():
    dest = config.OUT / "_greens_old.py"
    if not dest.exists():
        src = subprocess.run(
            ["git", "show", f"{OLD_REF}:tools/golf/greens.py"],
            capture_output=True, text=True, check=True,
            cwd=_TOOLS.parent).stdout
        dest.write_text(src)
    spec = importlib.util.spec_from_file_location("greens_old", dest)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["greens_old"] = mod    # dataclass introspection needs this
    spec.loader.exec_module(mod)
    return mod


class _Sit:
    pass


class _CH:
    pass


def _persistence_window(f, i0, i1, j0, j1):
    p = _Sit()
    p.pit = np.zeros(f.z8.shape)
    p.peak = np.zeros(f.z8.shape)
    p.spill_dir = np.zeros(f.z8.shape)
    zc = f.z8[i0:i1, j0:j1]
    fill, sp = siting._flood_fill_level(zc, f.cell)
    p.pit[i0:i1, j0:j1] = fill - zc
    fill_inv, _ = siting._flood_fill_level(-zc, f.cell)
    p.peak[i0:i1, j0:j1] = fill_inv + zc
    p.spill_dir[i0:i1, j0:j1] = sp
    return p


def one_course(cid: str, keep_pools: bool = False) -> dict | None:
    rec = registry.load(cid)
    t = rec.get("tile")
    if not t or "path" not in t:
        return None
    gs = greens_local(rec)
    if len(gs) < config.MIN_GREENS_KEEP:
        return None
    long9, short9 = DIMS.get(rec["region_tag"], DIMS_DEFAULT)
    scale = np.sqrt(2.0) if rec["holes_estimate"] >= 18 else 1.0
    H, W = long9 * scale, short9 * scale
    cov, arg = 0.0, None
    for (a, b) in ((H, W), (W, H)):
        c, g = best_cover(gs, a, b)
        if c > cov:
            cov, arg = c, g
    if arg is None:
        return None
    wy, wx, HH, WW = arg
    z2, (_, _, cell2) = cgrid.read_f32(pathlib.Path(t["path"]))
    z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(np.float64)
    # clamp the window inside the tile
    ext = z2.shape[0] * cell2
    wy = float(np.clip(wy, 0, max(ext - HH, 0)))
    wx = float(np.clip(wx, 0, max(ext - WW, 0)))
    wp = pathlib.Path(t["path"]).with_suffix(".water.npy")
    wet2 = np.load(wp) if wp.exists() else np.zeros(z2.shape, bool)
    inw = ((gs[:, 0] >= wy) & (gs[:, 0] <= wy + HH)
           & (gs[:, 1] >= wx) & (gs[:, 1] <= wx + WW))
    gin = gs[inw]
    if len(gin) < 5:
        return None

    f = siting.build_fields(z2, cell2, wet2)
    m = siting.build_morphology(f)
    i0 = max(0, int(wy / f.cell) - 25)
    i1 = min(f.z8.shape[0], int((wy + HH) / f.cell) + 25)
    j0 = max(0, int(wx / f.cell) - 25)
    j1 = min(f.z8.shape[1], int((wx + WW) / f.cell) + 25)
    p = _persistence_window(f, i0, i1, j0, j1)
    sit = _Sit()
    sit.window_ij = (int(wy / f.cell), int(wx / f.cell))
    sit.window_m = (wy, wx, HH, WW)
    ch = _CH()
    ch.yx = (wy + HH / 2, wx + WW / 2)
    ch.reserved_green = ch.yx
    ch.reserved_tee = ch.yx
    ch.score = 1.0
    sit.clubhouse = ch

    G_old = _load_old()
    out = dict(course_id=cid, region=rec["region_tag"], n_in=int(len(gin)),
               holes=int(rec["holes_estimate"]), window=[wy, wx, HH, WW])
    for tag, G in (("new", G_new), ("old", G_old)):
        pool = G.generate(z2, cell2, wet2, sit, f, m, p, n_target=110)
        q = (np.array([a.yx for a in pool]) if pool else np.zeros((0, 2)))
        if len(q):
            d = np.array([np.hypot(q[:, 0] - y, q[:, 1] - x).min()
                          for (y, x) in gin])
        else:
            d = np.full(len(gin), 9e9)
        out[tag] = dict(n_pool=len(pool),
                        w60=float((d <= 60).mean() * 100),
                        w100=float((d <= 100).mean() * 100),
                        med=float(np.median(d)))
        if keep_pools:
            out[f"pool_{tag}"] = [list(a.yx) for a in pool]
        # standing dispersion gate (P7): the pool must make every part of the
        # window REACHABLE. Measured 2026-08-29 on 14 held-out courses the
        # new-scorer pool already exceeds the real-greens targets (hull 0.86
        # vs real p25 0.30, sectors 8/8) -- the planned sector-quota thinning
        # was NOT built because the problem it solved no longer measures.
        # These numbers keep it that way.
        if tag == "new" and len(q):
            from .dispersion import per_course as _disp
            inw = q[(q[:, 0] >= wy) & (q[:, 0] <= wy + HH)
                    & (q[:, 1] >= wx) & (q[:, 1] <= wx + WW)]
            if len(inw) >= 10:
                out["pool_dispersion"] = _disp(inw, HH * WW)
    return out


def run(procs: int = 4) -> dict:
    rows_all = fit.load_rows()
    _, held = fit.split_courses(rows_all)
    cids = sorted(c for c in held
                  if registry.path(c).exists()
                  and registry.load(c)["status"] == "keeper")
    _load_old()          # materialize before forking
    res = []
    with ProcessPoolExecutor(procs) as ex:
        for r in ex.map(one_course, cids, chunksize=1):
            if r:
                res.append(r)
                print(f"  {r['course_id'][:38]:40s} "
                      f"new {r['new']['w100']:3.0f}%  old {r['old']['w100']:3.0f}%"
                      f"  (n={r['n_in']})", flush=True)
    agg = {}
    for tag in ("new", "old"):
        agg[tag] = dict(
            w60=float(np.median([r[tag]["w60"] for r in res])),
            w100=float(np.median([r[tag]["w100"] for r in res])),
            med=float(np.median([r[tag]["med"] for r in res])),
            pool=float(np.median([r[tag]["n_pool"] for r in res])))
    disp = [r["pool_dispersion"] for r in res if "pool_dispersion" in r]
    if disp:
        agg["pool_dispersion"] = {
            k: float(np.median([d[k] for d in disp]))
            for k in ("hull_frac", "sectors", "nn_p10")}
        # real-greens targets from corpus/out/dispersion.json (2026-08-29):
        # hull_frac p25 0.299, sectors med 7, nn_p10 p25 53 m
        agg["dispersion_gate"] = bool(
            agg["pool_dispersion"]["hull_frac"] >= 0.30
            and agg["pool_dispersion"]["sectors"] >= 6)
    out = dict(courses=res, agg=agg, n=len(res))
    (config.OUT / "recall_eval.json").write_text(json.dumps(out, indent=1))
    return out


if __name__ == "__main__":
    r = run(int(sys.argv[1]) if len(sys.argv) > 1 else 4)
    print(json.dumps(r["agg"], indent=1))
