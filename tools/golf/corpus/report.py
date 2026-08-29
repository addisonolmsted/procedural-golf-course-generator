"""Render artifacts per phase (project rule: render before trusting metrics).

Phase 1: US scatter map + boundary/greens overlay cards (SVG — no DEM yet).
"""

from __future__ import annotations

import json

import numpy as np

from . import config, registry


def _svg_course(rec: dict, px: int = 260) -> str:
    ring = np.array(rec["osm"]["ring_ll"], float)
    lat0 = ring[:, 0].mean()
    scale = np.cos(np.radians(lat0))
    xy = np.stack([(ring[:, 1] - ring[:, 1].min()) * scale,
                   ring[:, 0] - ring[:, 0].min()], 1)
    span = max(xy.max(), 1e-9)
    xy = xy / span * (px - 20) + 10
    pts = " ".join(f"{x:.1f},{px - y:.1f}" for (x, y) in xy)
    dots = ""
    t = rec.get("tile") or {}
    for g in rec["greens"]:
        # greens stored in UTM; project via the ll bbox is enough for QA cards
        pass
    # greens in lat/lon are not stored; draw from utm offsets when tile known
    gl = ""
    if rec["greens"]:
        gs = np.array([g["yx_utm"] for g in rec["greens"]], float)
        # map utm -> the same 0-1 box via the ring's utm
        from . import geo
        ring_m = geo.ll_to_m(ring[:, 0], ring[:, 1], rec["utm"]["zone"])
        mn = ring_m.min(0)
        span_m = max((ring_m.max(0) - mn).max(), 1e-9)
        q = (gs - mn) / span_m * (px - 20) + 10
        gl = "".join(f'<circle cx="{x:.1f}" cy="{px - y:.1f}" r="3" '
                     f'fill="#e8d24a"/>' for (y, x) in q)
    return (f'<svg viewBox="0 0 {px} {px}" width="{px}" height="{px}">'
            f'<rect width="{px}" height="{px}" fill="var(--card)"/>'
            f'<polygon points="{pts}" fill="#3d6b3a55" stroke="#3d6b3a" '
            f'stroke-width="1.5"/>{gl}</svg>')


def phase1(path=None) -> str:
    recs = registry.all_records()
    rng = np.random.default_rng(11)
    keepers = [r for r in recs if r["status"] == "keeper"]
    unsplit = [r for r in recs if r["status"] == "multi_course_unsplit"]
    sample = list(rng.choice(len(keepers), min(20, len(keepers)),
                             replace=False)) if keepers else []
    cards = ""
    for r in [keepers[i] for i in sample] + unsplit:
        cards += (f'<figure><div class="svgbox">{_svg_course(r)}</div>'
                  f"<figcaption><b>{r['name'][:36]}</b><br>"
                  f"{r['status']} · {len(r['greens'])} greens · "
                  f"tier {r.get('fame_tier', 1)} · {r['region_tag']}"
                  f"</figcaption></figure>")
    # scatter map
    pts = "".join(
        f'<circle cx="{(r["centroid_ll"][1] + 125) * 8:.1f}" '
        f'cy="{(50 - r["centroid_ll"][0]) * 10:.1f}" r="3" '
        f'fill="{ {"keeper": "#3d8b40", "multi_course_unsplit": "#c07828", "few_greens": "#888"}[r["status"]] }" '
        f'opacity="0.75"><title>{r["name"]}</title></circle>'
        for r in recs)
    counts = {}
    for r in recs:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    by_region = {}
    for r in keepers:
        by_region[r["region_tag"]] = by_region.get(r["region_tag"], 0) + 1
    html = f"""<title>Green corpus — phase 1</title>
<style>
:root{{--bg:#f3f1ea;--ink:#1c1a16;--dim:#6d675b;--line:#dcd6c7;--card:#fffdf7}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}}}
:root[data-theme=dark]{{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}
*{{box-sizing:border-box}}body{{background:var(--bg);color:var(--ink);margin:0;
padding:2rem 1.4rem 4rem;font:15px/1.6 ui-sans-serif,system-ui}}
main{{max-width:1180px;margin:0 auto}}h1{{font-size:1.5rem}}
p{{color:var(--dim)}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(270px,1fr));gap:.9rem}}
figure{{margin:0;background:var(--card);border:1px solid var(--line);border-radius:4px;padding:.4rem}}
figcaption{{font-size:12px;padding:.3rem .2rem;color:var(--dim)}}
.svgbox{{display:flex;justify-content:center}}
.map{{background:var(--card);border:1px solid var(--line);border-radius:4px;margin:0 0 1.4rem}}
</style>
<main><h1>Green corpus — registry after phase 1</h1>
<p>{counts} · keepers by region: {by_region}</p>
<svg class="map" viewBox="200 240 400 160" width="100%" height="260">{pts}</svg>
<p>20 random keepers + every multi-course-unsplit property (excluded from
window fitting; greens still feed the scorer). Yellow dots = assigned greens.</p>
<div class="grid">{cards}</div></main>"""
    dest = path or (config.OUT / "phase1.html")
    dest.write_text(html)
    return str(dest)


