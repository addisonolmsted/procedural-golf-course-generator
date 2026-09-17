"""Grow the trees on our own tiles.

  python3 tools/golf/canopy_gen.py out/final250_v2 out/canopy250
  python3 tools/golf/canopy_gen.py out/final250_v2 out/canopy250 --seeds 900000,900014

Round 1 measured where trees grow on natural land and fitted a logistic model
per archetype. This applies that model to the 250 frozen seeds and writes a
canopy sidecar beside nothing: the output goes to its OWN directory, because
the terrain is frozen and every routing metric from four rounds is measured
against those exact files.

The model alone cannot do the job. Its Brier skill is 0.09-0.13, so it says
where trees are LIKELIER, not where a wood ends. Thresholding it gives smooth
blobs of one size, and the real tiles are emphatically not that: a sandhills
tile carries one dominant stand holding half the treed area plus a long tail of
specks. So the score is the model mixed with a fractal noise field, and the
threshold is a quantile chosen to hit a per-seed coverage drawn from the real
per-tile distribution. Coverage therefore matches by construction and the
free parameters only have to buy the STRUCTURE.

Two rasters out, mirroring what the corpus holds for real ground:
`.canopy.cgrid` in WorldCover class codes, so `corridor_width.py` reads it with
no change at all, and `.tcc.cgrid` in percent canopy, so the savanna signature
-- treed land against canopy density -- survives.
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
import canopy_metrics as cm                                        # noqa: E402
import canopy_model as model                                       # noqa: E402

CELL = model.CELL
CLASS_TREE, CLASS_WATER, CLASS_OTHER = 10, 80, 30
# A creek is 3-4 m wide on a 2 m grid, so it never fills a 10 m cell and a
# majority vote inside the 5x5 block SHATTERS it: measured over our 45 river
# seeds, a majority leaves a median 13 fragments whose longest run is 9 cells,
# and erases the creek outright on two seeds. Five of twenty-five sub-cells
# leaves one continuous channel, median 385 cells long, and is identical to a
# majority on a lake. What d_water needs is a connected channel in the right
# place, not an exact area -- and the area cannot be right either way, since a
# 3.5 m creek rasterised at 10 m either occupies a whole cell or vanishes.
WET_BLOCK_FRAC = 0.2


def read_index(dump: pathlib.Path):
    """seed -> (mode, lake_frac, wet_kind) from the dump's own index."""
    out = {}
    for p in sorted(dump.glob("index_*.txt")):
        for ln in p.read_text().split("\n"):
            f = ln.split()
            if len(f) >= 4:
                out[int(f[0])] = (f[1], float(f[2]), f[3])
    return out


def arch_key(mode: str, wet_kind: str) -> str:
    return f"{mode}_river" if wet_kind == "river" else mode


def zscore(a):
    return (a - a.mean()) / max(a.std(), 1e-12)


def tile_inputs(seed: int, dump: pathlib.Path):
    """The frozen fields, and the 10 m water mask and class raster from them."""
    z2, (ox, oy, c2) = cgrid.read_f32(dump / f"m_{seed}.cgrid")
    w2, _ = cgrid.read_f32(dump / f"m_{seed}.water.cgrid")
    # canopy_metrics.run's fill, verbatim: float64 is load-bearing, since the
    # block mean and the gaussian accumulation are what the fit standardised
    z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(float)
    k = max(1, int(round(CELL / float(c2))))
    n = z2.shape[0] // k
    wet2 = np.isfinite(w2)[:n * k, :n * k].astype(float)
    wet10 = wet2.reshape(n, k, n, k).mean(axis=(1, 3)) >= WET_BLOCK_FRAC
    cls = np.where(wet10, CLASS_WATER, CLASS_OTHER).astype(np.uint8)
    return z2, float(c2), cls, wet10, (ox, oy)


def logit_field(pr: dict, n: int, fit: dict) -> np.ndarray:
    """The fitted model on our ground, standardised exactly as it was fitted."""
    t = np.full((n, n), float(fit["intercept"]))
    for k in model.FEATURES:
        x = np.asarray(pr[k][:n, :n], float)
        t = t + fit["coef"][k] * (x - fit["mean"][k]) / fit["std"][k]
    return t


