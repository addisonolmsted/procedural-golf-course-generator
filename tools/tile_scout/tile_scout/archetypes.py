"""Soft terrain archetypes over the course S1 vectors — bookkeeping for
region quotas + report naming, never hard assignment (v1 finding of record:
terrain is a continuum). Matching itself is course-nearest throughout."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import pandas as pd

from . import sconfig
from . import scoring


@dataclass
class Archetypes:
    keys: list[str]
    metric_cols: list[str]
    center: pd.Series
    scale: pd.Series
    z: pd.DataFrame            # 135 x metrics (course frame)
    resp: np.ndarray           # 135 x k responsibilities
    names: list[str]           # per component, exemplar-based


def fit(cfg: dict) -> Archetypes:
    acfg = cfg["archetypes"]
    df = pd.read_parquet(sconfig.repo_path(cfg["paths"]["course_metrics_parquet"]))
    cols = list(acfg["metric_cols"])
    t = scoring.transform(df, cols, acfg["log1p_cols"])
    t = t.fillna(t.median())
    med, sd = scoring.robust_scales(t)
    z = scoring.zscore(t, med, sd)

    from sklearn.decomposition import PCA
    from sklearn.mixture import GaussianMixture
    pca = PCA(n_components=min(acfg["pca_max"], len(cols)),
              random_state=cfg["seed"])
    pcs = pca.fit_transform(z.to_numpy())
    n_pc = int(np.searchsorted(np.cumsum(pca.explained_variance_ratio_),
                               acfg["pca_var"]) + 1)
    pcs = pcs[:, :n_pc]

    best = None
    lo, hi = acfg["gmm_k_range"]
    for k in range(lo, hi + 1):
        gm = GaussianMixture(n_components=k, covariance_type="full",
                             n_init=acfg["gmm_n_init"],
                             random_state=cfg["seed"]).fit(pcs)
        bic = gm.bic(pcs)
        if best is None or bic < best[0]:
            best = (bic, k, gm)
    _bic, k, gm = best
    resp = gm.predict_proba(pcs)

    names = []
    for j in range(k):
        top = np.argsort(resp[:, j])[::-1][:3]
        names.append("-".join(df.index[i] for i in top))

    return Archetypes(keys=list(df.index), metric_cols=cols, center=med,
                      scale=sd, z=z, resp=resp, names=names)


def save(a: Archetypes, path: str) -> None:
    sconfig.save_json(path, {
        "keys": a.keys,
        "metric_cols": a.metric_cols,
        "center": {c: round(float(v), 6) for c, v in a.center.items()},
        "scale": {c: round(float(v), 6) for c, v in a.scale.items()},
        "resp": [[round(float(v), 4) for v in row] for row in a.resp],
        "names": a.names,
        "z": {k: [round(float(v), 4) for v in a.z.loc[k]] for k in a.keys},
    })


def load(path: str) -> Archetypes:
    import json
    with open(path) as f:
        d = json.load(f)
    z = pd.DataFrame([d["z"][k] for k in d["keys"]], index=d["keys"],
                     columns=d["metric_cols"])
    return Archetypes(keys=d["keys"], metric_cols=d["metric_cols"],
                      center=pd.Series(d["center"]),
                      scale=pd.Series(d["scale"]), z=z,
                      resp=np.array(d["resp"]), names=d["names"])


def run(cfg: dict) -> int:
    a = fit(cfg)
    out = sconfig.out_path("archetypes.json")
    save(a, out)
    print(f"{len(a.names)} archetypes over {len(a.keys)} courses "
          f"({len(a.metric_cols)} metrics):")
    w = a.resp.sum(axis=0)
    for j, n in enumerate(a.names):
        print(f"  A{j} weight {w[j]:5.1f}  exemplars {n}")
    print(f"wrote {out}")
    return 0