def _hillshade(z, cell, wet=None, px=420):
    """Tinted hillshade, branch-standard params (course-lab::render_terrain)."""
    import io, base64
    from PIL import Image
    k = max(1, int(round(z.shape[0] / px)))
    z = z[::k, ::k].astype(np.float64)
    if wet is not None:
        wet = wet[::k, ::k][:z.shape[0], :z.shape[1]]
    cell = cell * k
    z = np.where(np.isfinite(z), z, np.nanmean(z))
    az, alt = np.radians(315.0), np.radians(45.0)
    L = (np.cos(az) * np.cos(alt), np.sin(az) * np.cos(alt), np.sin(alt))
    gy, gx = np.gradient(z * 2.4, cell)
    lam = ((-gx) * L[0] + (-gy) * L[1] + L[2]) / np.sqrt(gx**2 + gy**2 + 1.0)
    shade = np.clip(lam, 0, None) ** 1.15
    lo, hi = np.percentile(z, 2), np.percentile(z, 98)
    t = np.clip((z - lo) / max(hi - lo, 1e-6), 0, 1)
    ramp = [(0.00, (0.34, 0.50, 0.29)), (0.40, (0.62, 0.62, 0.38)),
            (0.75, (0.78, 0.68, 0.44)), (1.00, (0.87, 0.79, 0.60))]
    rgb = np.zeros(z.shape + (3,))
    for i in range(len(ramp) - 1):
        a, ca = ramp[i]; b, cb = ramp[i + 1]
        m = (t >= a) & (t <= b)
        f = np.where(m, (t - a) / (b - a), 0)
        for c in range(3):
            rgb[..., c] = np.where(m, ca[c] + (cb[c] - ca[c]) * f, rgb[..., c])
    img = np.clip((rgb * 0.85 + 0.15) * (0.25 + 0.95 * shade)[..., None] * 235,
                  0, 255)
    if wet is not None and wet.any():
        img[wet] = img[wet] * 0.3 + np.array([38, 86, 140]) * 0.7
    im = Image.fromarray(np.flipud(img.astype(np.uint8)))
    b = io.BytesIO(); im.save(b, "WEBP", quality=86)
    return ("data:image/webp;base64,"
            + base64.b64encode(b.getvalue()).decode()), k