def octave_fields(seed: int, n: int) -> list:
    """One unit-variance smoothed-noise field per octave, cached-friendly.

    White noise convolved with a Gaussian of sigma has a Gaussian
    autocovariance of sigma*sqrt(2), whose 1/e distance is 2*sigma -- so a
    feature length L means sigma = L / (2 * CELL) cells, and "L = 80 m" says
    something. `wrap` keeps the synthetic field stationary; `reflect` would put
    a variance dip round the tile edge.
    """
    rng = np.random.default_rng([int(seed), model.TAG_NOISE])
    out = []
    for L in model.OCTAVES_M:
        w = rng.standard_normal((n, n))
        out.append(zscore(ndi.gaussian_filter(w, L / (2.0 * CELL), mode="wrap")))
    return out


def score_field(pr, n, fit, octs, p) -> np.ndarray:
    """Model plus fractal noise, mixed to unit variance."""
    t = logit_field(pr, n, fit)
    if p["sm"] > 0:
        t = ndi.gaussian_filter(t, p["sm"] / CELL)
    T = zscore(t)
    w = np.array([L ** p["H"] for L in model.OCTAVES_M], float)
    w[0] *= (1.0 + p.get("grain", 0.0) * 10.0)
    F = zscore(sum(wi * f for wi, f in zip(w, octs)))
    a = float(p["a"])
    return a * T + np.sqrt(max(1.0 - a * a, 0.0)) * F


def threshold(s: np.ndarray, wet10: np.ndarray, c: float) -> np.ndarray:
    """Exactly `c` of the tile treed, and never on water.

    Water sits at -inf AFTER the z-scores, so it cannot be selected and cannot
    poison a mean or a standard deviation. Coverage is measured over the whole
    tile including water, which is how `tree_frac` is measured on the real
    tiles, so the two denominators agree and no correction is needed.
    """
    s = np.where(wet10, -np.inf, s)
    c = float(np.clip(c, 0.0, max(0.0, 1.0 - wet10.mean() - 0.01)))
    if c <= 0.0:
        return np.zeros(s.shape, bool)
    return s > np.quantile(s, 1.0 - c)


def canopy_pct(tree: np.ndarray, density: float, r_edge: float) -> np.ndarray:
    """Percent canopy inside the stands, ramping up from their edges.

    Round 1 measured the ramp on real courses: canopy reaches half its
    far-field value about 30 m in from a cleared edge. The scale is solved per
    tile so the tile's canopy-over-treed ratio is the measured savanna density
    exactly -- 0.66 for the Carolina pine, 0.33 for the dune country.
    """
    if not tree.any():
        return np.zeros(tree.shape, np.uint8), False
    d = ndi.distance_transform_edt(tree, sampling=CELL)
    f = np.clip(d / r_edge, 0.0, 1.0)
    k = density * tree.mean() * 100.0 / max(f.mean(), 1e-12)
    raw = k * f
    return np.clip(np.rint(raw), 0, 100).astype(np.uint8), bool(raw.max() > 100.0)


def coverage_for(seeds, cover_key: str) -> dict:
    """Stratified inverse-CDF, so the cohort REPRODUCES the real distribution.

    Drawing 87 coverages i.i.d. from a 54-point sample this skewed leaves a
    visible wobble in the cohort median. Ranking the seeds by their own hash
    and reading the empirical quantile at each rank reproduces the distribution
    while staying deterministic and independent of seed order.

    `inverted_cdf` and not the default interpolation, because interpolating
    between the two largest of 54 observations can never return the largest,
    and on a distribution whose mean lives in its tail that shortfall is the
    whole error: the aeolian cohort came out at 6.66 percent treed against a
    real 7.19, and its p90 at 21.5 against 25.4. Reading the step function
    instead gives 7.16 and 25.4, and has the better property anyway -- every
    coverage we generate is one that was actually observed on real ground.
    """
    fr = np.asarray(model.COVERAGE[cover_key], float)
    keys = {s: np.random.default_rng([int(s), model.TAG_COVERAGE]).random()
            for s in seeds}
    order = sorted(seeds, key=lambda s: (keys[s], s))
    n = len(order)
    return {s: float(np.quantile(fr, (i + 0.5) / n, method="inverted_cdf"))
            for i, s in enumerate(order)}


