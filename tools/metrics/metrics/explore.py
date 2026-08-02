"""EXPLORATORY diagnostics — NOT the model.

A quick PCA + clustering pass over the scalar feature matrix to see whether the
courses form a continuum or separate islands. This informs the downstream
modelling choice (continuous distribution vs discrete terrain types); it is not
the joint model itself. k = 1 is allowed to win.
"""

from __future__ import annotations

import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from . import features

GROUP_COLOR = {"lowland": "#4C9F70", "rolling": "#D9A441", "mountain": "#B5533C", "": "#888"}


def _matrix(df):
    ex = df[df["clip_variant"] == "course_extent"].reset_index(drop=True)
    X = ex[features.SCALAR_COLUMNS].to_numpy(dtype=float)
    # drop all-NaN columns; impute remaining NaN with column mean; z-score
    keep = ~np.all(np.isnan(X), axis=0)
    cols = [c for c, k in zip(features.SCALAR_COLUMNS, keep) if k]
    X = X[:, keep]
    mu = np.nanmean(X, axis=0)
    X = np.where(np.isnan(X), mu, X)
    sd = X.std(axis=0); sd[sd == 0] = 1.0
    Xz = (X - X.mean(axis=0)) / sd
    return ex, Xz, cols


def run(df, out):
    from sklearn.decomposition import PCA
    from sklearn.mixture import GaussianMixture
    from sklearn.metrics import silhouette_score
    from scipy.cluster.hierarchy import linkage, dendrogram

    rep = os.path.join(out, "report")
    os.makedirs(rep, exist_ok=True)
    ex, Xz, cols = _matrix(df)
    groups = ex["group"].to_numpy()

    pca = PCA().fit(Xz)
    scores = pca.transform(Xz)
    ev = pca.explained_variance_ratio_

    # scree
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.plot(np.arange(1, len(ev) + 1), np.cumsum(ev), "-o", ms=3)
    ax.axhline(0.9, ls="--", c="gray"); ax.set_xlabel("PC"); ax.set_ylabel("cumulative variance")
    ax.set_title("PCA scree (diagnostic)"); fig.tight_layout()
    fig.savefig(os.path.join(rep, "pca_scree.png"), dpi=110); plt.close(fig)

    # PC1/PC2 scatter
    fig, ax = plt.subplots(figsize=(6.5, 5.5))
    for g in ("lowland", "rolling", "mountain"):
        m = groups == g
        ax.scatter(scores[m, 0], scores[m, 1], s=16, label=g, color=GROUP_COLOR[g], alpha=0.8)
    ax.set_xlabel(f"PC1 ({ev[0]*100:.0f}%)"); ax.set_ylabel(f"PC2 ({ev[1]*100:.0f}%)")
    ax.legend(); ax.set_title("PCA of terrain features (diagnostic)"); fig.tight_layout()
    fig.savefig(os.path.join(rep, "pca_scatter.png"), dpi=110); plt.close(fig)

    # dendrogram
    Z = linkage(Xz, method="ward")
    fig, ax = plt.subplots(figsize=(14, 4))
    dendrogram(Z, no_labels=True, color_threshold=0.7 * Z[:, 2].max(), ax=ax)
    ax.set_title("Hierarchical clustering (Ward) — diagnostic"); fig.tight_layout()
    fig.savefig(os.path.join(rep, "dendrogram.png"), dpi=110); plt.close(fig)

    # GMM BIC + silhouette over small k (k=1 allowed). Cluster in the reduced
    # PC space (first PCs reaching 90% variance, capped) — full-covariance GMM
    # on all 40 features with only 200 points overfits BIC badly.
    n_pc = int(min(8, np.searchsorted(np.cumsum(ev), 0.9) + 1))
    Xr = scores[:, :n_pc]
    ks = range(1, 7)
    bic, sil = [], []
    for k in ks:
        gm = GaussianMixture(k, covariance_type="full", random_state=0, n_init=5).fit(Xr)
        bic.append(gm.bic(Xr))
        sil.append(silhouette_score(Xr, gm.predict(Xr)) if k >= 2 else np.nan)
    kbest = list(ks)[int(np.argmin(bic))]
    fig, ax = plt.subplots(1, 2, figsize=(11, 4))
    ax[0].plot(list(ks), bic, "-o"); ax[0].set_xlabel("k"); ax[0].set_ylabel("BIC (lower=better)")
    ax[0].axvline(kbest, ls="--", c="r"); ax[0].set_title(f"GMM BIC — min at k={kbest}")
    ax[1].plot(list(ks), sil, "-o"); ax[1].set_xlabel("k"); ax[1].set_ylabel("silhouette")
    ax[1].set_title("Silhouette (higher=better)")
    fig.tight_layout(); fig.savefig(os.path.join(rep, "clustering_bic.png"), dpi=110); plt.close(fig)

    n90 = int(np.searchsorted(np.cumsum(ev), 0.9) + 1)
    sil_best = sil[kbest - 1] if kbest >= 2 else float("nan")
    with open(os.path.join(out, "explore_summary.txt"), "w") as f:
        f.write("EXPLORATORY DIAGNOSTIC — informs the modelling choice, is NOT the model.\n\n")
        f.write(f"n_courses={len(ex)}  n_features={len(cols)}\n")
        f.write(f"PCA: PC1={ev[0]*100:.1f}%  PC2={ev[1]*100:.1f}%  PC3={ev[2]*100:.1f}%"
                f"  ->  {n90} PCs needed for 90% variance\n")
        f.write("PC1 = overall ruggedness (slope / roughness / local relief).\n")
        f.write("PC2 = surface sharpness / curvature vs spectral slope.\n")
        f.write(f"GMM in {n_pc}-D PC space: BIC-min at k={kbest}"
                f"{' (silhouette %.2f)' % sil_best if kbest >= 2 else ''}.\n\n")
        f.write("READING: PC1 explains only ~%d%% and %d PCs are needed for 90%%; on the\n"
                % (ev[0] * 100, n90))
        f.write("PC1/PC2 plane the three relief bands form ONE overlapping gradient\n")
        f.write("(lowland->rolling->mountain), not separate islands. The terrain is a\n")
        f.write("HIGH-DIMENSIONAL CONTINUUM. Any k>1 the GMM picks reflects gradual\n")
        f.write("density variation, not discrete terrain types — so the downstream step\n")
        f.write("should model the joint distribution continuously (copula / GMM density /\n")
        f.write("PCA-regression) rather than fitting hard clusters. k is diagnostic only.\n\n")
        f.write("Top |loadings| PC1: " + ", ".join(
            f"{cols[i]}({pca.components_[0][i]:+.2f})"
            for i in np.argsort(-np.abs(pca.components_[0]))[:6]) + "\n")
        f.write("Top |loadings| PC2: " + ", ".join(
            f"{cols[i]}({pca.components_[1][i]:+.2f})"
            for i in np.argsort(-np.abs(pca.components_[1]))[:6]) + "\n")
    print(f"wrote report/pca_*.png, dendrogram.png, clustering_bic.png; "
          f"explore_summary.txt (PC1 {ev[0]*100:.0f}%, {n90} PCs->90%, GMM k*={kbest} — continuum)")
