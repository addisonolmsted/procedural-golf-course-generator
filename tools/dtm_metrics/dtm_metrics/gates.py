"""G1-G6 metric acceptance gates -> fit_target / diagnostic / dropped, plus
the Stage-1 justification document (reports/stage1_metrics.md): every candidate
with its definition-domain rationale (the primitive-awareness argument) and the
gate evidence that decided its fate.

Evidence inputs: metrics.parquet (G4/G5), robustness/g1.parquet (G1),
stability/g2g3.parquet (G2/G3-real) + l_sensitivity, sandbox/theta_share.csv
(G3-sandbox), sandbox/sobol_st.csv + emulator_r2.csv (G6).
"""

from __future__ import annotations

import os

import numpy as np
import pandas as pd
from scipy.cluster.hierarchy import fcluster, linkage
from scipy.stats import spearmanr

from . import store

# Per-metric domain rationale (the primitive-awareness justification the plan
# requires in stage1_metrics.md). Families share a preamble; notable metrics
# get specific lines.
FAMILY_RATIONALE = {
    "s1": ("MACRO FORM — the landform primitives' domain. In the full pipeline "
           "primitives are MEASURED per course (Stage 3), never fitted, so "
           "these metrics validate extraction and gate feasibility; using them "
           "as noise-fit targets would double-count what primitives explain."),
    "s2": ("TEXTURE CHARACTER — everything below the primitive-proxy cutoff "
           "(L=200 m) that the Stage-4 noise layer must synthesize. These are "
           "the Stage-7b fitting-objective candidates."),
    "s3": ("MODULATION CONTRACT — texture conditioned on landform position. "
           "Stage 4's 'damped on floors, full on uplands, grain-aligned' spec "
           "becomes a measured, per-course, fittable quantity here."),
}

# Clean-window estimators (specwin family): the masks are load-bearing BY
# DESIGN — Laplace fill has no fine-band energy, so the masks_off variant
# deliberately violates the estimator's precondition and measures the inpaint
# bias itself, not metric instability. G2 is reported but not gating for
# these; their stability evidence is sandbox theta-share (no inpainting
# present there) + real-side ICC + G1.
MASK_DEFINED = ("s2_cw_bp_", "s2_beta_char")

METRIC_NOTES = {
    "s2_beta_char": "spectral-shape axis; mask-defined estimator (G2 informational)",
    "s2_cw_bp_100_200": "512m-tier only — sparse (n<40) on low-clean courses; straddles L",
    "s2_res_rough_cv_25": "low ICC = within-course heterogeneity; candidate DISTRIBUTION-level 7b objective (quantile match), not per-course target",
    "s2_res_rough_top10_25": "roughness concentration — the v1 anti-choppiness axis; distribution-level 7b candidate (low ICC)",
    "s2_res_rough_moran_50": "roughness clustering (blob vs fine-grained); distribution-level 7b candidate (low ICC)",
    "s2_vg_nugget": "sub-8 m stipple amplitude; expected ~0 on block-meaned lidar (degeneracy candidate)",
    "s2_cw_bp_4_8": "QA band — spike/seam detector BY DESIGN; never a fit target",
    "s1_valley_count_km2_200": "macro fragmentation; pond-injection-fragile (G1) — validation only",
    "s3_relief_pos_texture_corr": "expected redundant with the TPI ratio (deliberate G4 candidate)",
}


def _load():
    out = {}
    out["metrics"] = pd.read_parquet(os.path.join(store.OUT, "metrics.parquet"))
    out["g1"] = pd.read_parquet(os.path.join(store.OUT, "robustness", "g1.parquet"))
    out["g2g3"] = pd.read_parquet(os.path.join(store.OUT, "stability", "g2g3.parquet"))
    ts_path = os.path.join(store.OUT, "sandbox", "theta_share.csv")
    out["theta_share"] = (pd.read_csv(ts_path, index_col=0).iloc[:, 0]
                          if os.path.exists(ts_path) else pd.Series(dtype=float))
    st_path = os.path.join(store.OUT, "sandbox", "sobol_st.csv")
    out["st"] = (pd.read_csv(st_path, index_col=0)
                 if os.path.exists(st_path) else pd.DataFrame())
    r2_path = os.path.join(store.OUT, "sandbox", "emulator_r2.csv")
    out["r2"] = (pd.read_csv(r2_path, index_col=0).iloc[:, 0]
                 if os.path.exists(r2_path) else pd.Series(dtype=float))
    return out


