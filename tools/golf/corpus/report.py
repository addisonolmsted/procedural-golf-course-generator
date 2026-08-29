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
