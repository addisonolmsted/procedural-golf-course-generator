"""Step-2 creek terrain sheet: the creek ON THE GROUND, three ways.

  python3 tools/aeolian/creek_terrain_sheet.py <new_dir> <trainskirt_dir> <old_dir> <out.html> <seed>... [--prefix tex_]

Per seed: a whole-tile hillshade of the NEW build with the creek, then two
600 m crops along the creek rendered for all three variants side by side
(new planform + channel carve | new planform + OLD skirt carve | shipped sine
+ skirt), and three cross-valley transects per crop (elevation vs distance,
+-40 m) -- the number that catches a skirt: the cut must return to the
background within a few metres of the wet edge. Prints the creek's wet-body
count (must be 1) and the >=35-degree double-crossing rate + amplitude p95
from creek_crossings.py.

Tinted hillshade is the branch standard: `hillshade_rgb` from
tools/golf/siting_sheet.py, imported, not copied.
"""
import sys, os, io, base64, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE)); sys.path.insert(0, str(HERE.parent / "golf")); sys.path.insert(0, str(HERE.parent / "macro_campaign"))
from macro_campaign import cgrid
from siting_sheet import hillshade_rgb, to_uri
import creek_crossings as cc
from scipy import ndimage
from PIL import Image

def load(d, seed, prefix):
    z, (_, _, c) = cgrid.read_f32(pathlib.Path(d) / f"{prefix}{seed}.cgrid")
    w, _ = cgrid.read_f32(pathlib.Path(d) / f"{prefix}{seed}.water.cgrid")
    return z, np.isfinite(w), c

def creek_stations(wet, c, fracs=(0.33, 0.66)):
    """Cells at given fractions along the creek's principal axis."""
    ys, xs = np.where(wet)
    P = np.stack([ys, xs], 1).astype(float)
    u = np.linalg.svd(P - P.mean(0), full_matrices=False)[2][0]
    t = (P - P.mean(0)) @ u
    o = np.argsort(t)
    return [(int(ys[o[int(f * (len(o) - 1))]]), int(xs[o[int(f * (len(o) - 1))]])) for f in fracs]

def crop(z, wet, cy, cx, half):
    y0, y1 = max(cy - half, 0), min(cy + half, z.shape[0]); x0, x1 = max(cx - half, 0), min(cx + half, z.shape[1])
    return z[y0:y1, x0:x1], wet[y0:y1, x0:x1], (y0, x0)

def transects(z, wet, c, cy, cx, n=3, half_m=40.0, step_m=60.0):
    """n transects perpendicular to the local creek axis, spaced step_m apart
    along it, centred on the nearest creek cell to each station."""
    out = []
    ang, ys, xs = cc.local_axis_angles(wet, c)
    P = np.stack([ys, xs], 1)
    d0 = np.hypot(ys - cy, xs - cx); k0 = int(np.argmin(d0))
    a0 = ang[k0]
    for j in range(n):
        s = (j - (n - 1) / 2) * step_m / c
        ty, tx = ys[k0] + np.sin(a0) * s, xs[k0] + np.cos(a0) * s
        d = np.hypot(ys - ty, xs - tx); k = int(np.argmin(d))
        a = ang[k] + np.pi / 2
        dd = np.arange(-half_m, half_m + 1e-9, c / 2)
        yy = ys[k] + np.sin(a) * dd / c; xx = xs[k] + np.cos(a) * dd / c
        zz = ndimage.map_coordinates(z, [yy, xx], order=1, mode="nearest")
        ww = ndimage.map_coordinates(wet.astype(float), [yy, xx], order=0, mode="nearest") > 0.5
        out.append((dd, zz, ww))
    return out