def phase2(per_region: int = 3, path=None) -> str:
    """Georeferencing gate: greens + boundary drawn ON the hillshade.

    The check a human makes here is whether the yellow dots sit on the pale
    smooth ovals. Four transforms stack between OSM lat/lon and a tile pixel
    (UTM, origin snap, north/south row flip, shoelace centroid); if any is
    wrong the fit learns noise while reporting a healthy AUC."""
    import pathlib
    from . import geo
    _T = pathlib.Path(__file__).resolve().parents[2]
    import sys
    if str(_T / "macro_campaign") not in sys.path:
        sys.path.insert(0, str(_T / "macro_campaign"))
    from macro_campaign import cgrid

    recs = [r for r in registry.all_records()
            if r["status"] == "keeper" and r.get("tile", {}).get("path")]
    by_region = {}
    for r in sorted(recs, key=lambda r: (-r.get("fame_tier", 1),
                                         r["course_id"])):
        by_region.setdefault(r["region_tag"], []).append(r)
    cards = ""
    for reg in sorted(by_region):
        for rec in by_region[reg][:per_region]:
            t = rec["tile"]
            z, (_, _, cell) = cgrid.read_f32(pathlib.Path(t["path"]))
            wp = pathlib.Path(t["path"]).with_suffix(".water.npy")
            wet = np.load(wp) if wp.exists() else None
            uri, k = _hillshade(z, cell, wet)
            sc = cell * k                      # metres per rendered pixel
            n = int(np.ceil(z.shape[0] / k))
            ring = np.array(rec["osm"]["ring_ll"], float)
            rm = geo.ll_to_m(ring[:, 0], ring[:, 1], rec["utm"]["zone"])
            rl = rm - np.array([t["n0"], t["e0"]])
            poly = " ".join(f"{x / sc:.1f},{n - y / sc:.1f}" for (y, x) in rl)
            gs = np.array([[g["yx_utm"][0] - t["n0"], g["yx_utm"][1] - t["e0"]]
                           for g in rec["greens"]], float)
            dots = "".join(
                f'<circle cx="{x / sc:.1f}" cy="{n - y / sc:.1f}" r="3.4" '
                f'fill="none" stroke="#faf03c" stroke-width="1.6"/>'
                for (y, x) in gs)
            cards += f"""<figure>
<div class="ov" style="--n:{n}">
<img src="{uri}" width="{n}" height="{n}">
<svg viewBox="0 0 {n} {n}"><polygon points="{poly}" fill="none"
stroke="#e85d3d" stroke-width="1.6" stroke-dasharray="5 3"/>{dots}</svg></div>
<figcaption><b>{rec['name'][:34]}</b><br>{reg} · tier
{rec.get('fame_tier',1)} · {len(gs)} greens · {t['side_m']:.0f} m tile
· water {(wet.mean()*100 if wet is not None else 0):.1f}%</figcaption>
</figure>"""
    html = f"""<title>Corpus phase 2 — georeferencing gate</title>
<style>
:root{{--bg:#f3f1ea;--ink:#1c1a16;--dim:#6d675b;--line:#dcd6c7;--card:#fffdf7}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}}}
:root[data-theme=dark]{{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}
*{{box-sizing:border-box}}body{{background:var(--bg);color:var(--ink);margin:0;
padding:2rem 1.4rem 4rem;font:15px/1.6 ui-sans-serif,system-ui}}
main{{max-width:1200px;margin:0 auto}}h1{{font-size:1.5rem;margin:0 0 .3rem}}
p{{color:var(--dim);max-width:66ch}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));gap:1rem;margin-top:1.5rem}}
figure{{margin:0;background:var(--card);border:1px solid var(--line);
border-radius:4px;overflow:hidden}}
.ov{{position:relative;line-height:0}}
.ov img,.ov svg{{width:100%;height:auto;display:block}}
.ov svg{{position:absolute;inset:0}}
figcaption{{padding:.5rem .6rem;font-size:12px;color:var(--dim)}}
</style>
<main><h1>Phase 2 — do the greens land on the greens?</h1>
<p>Yellow rings are OSM green centroids projected through UTM → tile origin →
row flip; the dashed red line is the course boundary. The check: rings should
sit on the pale, smooth, faintly raised ovals. A systematic offset, mirror, or
rotation here means every downstream feature is measured at the wrong place —
and no metric would catch it.</p>
<div class="grid">{cards}</div></main>"""
    dest = path or (config.OUT / "phase2.html")
    dest.write_text(html)
    return str(dest)