def generate(seed, dump, place_key, cover_key, c, params=None):
    """One tile: the class raster, the canopy percent, and the statistics."""
    # structure follows the archetype we are trying to look like; the
    # coefficients that bias WHERE follow the fit we trust on this terrain
    p = dict(params or model.PARAMS[cover_key])
    density = model.DENSITY[cover_key]
    z2, c2, cls, wet10, origin = tile_inputs(seed, dump)
    pr, n = cm.predictors(z2, c2, cls)
    s = score_field(pr, n, model.FITS[place_key], octave_fields(seed, n), p)
    tree = threshold(s, wet10[:n, :n], c)
    tcc, clipped = canopy_pct(tree, density, p["r_edge"])
    grid = np.where(wet10[:n, :n], CLASS_WATER,
                    np.where(tree, CLASS_TREE, CLASS_OTHER)).astype(np.uint8)
    z10 = pr["elev_rel"][:n, :n]
    st = dict(cm.patch_stats(tree), tree_frac=float(tree.mean()),
              tcc_mean=float(tcc.mean()), c_target=float(c), n=int(n),
              clipped=clipped, wet_frac=float(wet10[:n, :n].mean()))
    return grid, tcc, dict(st, seed=int(seed), place=place_key), (z10, wet10[:n, :n], origin)


def write(out_dir, seed, grid, tcc, origin):
    ox, oy = origin
    cgrid.write_u8(out_dir / f"m_{seed}.canopy.cgrid", grid, ox, oy, CELL)
    cgrid.write_u8(out_dir / f"m_{seed}.tcc.cgrid", tcc, ox, oy, CELL)


def run(dump: pathlib.Path, out_dir: pathlib.Path, only=None):
    idx = read_index(dump)
    seeds = sorted(idx) if not only else [s for s in sorted(idx) if s in set(only)]
    cohorts = {}
    for s in seeds:
        mode, _lf, wk = idx[s]
        cohorts.setdefault(model.COVER_FIT[arch_key(mode, wk)], []).append(s)
    cov = {}
    for key, ss in cohorts.items():
        cov.update(coverage_for(ss, key))
    out_dir.mkdir(parents=True, exist_ok=True)
    rows = []
    for s in seeds:
        mode, _lf, wk = idx[s]
        ak = arch_key(mode, wk)
        place, cover = model.PLACE_FIT[ak], model.COVER_FIT[ak]
        grid, tcc, st, extra = generate(s, dump, place, cover, cov[s])
        write(out_dir, s, grid, tcc, extra[2])
        st.update(mode=mode, wet_kind=wk, arch=ak, cover=model.COVER_FIT[ak])
        rows.append(st)
    (out_dir / "index_canopy.json").write_text(json.dumps(
        dict(params=model.PARAMS, dump=str(dump), rows=rows), indent=1))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dump")
    ap.add_argument("out")
    ap.add_argument("--seeds")
    a = ap.parse_args()
    only = [int(x) for x in a.seeds.split(",")] if a.seeds else None
    t0 = time.time()
    rows = run(pathlib.Path(a.dump), pathlib.Path(a.out), only)
    print(f"{len(rows)} tiles in {time.time() - t0:.0f} s")
    for k in ("sandhills", "sandhills_nc", "sandhills_river"):
        rs = [r for r in rows if r["cover"] == k]
        if not rs:
            continue
        tf = sorted(100 * r["tree_frac"] for r in rs)
        print(f"  {k:16s} {len(rs):3d} seeds  treed mean {np.mean(tf):6.2f} %  "
              f"p50 {tf[len(tf) // 2]:6.2f} %  lone/km2 med "
              f"{np.median([r['isolated_per_km2'] for r in rs]):5.2f}")


if __name__ == "__main__":
    main()