def main():
    a = [x for x in sys.argv[1:] if not x.startswith("--")]
    prefix = "tex_"
    if "--prefix" in sys.argv: prefix = sys.argv[sys.argv.index("--prefix") + 1]; a = [x for x in a if x != prefix]
    d_new, d_ts, d_old, out_html = a[:4]; seeds = a[4:]
    labels = ["NEW: bend train + channel carve", "bend train + OLD skirt carve", "SHIPPED: sine + skirt"]
    if "--labels" in sys.argv:
        labels = sys.argv[sys.argv.index("--labels") + 1].split("|"); seeds = [x for x in seeds if x != sys.argv[sys.argv.index("--labels") + 1]]
    real = None
    if "--real" in sys.argv:
        real = sys.argv[sys.argv.index("--real") + 1]; seeds = [x for x in seeds if x != real]
    import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
    variants = [(labels[0], d_new), (labels[1], d_ts), (labels[2], d_old)]
    html = ["<title>Creek Terrain Sheet</title><style> :root{--bg:#f6f4ee;--ink:#22231f;--muted:#5f5e57;--rule:#d9d5c9;--accent:#8a3b2a;--panel:#fffdf8} @media (prefers-color-scheme: dark){:root:not([data-theme=\"light\"]){--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}} :root[data-theme=\"dark\"]{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420} body{background:var(--bg);color:var(--ink);font-family:\"IBM Plex Sans\",system-ui,sans-serif;margin:0;padding:20px 24px;line-height:1.45} h2{font-size:1.45rem;margin:0 0 6px;text-wrap:balance}h3{font-size:1.05rem;margin:26px 0 4px;color:var(--accent)} p{max-width:70ch;color:var(--muted);margin:4px 0 12px}.cap{font-size:11.5px;color:var(--muted);margin:2px 0 8px;font-family:\"IBM Plex Mono\",ui-monospace,monospace} .row{display:flex;gap:10px;align-items:flex-start;flex-wrap:wrap}img{max-width:100%;background:var(--panel)} table{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums}td,th{border:1px solid var(--rule);padding:3px 8px;text-align:right}td:first-child,th:first-child{text-align:left} .wrap{overflow-x:auto} </style> <link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap\">",
            "<h2>Creek on the ground, step 2: bend train + channel carve vs the ablations</h2>",
            "<p>Per seed: whole tile (first variant), then two 600 m crops along the creek for the three variants, and cross-valley transects (±40 m) through each crop.</p>"]
    table = []
    for seed in seeds:
        try:
            data = {lab: load(d, seed, prefix) for lab, d in variants}
        except Exception as e:
            html.append(f"<h3>seed {seed}</h3><p>missing: {e}</p>"); continue
        z, wet, c = data[variants[0][0]]
        mu, m, _ = cc.creek_mask(pathlib.Path(d_new) / f"{prefix}{seed}.water.cgrid")
        if m is None:
            html.append(f"<h3>seed {seed}</h3><p>no creek body found</p>"); continue
        r = cc.multi_cross_rate(m, c); amp = cc.amplitude(m, c); bodies = cc.creek_bodies(mu)
        ro = None
        _, mo, _ = cc.creek_mask(pathlib.Path(d_old) / f"{prefix}{seed}.water.cgrid")
        if mo is not None: ro = cc.multi_cross_rate(mo, c)
        table.append((seed, r, amp, bodies, ro))
        html.append(f"<h3>seed {seed}</h3><div class=cap>NEW creek: double-crossing(≥35°) {r:.1f}% · amplitude p95 {amp:.1f} m · wet bodies (creek + its ponds) {bodies}"
                    + (f" · shipped sine double-crossing {ro:.1f}%" if ro is not None else "") + "</div>")
        # overview: whole tile at 1/3 scale
        ov = hillshade_rgb(z, c, wet)
        im = Image.fromarray(np.flipud(ov)).resize((ov.shape[1] // 3, ov.shape[0] // 3), Image.LANCZOS)
        b = io.BytesIO(); im.save(b, "WEBP", quality=80)
        html.append(f'<div class=row><div><img src="data:image/webp;base64,{base64.b64encode(b.getvalue()).decode()}"><div class=cap>whole tile, new build (3 km)</div></div></div>')
        half = int(300 / c)
        for (cy, cx) in creek_stations(m, c):
            html.append('<div class=row>')
            for lab, _ in variants:
                zz, ww, _ = crop(*data[lab][:2], cy, cx, half)
                img = hillshade_rgb(zz, c, ww)
                html.append(f'<div><img src="{to_uri(img, upscale=2)}" width=600><div class=cap>{lab} — 600 m crop</div></div>')
            # transects
            fig, axes = plt.subplots(1, 3, figsize=(9.0, 2.6), dpi=100, sharey=True)
            for lab, _ in variants:
                zz, ww, cc_ = data[lab]
                # station in the variant's own creek (nearest wet cell)
                _, mm, _ = cc.creek_mask(pathlib.Path(dict(variants)[lab]) / f"{prefix}{seed}.water.cgrid")
                if mm is None: continue
                for k, (dd, zt, wt) in enumerate(transects(zz, mm, cc_, cy, cx)):
                    axes[k].plot(dd, zt - zt[len(zt) // 2], lw=1.2, label=lab if k == 0 else None)
                    if wt.any(): axes[k].plot(dd[wt], (zt - zt[len(zt) // 2])[wt], ".", ms=3, color="#2a6ebb")
            for k in range(3):
                axes[k].set_xlabel("m across creek", fontsize=8); axes[k].tick_params(labelsize=7); axes[k].grid(True, color="#eee")
                axes[k].axvspan(-2.5, 2.5, color="#dbe8f5", alpha=0.5)
            axes[0].set_ylabel("m rel. to creek", fontsize=8); axes[0].legend(fontsize=6.5, loc="upper left")
            fig.tight_layout()
            b = io.BytesIO(); fig.savefig(b, format="png"); plt.close(fig)
            html.append(f'<div><img src="data:image/png;base64,{base64.b64encode(b.getvalue()).decode()}" width=900><div class=cap>three transects across this crop, all variants (blue dots = wet)</div></div>')
            html.append('</div>')
    html.append("<h3>Routing metric, all seeds</h3><table><tr><th>seed</th><th>NEW double-crossing ≥35° %</th><th>NEW amplitude p95 m</th><th>NEW wet bodies</th><th>shipped sine double-crossing %</th></tr>")
    for seed, r, amp, bodies, ro in table:
        html.append(f"<tr><td>{seed}</td><td>{r:.1f}</td><td>{amp:.1f}</td><td>{bodies}</td><td>{'' if ro is None else f'{ro:.1f}'}</td></tr>")
    rs = [t[1] for t in table if t[1] is not None]; ros = [t[4] for t in table if t[4] is not None]
    if rs: html.append(f"<tr><td><b>median</b></td><td><b>{np.median(rs):.1f}</b></td><td><b>{np.median([t[2] for t in table]):.1f}</b></td><td></td><td><b>{np.median(ros):.1f}</b></td></tr>")
    html.append("</table>")
    if real:
        html.append(f"<h3>REAL corpus tile {real}: resolved creek stations, 600 m crops at the same render, with transects</h3>")
        html.append("<p>Yellow dots mark stations where the 2 m DTM resolves a channel (tools/aeolian/creek_sections.py). "
                    "Transects use the same ±40 m window as ours above.</p>")
        import creek_sections as cs
        from macro_campaign import cgrid as cg
        import json as _json
        z, (_, _, c) = cg.read_f32(cs.TILE_ROOT / "sandhills_nc" / f"{real}.cgrid"); z = z.astype(float)
        meta = _json.load(open(cs.TILE_ROOT / "sandhills_nc" / f"{real}.json")); e0, n0 = meta["easting0"], meta["northing0"]
        ways = cs.stream_ways(); ext = z.shape[0] * c
        rows = []
        for wid, (E, N) in ways.items():
            if not (E.min() < e0 + ext and E.max() > e0 and N.min() < n0 + ext and N.max() > n0): continue
            rows += [r for r in cs.way_transects(z, e0, n0, c, E, N) if r["resolved"]]
        rng = np.random.default_rng(5)
        picks = [rows[i] for i in rng.choice(len(rows), min(4, len(rows)), replace=False)] if rows else []
        html.append('<div class=row>')
        for r in picks:
            cx, cy = int(r["x"] / c), int(r["y"] / c); half = int(300 / c)
            x0 = int(np.clip(cx - half, 0, z.shape[1] - 2*half)); y0 = int(np.clip(cy - half, 0, z.shape[0] - 2*half))
            zz = z[y0:y0+2*half, x0:x0+2*half]
            img = hillshade_rgb(zz, c)
            html.append(f'<div><img src="{to_uri(img, upscale=2)}" width=600><div class=cap>{real} · station ({r["x"]:.0f}, {r["y"]:.0f}) m · rim {r["depth"]:.2f} m · w {0.5*(r["w_l"]+r["w_r"]):.0f} m</div></div>')
        html.append('</div>')
        if rows:
            fig, ax = plt.subplots(1, 1, figsize=(9.0, 2.8), dpi=100)
            S = np.stack([q["S"] for q in rows])
            ax.fill_between(cs.D, np.percentile(S, 25, 0), np.percentile(S, 75, 0), color="#1f4e79", alpha=0.25, lw=0)
            ax.plot(cs.D, np.median(S, 0), color="#1f4e79", lw=2, label=f"real tile {real}, {len(rows)} resolved transects")
            for r in picks: ax.plot(cs.D, r["S"], lw=0.8, alpha=0.7)
            ax.set_xlabel("m across creek"); ax.set_ylabel("m rel. to creek"); ax.grid(True, color="#eee"); ax.legend(fontsize=7); ax.set_ylim(-0.5, 3)
            fig.tight_layout(); b = io.BytesIO(); fig.savefig(b, format="png"); plt.close(fig)
            html.append(f'<img src="data:image/png;base64,{base64.b64encode(b.getvalue()).decode()}" width=900>')
    open(out_html, "w").write("\n".join(html))
    print(f"wrote {out_html}")
    for seed, r, amp, bodies, ro in table:
        print(f"  {seed}: new xcross35 {r:.1f}%  amp {amp:.1f} m  bodies {bodies}   old xcross35 {ro}")

if __name__ == "__main__":
    main()
