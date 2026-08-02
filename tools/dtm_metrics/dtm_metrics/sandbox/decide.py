"""The parameter decision table — KEEP / MERGE / DROP / METRIC-GAP /
DEFER-TO-STAGE-5 — from the three evidence lines (influence, identifiability,
coverage), plus the recommended Stage-4 parameter set with bounds.
"""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from .. import store
from . import params as P

SBOX = os.path.join(store.OUT, "sandbox")

# parameters whose metric signatures overlap the expected Stage-5 erosion
# effects; flagged for the erosion-sensitivity addendum before 7b
EROSION_OVERLAP = {"ridged_mix": "fluvial sharpening/creases",
                   "redistribution": "graded deposition hypsometry"}


def run() -> int:
    st = pd.read_csv(os.path.join(SBOX, "sobol_st.csv"), index_col=0)
    rec = pd.read_csv(os.path.join(SBOX, "recovery_summary.csv"),
                      index_col=0, header=[0, 1])
    cov = pd.read_csv(os.path.join(SBOX, "coverage_relevance.csv"), index_col=0)

    dummy_ceiling = float(st.get("dummy", pd.Series([0.0])).max())
    infl_floor = max(0.05, 2.0 * dummy_ceiling)
    # the coverage evidence line has its own noise floor: the inert dummy's
    # apparent reach-drop (emulated-percentile jitter). Critical threshold
    # self-calibrates above it.
    dummy_cov = float(cov.loc["dummy", "worst_reach_drop"]) if "dummy" in cov.index else 0.0
    cov_floor = max(0.05, 2.0 * dummy_cov)

    rows = []
    for pname in P.NAMES:
        if pname == "dummy":
            continue
        stmax = float(st[pname].max()) if pname in st.columns else 0.0
        st_best = st[pname].idxmax() if pname in st.columns and stmax > 0 else "-"
        # Recovery evidence comes from the mode DESIGNED to identify the
        # param: the flat campaign for base params (41 emulated metrics);
        # the skeleton campaign only for the modulation trio (its 5 s3/s3t
        # metrics cannot identify base params — that's not a failure of the
        # base params, just the wrong instrument).
        modes = (("skel",) if pname in ("floor_damp", "slope_gain", "grain_align")
                 else ("flat",))
        rho = np.nan
        for mode in modes:
            try:
                v = rec.loc[pname, (mode, "spearman")]
                if np.isfinite(v):
                    rho = float(v)
                    break
            except KeyError:
                continue
        drop = float(cov.loc[pname, "worst_reach_drop"]) if pname in cov.index else 0.0

        influential = stmax >= infl_floor
        recoverable = np.isfinite(rho) and rho >= 0.6
        partial = np.isfinite(rho) and 0.3 <= rho < 0.6
        coverage_critical = drop >= cov_floor

        if influential and recoverable:
            verdict = "KEEP"
        elif influential and partial:
            verdict = "MERGE-OR-CONSTRAIN"
        elif influential and not recoverable:
            verdict = "METRIC-GAP"
        elif coverage_critical:
            verdict = "KEEP (coverage)"
        else:
            verdict = "DROP"
        note = EROSION_OVERLAP.get(pname, "")
        if note:
            verdict += " +DEFER-CHECK-S5"
        rows.append({"param": pname, "max_ST": round(stmax, 3),
                     "best_metric": st_best,
                     "recovery_rho": round(rho, 3) if np.isfinite(rho) else np.nan,
                     "coverage_drop": round(drop, 3),
                     "verdict": verdict, "note": note})
    table = pd.DataFrame(rows).set_index("param")
    table.to_csv(os.path.join(SBOX, "decision_table.csv"))

    kept = [p for p, r in table.iterrows() if r["verdict"].startswith("KEEP")]
    bounds = {}
    for _name, lo, hi, tr, is_int, _t in [(p[0], p[1], p[2], p[3], p[4], p[5])
                                          for p in P.PARAMS if p[0] in kept]:
        bounds[_name] = {"lo": lo, "hi": hi, "transform": tr, "int": bool(is_int)}
    with open(os.path.join(SBOX, "stage4_recommended_params.json"), "w") as f:
        json.dump({"schema": "stage4-noise-params-v1",
                   "influence_floor": infl_floor,
                   "params": bounds,
                   "erosion_addendum": sorted(EROSION_OVERLAP),
                   }, f, indent=1, sort_keys=True)

    def md_table(df):
        cols = list(df.columns)
        out = ["| param | " + " | ".join(cols) + " |",
               "|---" * (len(cols) + 1) + "|"]
        for idx, row in df.iterrows():
            out.append("| " + str(idx) + " | "
                       + " | ".join(str(row[c]) for c in cols) + " |")
        return "\n".join(out)

    md = ["# Parameter assessment — decision table", "",
          f"Influence floor: max(0.05, 2 x dummy ST {dummy_ceiling:.4f}) = "
          f"{infl_floor:.4f}; coverage floor: max(0.05, 2 x dummy drop "
          f"{dummy_cov:.3f}) = {cov_floor:.3f}", "",
          md_table(table), "",
          f"**Recommended Stage-4 set ({len(kept)}):** {', '.join(kept)}", "",
          "Erosion-sensitivity addendum (pre-7b): rerun emulate/identify/"
          "coverage with the Stage-5 erosion knobs appended; params flagged "
          "+DEFER-CHECK-S5 must be re-checked for signature overlap.", ""]
    rp = os.path.join(store.OUT, "reports")
    os.makedirs(rp, exist_ok=True)
    with open(os.path.join(rp, "parameter_assessment.md"), "w") as f:
        f.write("\n".join(md))
    print(table.to_string())
    print(f"\nrecommended Stage-4 set ({len(kept)}): {kept}")
    return 0