def run() -> int:
    cfg = store.load_config()
    g = cfg["gates"]
    ev = _load()
    mdf = ev["metrics"]
    conf = mdf[~mdf["ctx_low_confidence"].astype(bool)]
    metrics = [c for c in mdf.columns
               if c.startswith(("s1_", "s2_", "s3_"))
               and not c.endswith("__clean") and mdf[c].dtype.kind == "f"]

    rows = []
    for m in metrics:
        fam = m[:2]
        r = {"metric": m, "family": fam,
             "n_finite": int(conf[m].notna().sum())}
        # G1
        if m in ev["g1"].index:
            g1r = ev["g1"].loc[m]
            is_macro = fam == "s1"
            val = g1r["worst_rel_iqr"] if is_macro else g1r["worst_rel_popstd"]
            r["g1_val"] = float(val)
            r["g1_pass"] = bool(val <= (g["g1_macro_iqr_rel"] if is_macro
                                        else g["g1_char_popstd"]))
        # G2
        if m in ev["g2g3"].index:
            row2 = ev["g2g3"].loc[m]
            mo = row2.get("g2_masks_off_rho")
            r["g2_min_rho"] = float(mo) if pd.notna(mo) else np.nan
            r["g2_mask_defined"] = m.startswith(MASK_DEFINED)
            r["g2_pass"] = bool(r["g2_mask_defined"]
                                or (pd.notna(mo) and mo >= g["g2_spearman"]))
            io = row2.get("g2_interior_only_rho")
            r["g2_interior_rho"] = float(io) if pd.notna(io) else np.nan
            icc = row2.get("g3_icc")
            r["g3_icc"] = float(icc) if pd.notna(icc) else np.nan
            macro_exempt = fam == "s1" and ("bp_400" in m or "bp_800" in m)
            r["g3_real_pass"] = bool(macro_exempt or
                                     (pd.notna(icc) and icc >= g["g3_icc"]))
        # G3 sandbox (theta share). s3 proxy metrics draw their sandbox
        # evidence from the truth-conditioned s3t twin: Stage-7b fits with
        # KNOWN skeletons, so truth conditioning is the fitting context.
        sandbox_key = m.replace("s3_", "s3t_") if m.startswith("s3_") else m
        if sandbox_key not in ev["theta_share"].index:
            sandbox_key = m
        ts = ev["theta_share"].get(sandbox_key, np.nan)
        r["g3_theta_share"] = float(ts) if pd.notna(ts) else np.nan
        r["g3_sandbox_pass"] = bool(pd.notna(ts) and ts >= g["g3_theta_share"])
        # G5 handled globally below; G6
        st_key = sandbox_key if sandbox_key in ev["st"].index else m
        st_ok = (not ev["st"].empty and st_key in ev["st"].index
                 and float(ev["st"].loc[st_key].drop("dummy", errors="ignore").max())
                 >= g["g6_min_st"])
        r2v = ev["r2"].get(st_key, np.nan)
        r["g6_max_st"] = (float(ev["st"].loc[st_key].drop("dummy", errors="ignore").max())
                          if not ev["st"].empty and st_key in ev["st"].index else np.nan)
        r["g6_r2"] = float(r2v) if pd.notna(r2v) else np.nan
        r["g6_pass"] = bool(st_ok and pd.notna(r2v) and r2v >= g["g6_min_r2"])
        rows.append(r)
    tab = pd.DataFrame(rows).set_index("metric")

    # G4 redundancy clusters over confident real courses
    avail = [m for m in metrics if conf[m].notna().sum() >= 40]
    sub = conf[avail].rank()  # spearman via rank-pearson
    rho = sub.corr().abs().fillna(0.0)
    dist = 1.0 - rho.to_numpy()
    np.fill_diagonal(dist, 0.0)
    from scipy.spatial.distance import squareform
    condensed = squareform((dist + dist.T) / 2.0, checks=False)
    clusters = fcluster(linkage(condensed, method="average"),
                        t=1.0 - g["g4_abs_rho"], criterion="distance")
    tab["g4_cluster"] = pd.Series({m: int(c) for m, c in zip(avail, clusters)})

    # pick cluster representatives: most gates passed, then highest g6 ST
    def score(m):
        r = tab.loc[m]
        passes = sum(bool(r.get(k)) for k in
                     ("g1_pass", "g2_pass", "g3_real_pass", "g3_sandbox_pass", "g6_pass"))
        return (passes, r.get("g6_max_st") or 0.0)
    tab["g4_representative"] = False
    for cl in set(clusters):
        members = [m for m, c in zip(avail, clusters) if c == cl]
        rep = max(members, key=score)
        tab.loc[rep, "g4_representative"] = True

    # classification
    def classify(m):
        r = tab.loc[m]
        fam = r["family"]
        if m == "s2_cw_bp_4_8":
            return "diagnostic"          # QA band by design
        if r["n_finite"] < 40:
            return "diagnostic"          # too sparse to fit
        if fam == "s1":
            # macro: validation/feasibility only (primitive-awareness) —
            # 'fit_target' never applies; robust macro metrics stay as
            # validation anchors, fragile ones drop to diagnostic
            return "validation" if bool(r.get("g1_pass")) else "diagnostic"
        # s2/s3 candidates:
        needed = [bool(r.get("g1_pass")), bool(r.get("g2_pass") or pd.isna(r.get("g2_min_rho"))),
                  bool(r.get("g3_real_pass")), bool(r.get("g3_sandbox_pass")),
                  bool(r.get("g6_pass")), bool(r.get("g4_representative"))]
        if all(needed):
            return "fit_target"
        # drivable + stable but redundant -> merged (diagnostic)
        return "diagnostic"
    tab["class"] = [classify(m) for m in tab.index]

    # G5 empirical relationships (global rows)
    g5 = {}
    pair = conf[["ctx_water_fraction", "s1_relief_p95_p5"]].dropna()
    g5["water_vs_relief_rho"] = float(spearmanr(pair.iloc[:, 0], pair.iloc[:, 1]).statistic)
    lp_band = conf["s1_bp_50_200"].dropna()
    g5["macro_band_amp_p25_p75"] = [float(lp_band.quantile(0.25)),
                                    float(lp_band.quantile(0.75))]
    beta = conf["s2_beta_char"].dropna()
    g5["beta_char_p10_p90"] = [float(beta.quantile(0.10)), float(beta.quantile(0.90))]

    os.makedirs(os.path.join(store.OUT, "reports"), exist_ok=True)
    tab.to_parquet(os.path.join(store.OUT, "gates.parquet"))

    # ---- stage1_metrics.md ----
    lines = ["# Stage-1 metric battery — gate evidence + rationale", "",
             "Gate design notes:",
             f"- G1: injection worst-case; macro <= {g['g1_macro_iqr_rel']:.0%} "
             f"IQR-rel, character <= {g['g1_char_popstd']} pop-std.",
             f"- G2 gates the masks_off variant only (rho >= {g['g2_spearman']}); "
             "interior_only is descriptive — the 500 m margin is a different "
             "region by design. Clean-window estimators (cw_bp_*, beta_char) are "
             "G2-exempt: masks are load-bearing for them (masks_off measures the "
             "inpaint bias the estimator exists to avoid); their stability "
             "evidence is sandbox theta-share + ICC + G1.",
             f"- G3 real: split-half ICC >= {g['g3_icc']} (between-course signal "
             "dominates within-course heterogeneity); sandbox theta-share >= "
             f"{g['g3_theta_share']} is the controlled-replication stability gate.",
             "- Low-ICC but drivable roughness-organization metrics stay "
             "diagnostic per-course but are candidates for DISTRIBUTION-level "
             "Stage-7b objectives (population quantile matching).", ""]
    for fam in ("s1", "s2", "s3"):
        lines += [f"## {fam.upper()} — {FAMILY_RATIONALE[fam]}", ""]
        sub = tab[tab["family"] == fam]
        lines += ["| metric | class | n | G1 | G2 rho | G3 icc | th-share | G6 ST | R2 | note |",
                  "|---|---|---|---|---|---|---|---|---|---|"]
        for m, r in sub.iterrows():
            def f(v, d=3):
                return f"{v:.{d}f}" if pd.notna(v) else "-"
            lines.append(
                f"| {m} | **{r['class']}** | {r['n_finite']} "
                f"| {f(r.get('g1_val'))}{'✓' if r.get('g1_pass') else '✗'} "
                f"| {f(r.get('g2_min_rho'), 2)} | {f(r.get('g3_icc'), 2)} "
                f"| {f(r.get('g3_theta_share'), 2)} | {f(r.get('g6_max_st'), 2)} "
                f"| {f(r.get('g6_r2'), 2)} | {METRIC_NOTES.get(m, '')} |")
        lines.append("")
    lines += ["## G5 — empirical relationships", "",
              f"- water_fraction vs relief Spearman: {g5['water_vs_relief_rho']:+.2f} "
              f"(doc expects negative)",
              f"- macro-band (50-200 m) amplitude p25-p75: "
              f"{g5['macro_band_amp_p25_p75'][0]:.1f}-{g5['macro_band_amp_p25_p75'][1]:.1f} m",
              f"- beta_char p10-p90: {g5['beta_char_p10_p90'][0]:.2f}-"
              f"{g5['beta_char_p10_p90'][1]:.2f}", ""]
    counts = tab["class"].value_counts().to_dict()
    lines += [f"## Summary: {counts}", ""]
    with open(os.path.join(store.OUT, "reports", "stage1_metrics.md"), "w") as f:
        f.write("\n".join(lines))
    print(f"classes: {counts}")
    print("wrote gates.parquet + reports/stage1_metrics.md")
    return 0
