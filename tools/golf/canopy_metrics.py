"""Where trees grow, as a function of the land, on natural tiles.

  python3 tools/golf/canopy_metrics.py [--archetype sandhills_nc] [--limit 40]

Reads the macro_campaign archetype tiles, which are 3 km of 2 m elevation
sited on national forest, refuge and wilderness land, together with the
ESA WorldCover classes and NLCD canopy percent fetched beside them by
`corpus/fetch_canopy.py`. No golf has touched these, so they answer "where do
trees grow" without the clearing that contaminates a course tile.

Every predictor is one the router already computes in `Fields`, so a fitted
model can be evaluated on our own tiles without new terrain machinery:
relief position, slope, tpi200, northness, curvature, roughness and distance
to water. They are derived at the canopy's own 10 m grid.

Two different measures of tree cover are kept apart on purpose. WorldCover
says whether a 10 m cell is treed LAND; NLCD says what fraction of the ground
is under CANOPY. A pine savanna reads high on the first and middling on the
second, and that gap is the thing that separates a savanna from a closed wood.

Writes `out/canopy_by_terrain.json` and prints the per-archetype summary.
"""
import argparse
import json
import pathlib
import sys

import numpy as np
import scipy.ndimage as ndi

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                  # noqa: E402

TILES = HERE.parents[0] / "macro_campaign" / "out" / "tiles"
OUT = HERE / "corpus" / "out" / "canopy_by_terrain.json"
CELL = 10.0
CLASS_TREE, CLASS_WATER = 10, 80
LOWPASS_M = 400.0          # the router's relief_pos lowpass
TPI_M = 200.0
BINS = 12


