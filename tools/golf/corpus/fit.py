"""Fame-weighted L2 logistic over the named features (Phase 4).

IRLS in ~40 lines — the model is tiny and the repo avoids heavy deps; no
scikit-learn. Split is BY COURSE, 80/20, stratified by fame tier x region,
frozen into holdout.json so every later phase evaluates on the same courses.

Outputs out/fitted/coeffs.json: raw-unit coefficients (standardization folded
in) ready to transcribe into greens.py, plus bootstrap CIs and the AUC table.
"""

from __future__ import annotations

import hashlib
import json

import numpy as np

from . import config
from .features import FEATURES

FIT_FEATURES = [f for f in FEATURES if f not in ("pad_green", "confirm")]
SEED = 20260829


def load_rows(path=None) -> list[dict]:
    p = path or (config.OUT / "features.jsonl")
    return [json.loads(ln) for ln in p.read_text().splitlines() if ln.strip()]


def split_courses(rows: list[dict]) -> tuple[set, set]:
    """80/20 by course, stratified fame x region, deterministic; frozen."""
    hp = config.PKG / "holdout.json"
    if hp.exists():
        held = set(json.loads(hp.read_text())["holdout"])
        train = {r["course_id"] for r in rows} - held
        return train, held
    by_stratum: dict[tuple, list] = {}
    for r in rows:
        by_stratum.setdefault((r["fame"], r["region"]), []).append(r["course_id"])
    held = set()
    for key, cids in sorted(by_stratum.items()):
        cids = sorted(set(cids),
                      key=lambda c: hashlib.sha256(c.encode()).hexdigest())
        k = max(1, round(0.2 * len(cids))) if len(cids) > 1 else 0
        held.update(cids[:k])
    hp.write_text(json.dumps({"holdout": sorted(held), "seed": SEED},
                             indent=1))
    train = {r["course_id"] for r in rows} - held
    return train, held


def design(rows: list[dict], feats=None):
    feats = feats or FIT_FEATURES
    X = np.array([[r[f] for f in feats] for r in rows], float)
    y = np.array([r["label"] for r in rows], float)
    w = np.array([config.FAME_WEIGHTS[r["fame"]] for r in rows], float)
    return X, y, w


def irls_logistic(X, y, w, l2=1.0, iters=60):
    """Weighted L2 logistic; returns (beta, intercept) in STANDARDIZED units
    plus the (mu, sd) to fold back out."""
    mu = X.mean(0)
    sd = X.std(0)
    sd[sd < 1e-9] = 1.0
    Z = np.hstack([np.ones((len(X), 1)), (X - mu) / sd])
    beta = np.zeros(Z.shape[1])
    lam = np.full(Z.shape[1], l2)
    lam[0] = 0.0                       # never shrink the intercept
    for _ in range(iters):
        eta = np.clip(Z @ beta, -30, 30)
        p = 1.0 / (1.0 + np.exp(-eta))
        wt = w * np.maximum(p * (1 - p), 1e-6)
        g = Z.T @ (w * (y - p)) - lam * beta
        H = (Z * wt[:, None]).T @ Z + np.diag(lam)
        step = np.linalg.solve(H, g)
        beta += step
        if np.abs(step).max() < 1e-8:
            break
    return beta, mu, sd


def raw_units(beta, mu, sd, feats):
    """Fold standardization into raw-unit coefficients + intercept."""
    b = beta[1:] / sd
    b0 = float(beta[0] - (beta[1:] * mu / sd).sum())
    return {f: float(c) for f, c in zip(feats, b)}, b0


def auc(scores, y, w=None) -> float:
    w = np.ones(len(y)) if w is None else np.asarray(w, float)
    order = np.argsort(scores, kind="stable")
    s, yy, ww = np.asarray(scores)[order], np.asarray(y)[order], w[order]
    cum_neg = np.cumsum(ww * (yy == 0))
    num = float((ww * (yy == 1) * cum_neg).sum())
    den = float((ww * (yy == 1)).sum() * (ww * (yy == 0)).sum())
    return num / den if den > 0 else 0.5