def phase5(n_sample: int = 16, path=None) -> str:
    """Recall A/B overlays: real greens vs old and new pools, held-out only."""
    import pathlib
    from . import eval_recall, registry as reg
    from .features import greens_local
    import sys
    _T = pathlib.Path(__file__).resolve().parents[2]
    if str(_T / "macro_campaign") not in sys.path:
        sys.path.insert(0, str(_T / "macro_campaign"))
    from macro_campaign import cgrid

    d = json.loads((config.OUT / "recall_eval.json").read_text())
    rows = sorted(d["courses"], key=lambda r: r["old"]["w100"] - r["new"]["w100"])
    # biggest improvements, biggest regressions, plus both sandhills
    pick = (rows[:6] + rows[-4:]
            + [r for r in rows if r["region"].startswith("sandhills")])
    seen, sample = set(), []
    for r in pick:
        if r["course_id"] not in seen:
            seen.add(r["course_id"])
            sample.append(r)
        if len(sample) >= n_sample:
            break
    cards = ""
    for row in sample:
        rec = reg.load(row["course_id"])
        t = rec["tile"]
        z, (_, _, cell) = cgrid.read_f32(pathlib.Path(t["path"]))
        wp = pathlib.Path(t["path"]).with_suffix(".water.npy")
        wet = np.load(wp) if wp.exists() else None
        res = eval_recall.one_course(row["course_id"], keep_pools=True)
        uri, k = _hillshade(z, cell, wet)
        sc = cell * k
        n = int(np.ceil(z.shape[0] / k))
        wy, wx, HH, WW = row["window"]
        P = lambda ym, xm: (xm / sc, n - ym / sc)
        wr = (f'<rect x="{wx/sc:.1f}" y="{n-(wy+HH)/sc:.1f}" '
              f'width="{WW/sc:.1f}" height="{HH/sc:.1f}" fill="none" '
              f'stroke="#ffffff" stroke-width="1.4" stroke-dasharray="6 3"/>')
        marks = ""
        for (y, x) in res["pool_old"]:
            px, py = P(y, x)
            marks += (f'<circle cx="{px:.1f}" cy="{py:.1f}" r="2.0" '
                      f'fill="#f5963c" opacity="0.55"/>')
        for (y, x) in res["pool_new"]:
            px, py = P(y, x)
            marks += (f'<circle cx="{px:.1f}" cy="{py:.1f}" r="2.2" '
                      f'fill="#3cd7eb"/>')
        for (y, x) in greens_local(rec):
            px, py = P(y, x)
            marks += (f'<circle cx="{px:.1f}" cy="{py:.1f}" r="3.6" '
                      f'fill="none" stroke="#faf03c" stroke-width="1.7"/>')
        dw = row["new"]["w100"] - row["old"]["w100"]
        cards += f"""<figure>
<div class="ov"><img src="{uri}" width="{n}" height="{n}">
<svg viewBox="0 0 {n} {n}">{wr}{marks}</svg></div>
<figcaption><b>{rec['name'][:34]}</b> · {row['region']} ·
{row['n_in']} greens in window<br>
within-100 m: old {row['old']['w100']:.0f}% → new
<b>{row['new']['w100']:.0f}%</b> ({dw:+.0f}) · median
{row['old']['med']:.0f} → {row['new']['med']:.0f} m</figcaption></figure>"""
    a = d["agg"]
    html = f"""<title>Corpus phase 5 — fitted scorer vs hand scorer</title>
<style>
:root{{--bg:#f3f1ea;--ink:#1c1a16;--dim:#6d675b;--line:#dcd6c7;--card:#fffdf7}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}}}
:root[data-theme=dark]{{--bg:#12120e;--ink:#e9e5da;--dim:#948d7f;--line:#2f2b23;--card:#1b1a15}}
*{{box-sizing:border-box}}body{{background:var(--bg);color:var(--ink);margin:0;
padding:2rem 1.4rem 4rem;font:15px/1.6 ui-sans-serif,system-ui}}
main{{max-width:1200px;margin:0 auto}}h1{{font-size:1.5rem;margin:0 0 .3rem}}
p{{color:var(--dim);max-width:70ch}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(330px,1fr));gap:1rem;margin-top:1.4rem}}
figure{{margin:0;background:var(--card);border:1px solid var(--line);
border-radius:4px;overflow:hidden}}
.ov{{position:relative;line-height:0}}
.ov img,.ov svg{{width:100%;height:auto;display:block}}
.ov svg{{position:absolute;inset:0}}
figcaption{{padding:.5rem .6rem;font-size:12px;color:var(--dim)}}
b{{color:var(--ink)}}
</style>
<main><h1>Fitted scorer vs hand scorer — held-out courses</h1>
<p>Yellow rings = real greens · cyan = fitted-scorer pool · faint orange =
old hand-scorer pool · dashed white = fitted window. Medians over all
{d['n']} held-out courses: within 60 m {a['old']['w60']:.0f}% →
<b>{a['new']['w60']:.0f}%</b>, within 100 m {a['old']['w100']:.0f}% →
<b>{a['new']['w100']:.0f}%</b>, median distance {a['old']['med']:.0f} →
<b>{a['new']['med']:.0f} m</b>. Cards below are the biggest movers in both
directions plus every sandhills course.</p>
<div class="grid">{cards}</div></main>"""
    dest = path or (config.OUT / "phase5.html")
    dest.write_text(html)
    return str(dest)