def predictors(z2: np.ndarray, cell2: float, cls: np.ndarray):
    """The router's `Fields` quantities, on the canopy's 10 m grid."""
    k = max(1, int(round(CELL / cell2)))
    n = min(z2.shape[0] // k, cls.shape[0])
    z = z2[:n * k, :n * k].reshape(n, k, n, k).mean(axis=(1, 3))      # block mean
    rough = z2[:n * k, :n * k].reshape(n, k, n, k).std(axis=(1, 3))
    gy, gx = np.gradient(z, CELL)
    gmag = np.hypot(gy, gx)
    lp = ndi.gaussian_filter(z, LOWPASS_M / np.pi / CELL)
    rp = (np.argsort(np.argsort(lp.ravel(), kind="stable"), kind="stable")
          .reshape(z.shape) / max(z.size - 1, 1))
    tpi = z - ndi.uniform_filter(z, int(TPI_M / CELL) | 1)
    # row 0 is SOUTH, so +y is north; a north-facing slope descends northward
    north = np.where(gmag > 1e-9, -gy / np.maximum(gmag, 1e-9), 0.0)
    curv = ndi.laplace(z) / (CELL * CELL)
    wet = cls[:n, :n] == CLASS_WATER
    dwat = (ndi.distance_transform_edt(~wet, sampling=CELL) if wet.any()
            else np.full(z.shape, 5000.0))
    return dict(slope=gmag, relief_pos=rp, tpi200=tpi, northness=north,
                curvature=curv, roughness=rough, d_water=np.minimum(dwat, 1500.0),
                elev_rel=z - z.mean()), n


def patch_stats(tree: np.ndarray) -> dict:
    """Is this scattered specimens or a closed wood? Coverage cannot tell."""
    lab, nlab = ndi.label(tree, structure=np.ones((3, 3), int))
    if nlab == 0:
        # a genuinely treeless tile, which is the normal case on the prairie
        # archetypes and the whole reason the sandhills lone tree is a
        # deliberate aesthetic rather than a match to the land
        return dict(n_patches=0, isolated_rate=0.0, isolated_per_km2=0.0,
                    p50_patch_cells=0.0, p90_patch_cells=0.0,
                    largest_frac=0.0, edge_density=0.0)
    sizes = np.bincount(lab.ravel())[1:]
    per = ndi.binary_dilation(tree) & ~tree
    return dict(
        n_patches=int(nlab),
        # a lone tree is one or two 10 m cells standing clear of everything
        isolated_rate=float(np.mean(sizes <= 2)),
        isolated_per_km2=float((sizes <= 2).sum() / (tree.size * CELL * CELL / 1e6)),
        p50_patch_cells=float(np.percentile(sizes, 50)),
        p90_patch_cells=float(np.percentile(sizes, 90)),
        largest_frac=float(sizes.max() / max(tree.sum(), 1)),
        edge_density=float(per.sum() / max(tree.sum(), 1)))


def run(only=None, limit=None):
    rows = []
    for js in sorted(TILES.glob("*/*.json")):
        wc = js.with_suffix(".canopy.cgrid")
        if not wc.exists():
            continue
        arch = js.parent.name
        if only and arch != only:
            continue
        try:
            cls, (_, _, _) = cgrid.read_u8(wc)
            z2, (_, _, c2) = cgrid.read_f32(js.with_suffix(".cgrid"))
        except Exception:
            continue
        z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(float)
        pr, n = predictors(z2, float(c2), cls)
        tree = (cls[:n, :n] == CLASS_TREE)
        tcc_p = js.with_suffix(".tcc.cgrid")
        tcc = None
        if tcc_p.exists():
            t, (_, _, tc) = cgrid.read_u8(tcc_p)
            tcc = float(t.mean())
        rows.append(dict(archetype=arch, tile=js.stem, n=int(n),
                         tree_frac=float(tree.mean()), tcc_mean=tcc,
                         patch=patch_stats(tree),
                         # the binned response of tree cover to each predictor
                         curves={k: _curve(v[:n, :n], tree) for k, v in pr.items()}))
        if limit and len(rows) >= limit:
            break
    return rows


def _curve(x: np.ndarray, tree: np.ndarray):
    """Tree rate in each decile of `x`, with the bin centres."""
    v = x.ravel()
    t = tree.ravel()
    qs = np.quantile(v, np.linspace(0, 1, BINS + 1))
    qs[-1] += 1e-9
    out = []
    for i in range(BINS):
        m = (v >= qs[i]) & (v < qs[i + 1])
        if m.sum() < 20:
            continue
        out.append([float(0.5 * (qs[i] + qs[i + 1])), float(t[m].mean()), int(m.sum())])
    return out


FEATURES = ("relief_pos", "tpi200", "slope", "roughness", "northness",
            "curvature", "d_water", "elev_rel")


def fit_archetype(arch, max_tiles=40, per_tile=4000, seed=0):
    """A logistic model of tree presence from the terrain, for one archetype.

    Cells are pooled from up to `max_tiles` tiles, subsampled evenly so one
    big tile cannot dominate. Features are standardised so the coefficients
    are comparable, and the standardisation is returned with them because the
    generator needs it to evaluate the model on our own tiles. Fitted by
    scipy's minimiser on the regularised log-loss; no new dependency.
    """
    from scipy.optimize import minimize
    rng = np.random.default_rng(seed)
    X, y = [], []
    for js in sorted(TILES.glob(f"{arch}/*.json"))[:max_tiles]:
        wc = js.with_suffix(".canopy.cgrid")
        if not wc.exists():
            continue
        cls, _ = cgrid.read_u8(wc)
        z2, (_, _, c2) = cgrid.read_f32(js.with_suffix(".cgrid"))
        z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(float)
        pr, n = predictors(z2, float(c2), cls)
        tree = (cls[:n, :n] == CLASS_TREE).ravel()
        cols = np.stack([pr[k][:n, :n].ravel() for k in FEATURES], axis=1)
        idx = rng.choice(cols.shape[0], size=min(per_tile, cols.shape[0]), replace=False)
        X.append(cols[idx])
        y.append(tree[idx])
    if not X:
        return None
    X = np.concatenate(X)
    y = np.concatenate(y).astype(float)
    mu, sd = X.mean(axis=0), X.std(axis=0) + 1e-9
    Z = np.column_stack([np.ones(len(X)), (X - mu) / sd])

    def nll(w):
        t = Z @ w
        # log(1 + exp(t)) written to avoid overflow
        ll = np.logaddexp(0.0, t) - y * t
        return ll.mean() + 1e-3 * np.dot(w[1:], w[1:])

    res = minimize(nll, np.zeros(Z.shape[1]), method="L-BFGS-B")
    w = res.x
    pred = 1.0 / (1.0 + np.exp(-(Z @ w)))
    base = y.mean()
    return dict(archetype=arch, n_cells=int(len(y)), base_rate=float(base),
                intercept=float(w[0]),
                coef={k: float(v) for k, v in zip(FEATURES, w[1:])},
                mean={k: float(v) for k, v in zip(FEATURES, mu)},
                std={k: float(v) for k, v in zip(FEATURES, sd)},
                # how much better than always predicting the base rate
                brier=float(np.mean((pred - y) ** 2)),
                brier_base=float(np.mean((base - y) ** 2)))


def summarize(rows):
    by = {}
    for r in rows:
        by.setdefault(r["archetype"], []).append(r)
    print(f"{'archetype':16s} {'tiles':>5s} {'treed land':>11s} {'canopy %':>9s} "
          f"{'density':>8s} {'patches/km2':>12s} {'lone/km2':>9s} {'p50 patch':>10s} {'largest':>8s}")
    out = {}
    for a, rs in sorted(by.items()):
        tf = 100 * float(np.mean([r["tree_frac"] for r in rs]))
        tc = [r["tcc_mean"] for r in rs if r["tcc_mean"] is not None]
        tcm = float(np.mean(tc)) if tc else float("nan")
        dens = tcm / tf if tf > 1e-6 else float("nan")
        # MEDIAN, not mean: a fully forested tile is one patch of 90,000 cells
        # and a handful of them drags any mean patch size into the thousands
        pk = float(np.median([r["patch"]["n_patches"] / (r["n"] * CELL / 1000.0) ** 2 for r in rs]))
        lone = float(np.median([r["patch"]["isolated_per_km2"] for r in rs]))
        p50 = float(np.median([r["patch"]["p50_patch_cells"] for r in rs]))
        lg = 100 * float(np.median([r["patch"]["largest_frac"] for r in rs]))
        print(f"{a:16s} {len(rs):5d} {tf:10.1f}% {tcm:8.1f}% {dens:8.2f} "
              f"{pk:12.0f} {lone:9.0f} {p50:10.1f} {lg:7.0f}%")
        tfs = sorted(100 * r["tree_frac"] for r in rs)
        out[a] = dict(tiles=len(rs), treed_land_pct=tf, canopy_pct=tcm,
                      density=dens, patches_per_km2=pk, lone_per_km2=lone,
                      p50_patch_cells=p50, largest_patch_pct=lg,
                      treed_p10=tfs[len(tfs) // 10], treed_p50=tfs[len(tfs) // 2],
                      treed_p90=tfs[min(len(tfs) - 1, 9 * len(tfs) // 10)])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--archetype")
    ap.add_argument("--limit", type=int)
    a = ap.parse_args()
    rows = run(a.archetype, a.limit)
    print(f"{len(rows)} tiles with canopy\n")
    summary = summarize(rows)
    fits = {}
    print("\nfitted placement models (standardised coefficients):")
    for arch in sorted({r["archetype"] for r in rows}):
        f = fit_archetype(arch)
        if not f:
            continue
        fits[arch] = f
        top = sorted(f["coef"].items(), key=lambda kv: -abs(kv[1]))[:4]
        skill = 1.0 - f["brier"] / max(f["brier_base"], 1e-9)
        print(f"  {arch:16s} base {100 * f['base_rate']:5.1f} %  skill {skill:5.2f}  "
              + "  ".join(f"{k} {v:+.2f}" for k, v in top))
    OUT.write_text(json.dumps(dict(summary=summary, fits=fits, tiles=rows), indent=1))
    print(f"\nwrote {OUT}")


if __name__ == "__main__":
    main()