def score_rows(rows, coeffs, b0, feats=None):
    feats = feats or FIT_FEATURES
    X = np.array([[r[f] for f in feats] for r in rows], float)
    b = np.array([coeffs[f] for f in feats])
    return X @ b + b0


def hand_score(rows) -> np.ndarray:
    """The shipped greens.py score expressed over the feature table — the
    baseline ranker the fit must beat on the SAME rows."""
    out = []
    for r in rows:
        sur = r["surround"]
        if sur <= 1.5:
            st = -1.0
        elif sur < 3.5:
            st = (sur - 3.5) / 2.0
        elif sur <= 14.0:
            st = 1.0
        else:
            st = max(0.4, 1.0 - (sur - 14.0) / 10.0)
        out.append(r["room"] / 40.0 + 1.2 * r["vis_best"]
                   + 0.8 * r["recept_best"] + 0.4 * r["backdrop_best"]
                   + 0.6 * r["aroom_best"] + 1.0 * st
                   - 2.0 * r["resid"] - (0.5 if r["graded"] else 0.0))
    return np.array(out)


def bootstrap_ci(rows, feats, n_boot=200, l2=1.0):
    """Course-level bootstrap; returns {feat: (lo, hi)} raw-unit 90% CI."""
    rng = np.random.default_rng(SEED)
    by_course: dict[str, list] = {}
    for r in rows:
        by_course.setdefault(r["course_id"], []).append(r)
    cids = sorted(by_course)
    acc = {f: [] for f in feats}
    for _ in range(n_boot):
        pick = rng.choice(len(cids), len(cids), replace=True)
        sample = [r for i in pick for r in by_course[cids[i]]]
        X, y, w = design(sample, feats)
        beta, mu, sd = irls_logistic(X, y, w, l2=l2)
        co, _ = raw_units(beta, mu, sd, feats)
        for f in feats:
            acc[f].append(co[f])
    return {f: (float(np.percentile(v, 5)), float(np.percentile(v, 95)))
            for f, v in acc.items()}


def run(l2=1.0) -> dict:
    rows = load_rows()
    train_c, held_c = split_courses(rows)
    tr = [r for r in rows if r["course_id"] in train_c]
    he = [r for r in rows if r["course_id"] in held_c]
    X, y, w = design(tr)
    beta, mu, sd = irls_logistic(X, y, w, l2=l2)
    coeffs, b0 = raw_units(beta, mu, sd, FIT_FEATURES)

    res = dict(n_train_rows=len(tr), n_held_rows=len(he),
               n_train_courses=len(train_c), n_held_courses=len(held_c),
               coeffs=coeffs, intercept=b0, l2=l2, seed=SEED,
               features=FIT_FEATURES)
    res["auc_held_fitted"] = auc(score_rows(he, coeffs, b0),
                                 [r["label"] for r in he])
    res["auc_held_hand"] = auc(hand_score(he), [r["label"] for r in he])
    res["auc_train_fitted"] = auc(score_rows(tr, coeffs, b0),
                                  [r["label"] for r in tr])
    # unweighted-fame ablation (fame effect must be visible, plan rule)
    beta_u, mu_u, sd_u = irls_logistic(X, y, np.ones(len(y)), l2=l2)
    cu, b0u = raw_units(beta_u, mu_u, sd_u, FIT_FEATURES)
    res["auc_held_unweighted"] = auc(score_rows(he, cu, b0u),
                                     [r["label"] for r in he])
    # per-region held-out AUC
    regs = sorted({r["region"] for r in he})
    res["auc_held_by_region"] = {
        g: auc(score_rows([r for r in he if r["region"] == g], coeffs, b0),
               [r["label"] for r in he if r["region"] == g])
        for g in regs if sum(r["region"] == g for r in he) > 40}
    res["ci90"] = bootstrap_ci(tr, FIT_FEATURES, l2=l2)
    out = config.OUT / "fitted"
    out.mkdir(parents=True, exist_ok=True)
    (out / "coeffs.json").write_text(json.dumps(res, indent=1, sort_keys=True))
    return res
