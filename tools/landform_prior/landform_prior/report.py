"""Calibration report: scatter+fit figures (w-A, S-A, lambda-W, angles) and
per-archetype tables with the effective-n audit -> out/report/index.html."""

from __future__ import annotations

import json
import os

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

from . import OUT  # noqa: E402
from .fit import ARTIFICIAL_MAX, load_inputs  # noqa: E402

FIG_KW = dict(dpi=110, bbox_inches="tight")
ARCH_COLORS = ["#1f77b4", "#d62728", "#2ca02c", "#9467bd", "#8c564b"]


def _archetype_of(tile_resp: dict, tiles) -> np.ndarray:
    return np.array([int(np.argmax(tile_resp.get(t, [1.0]))) for t in tiles])


def _powerline(ax, b0: float, b1: float, x0: float, x1: float, **kw):
    xs = np.geomspace(x0, x1, 64)
    ax.plot(xs, np.exp(b0) * xs ** b1, **kw)


def run() -> int:
    prior = json.load(open(os.path.join(OUT, "landform_prior.json")))
    params, nets, branches, tile_resp, arch = load_inputs()
    rep = os.path.join(OUT, "report")
    os.makedirs(rep, exist_ok=True)

    br_art = branches.set_index(["tile", "branch"])["artificial_frac"]
    val = params[(params.kind == "valley") & params.ok
                 & (params.source == "nhd")].copy()
    val = val.join(br_art, on=["tile", "branch"])
    val = val[val["artificial_frac"].fillna(0.0) <= ARTIFICIAL_MAX]
    natural = branches[branches.artificial_frac.fillna(0.0)
                       <= ARTIFICIAL_MAX]
    meand = natural[np.isfinite(natural.wavelength_mult)]
    names = arch["names"]
    short = [n.split("-")[0] for n in names]
    ph = prior["pooled"]["hydraulic"]

    va = _archetype_of(tile_resp, val.tile)

    # -- fig 1: floor halfwidth vs A ------------------------------------
    fig, ax = plt.subplots(figsize=(7, 5))
    for j in range(len(names)):
        m = va == j
        ax.scatter(val.area_km2[m], val.hw[m], s=8, alpha=0.35,
                   color=ARCH_COLORS[j], label=short[j], lw=0)
    _powerline(ax, ph["floor_halfwidth"]["b0"], ph["floor_halfwidth"]["b1"],
               1e-2, 3e3, color="k", lw=2,
               label=f"pooled hw={np.exp(ph['floor_halfwidth']['b0']):.1f}"
                     f"·A^{ph['floor_halfwidth']['b1']:.2f}")
    for j, a in enumerate(prior["archetypes"]):
        r = a["hydraulic"]["floor_halfwidth"]
        _powerline(ax, r["b0"], r["b1"], 1e-2, 3e3, color=ARCH_COLORS[j],
                   lw=1, ls="--")
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("drainage area A (km²)")
    ax.set_ylabel("valley floor halfwidth (m)")
    ax.set_title("w–A: floor halfwidth vs drainage area (876 reaches)")
    ax.legend(fontsize=8)
    fig.savefig(os.path.join(rep, "w_A.png"), **FIG_KW)
    plt.close(fig)

    # -- fig 2: incision vs A -------------------------------------------
    fig, ax = plt.subplots(figsize=(7, 5))
    for j in range(len(names)):
        m = va == j
        ax.scatter(val.area_km2[m], val.incision[m], s=8, alpha=0.35,
                   color=ARCH_COLORS[j], label=short[j], lw=0)
    _powerline(ax, ph["incision"]["b0"], ph["incision"]["b1"], 1e-2, 3e3,
               color="k", lw=2,
               label=f"pooled inc={np.exp(ph['incision']['b0']):.1f}"
                     f"·A^{ph['incision']['b1']:.2f}")
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("drainage area A (km²)")
    ax.set_ylabel("incision (m)")
    ax.set_title("incision vs A — depth is set by local relief, "
                 "nearly flat in A")
    ax.legend(fontsize=8)
    fig.savefig(os.path.join(rep, "inc_A.png"), **FIG_KW)
    plt.close(fig)

    # -- fig 3: slope-area ----------------------------------------------
    ba = _archetype_of(tile_resp, natural.tile)
    fig, ax = plt.subplots(figsize=(7, 5))
    sa = natural[np.isfinite(natural.fall_raw) & (natural.fall_raw > 0)
                 & np.isfinite(natural.area_mouth_km2)
                 & (natural.area_mouth_km2 > 0)]
    saa = ba[np.asarray(natural.index.isin(sa.index))]
    for j in range(len(names)):
        m = saa == j
        ax.scatter(sa.area_mouth_km2[m], sa.fall_raw[m], s=8, alpha=0.35,
                   color=ARCH_COLORS[j], label=short[j], lw=0)
    fg = ph["fall_gradient"]
    _powerline(ax, fg["ln_k"], -fg["theta"], 1e-2, 3e3, color="k", lw=2,
               label=f"pooled S={np.exp(fg['ln_k']):.4f}"
                     f"·A^−{fg['theta']:.2f}")
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("drainage area at mouth (km²)")
    ax.set_ylabel("longitudinal fall gradient")
    ax.set_title(f"S–A law ({len(sa)} branches)")
    ax.legend(fontsize=8)
    fig.savefig(os.path.join(rep, "S_A.png"), **FIG_KW)
    plt.close(fig)

    # -- fig 4: meander wavelength vs top width -------------------------
    ma = _archetype_of(tile_resp, meand.tile)
    fig, ax = plt.subplots(figsize=(7, 5))
    for j in range(len(names)):
        m = ma == j
        ax.scatter(meand.top_width_m[m], meand.meander_lambda_m[m], s=10,
                   alpha=0.5, color=ARCH_COLORS[j], label=short[j], lw=0)
    q50 = prior["pooled"]["meander"]["wavelength_mult_deciles"][5]
    xs = np.geomspace(5, 500, 32)
    ax.plot(xs, q50 * xs, "k-", lw=2, label=f"λ = {q50:.1f}·W (q50)")
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("valley top width W (m)")
    ax.set_ylabel("meander wavelength λ (m)")
    ax.set_title(f"λ–W ({len(meand)} branches with a meander fit)")
    ax.legend(fontsize=8)
    fig.savefig(os.path.join(rep, "lam_W.png"), **FIG_KW)
    plt.close(fig)

    # -- fig 5: junction angles -----------------------------------------
    ja = natural[np.isfinite(natural.junction_angle_deg)]
    fig, ax = plt.subplots(figsize=(7, 4))
    ax.hist(ja.junction_angle_deg, bins=36, range=(0, 180),
            color="#4c78a8", alpha=0.85)
    for q, v in zip((10, 50, 90),
                    (prior["pooled"]["network"]
                     ["junction_angle_deg_deciles"][i] for i in (1, 5, 9))):
        ax.axvline(v, color="k", ls="--", lw=1)
        ax.text(v, ax.get_ylim()[1] * 0.92, f"q{q}={v:.0f}°", rotation=90,
                fontsize=7, ha="right")
    ax.set_xlabel("junction angle (deg)")
    ax.set_ylabel("junctions")
    ax.set_title(f"tributary junction angles ({len(ja)})")
    fig.savefig(os.path.join(rep, "angles.png"), **FIG_KW)
    plt.close(fig)

    # -- tables ----------------------------------------------------------
    def row(a: dict, label: str) -> str:
        h, n, m = a["hydraulic"], a["network"], a["meander"]
        cell = [
            label,
            f"{a['n_eff']['reaches']:.0f}/{a['n_eff']['branches']:.0f}"
            f"/{a['n_eff']['tiles']:.0f}",
            f"{np.exp(h['floor_halfwidth']['b0']):.1f}·A^"
            f"{h['floor_halfwidth']['b1']:.2f}",
            f"{np.exp(h['incision']['b0']):.1f}·A^{h['incision']['b1']:.2f}",
            f"{np.exp(h['fall_gradient']['ln_k']):.4f}·A^−"
            f"{h['fall_gradient']['theta']:.2f}",
            f"{n['drainage_density_deciles'][5]:.2f}",
            f"{n['junction_angle_deg_deciles'][5]:.0f}°",
            f"{m['fit_frac']:.2f} / {m['sinuosity_deciles'][5]:.2f}",
            f"{a['ridges']['prominence_deciles'][5]:.1f} m @ "
            f"{a['ridges']['spacing_m_deciles'][5]:.0f} m",
            f"{a['bluffs']['height_deciles'][5]:.1f} m @ "
            f"{a['bluffs']['face_grad_deciles'][5]:.2f}",
            f"{a['bowls']['depth_deciles'][5]:.1f} m r"
            f"{a['bowls']['radius_deciles'][5]:.0f}",
            f"{a['bowls']['lake_frac']:.2f}",
            a.get("copula", {}).get("source", "-"),
        ]
        return "<tr>" + "".join(f"<td>{c}</td>" for c in cell) + "</tr>"

    heads = ["archetype", "n_eff r/b/t", "hw(A)", "inc(A)", "S(A)",
             "dd q50", "angle q50", "meander frac/sinu", "ridge q50",
             "bluff q50", "bowl q50", "lake", "copula"]
    tbl = ("<table><tr>"
           + "".join(f"<th>{h}</th>" for h in heads) + "</tr>"
           + row(prior["pooled"], "<b>pooled</b>")
           + "".join(row(a, f"{short[j]} (w={a['weight']:.2f})")
                     for j, a in enumerate(prior["archetypes"]))
           + "</table>")

    src = prior["source"]
    html = f"""<!doctype html><meta charset="utf-8">
<title>landform_prior v{prior['prior_version']} calibration report</title>
<style>
 body {{ font: 14px/1.5 system-ui, sans-serif; margin: 24px; max-width: 1080px; }}
 table {{ border-collapse: collapse; font-size: 12px; }}
 td, th {{ border: 1px solid #ccc; padding: 3px 8px; text-align: left; }}
 th {{ background: #f0f0f0; }}
 img {{ max-width: 100%; border: 1px solid #ddd; margin: 6px 0; }}
 .note {{ background: #fff8e1; border: 1px solid #e6d28a; padding: 8px 12px; }}
</style>
<h1>landform_prior v{prior['prior_version']}</h1>
<p>Fit from {src['n_tiles']} tiles — {src['n_valley_reaches']} valley
reaches, {src['n_branches']} natural branches ({src['n_junctions']}
junctions, {src['n_meander']} meander fits), {src['n_ridges']} ridges,
{src['n_bluffs']} bluffs, {src['n_bowls']} bowls. Branch filter:
artificial_frac ≤ {ARTIFICIAL_MAX}; valley reaches: ok ∧ source=nhd.
Shrinkage λ=n/(n+25) toward pooled per table (Kish effective n).</p>
{tbl}
<div class="note"><b>Calibration findings (data, not bugs):</b>
incision is nearly flat in A (A^{ph['incision']['b1']:.2f}) — valley depth
is set by local relief, not upstream area, inside 3 km windows; top width
inherits that flatness. floor_round piles at the fitter's 20 m bound for
~half of reaches (censored — the sampler treats q≥50 draws as "very round").
side_balance was not recorded in v0.1 → Beta(1,1) documented default.</div>
<h2>w–A</h2><img src="w_A.png">
<h2>incision–A</h2><img src="inc_A.png">
<h2>S–A</h2><img src="S_A.png">
<h2>λ–W</h2><img src="lam_W.png">
<h2>Junction angles</h2><img src="angles.png">
"""
    with open(os.path.join(rep, "index.html"), "w") as f:
        f.write(html)
    print(f"report -> {os.path.join(rep, 'index.html')}")
    return 0
