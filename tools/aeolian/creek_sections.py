"""Real creek CROSS-SECTIONS from the corpus, measured and shown.

MEASUREMENT ONLY. Joins the cached OSM `waterway=stream` ways to the 2 m
sandhills_nc DTM tiles (kept AND excluded -- the excluded tiles are where the
mapped creeks are) and stacks perpendicular transects across them.

  python3 tools/aeolian/creek_sections.py --selftest
  python3 tools/aeolian/creek_sections.py measure                     # asset + step-0 sheet, offline
  python3 tools/aeolian/creek_sections.py sheet <gen_dir> <seed>...   # sheet only, with our cut overlaid

Pipeline per way: stations every 10 m on the 30 m-smoothed line (the planform
instruments' convention, so digitising is not measured); a +-40 m transect at
1 m along the normal (bilinear); RE-CENTRE on the median-3 minimum within
+-12 m (OSM lines sit 5-15 m off the true channel); DETREND by the line
through 25 <= |d| <= 40 m so the section is relief relative to the local
floor trend, zero at the channel; SCREEN steps > 1 m/m and rims > 4 m
(roads, culverts, ponds); RESOLVED = S(+-10 m) >= 0.15 m, i.e. a channel the
2 m DTM actually shows. `resolved_frac` is how often a mapped creek is
visible at all.

Writes assets/sandhills_creek_sections.txt (CSEC1) and
out/creek/step0_real_sections.html.
"""
import sys, json, hashlib, pathlib, io, base64
import numpy as np
from scipy import ndimage
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
for p in (HERE, HERE.parent / "golf", HERE.parent / "macro_campaign"):
    sys.path.insert(0, str(p))
import creek_measure as cm

HALF_M, STEP_M, EXPORT_HALF = 40, 1.0, 24
STATION_M, RECENTER_M = 10.0, 12
DETREND = (25, 40)
RESOLVE_MIN, STEP_MAX, RIM_MAX = 0.15, 1.0, 4.0
REACH_N = 20
TILE_ROOT = ROOT / "tools/macro_campaign/out/tiles"
CACHE = ROOT / "tools/golf/corpus/out/fetch_cache/overpass"
QUERY = '[out:json][timeout:180];(way["waterway"="{kind}"]({b}););out geom;'

# Which corpus a mode's creeks are measured against. The Carolina creek is a
# blackwater stream on an integrated network; the Nebraska one is the
# allogenic river on a canyon floor -- different water, different corpus, so
# each mode gets its own pack.
REGIONS = {
    "nc": dict(tiles=["sandhills_nc"], zone=17, box="sandhills_nc", kinds=["stream"],
               asset="assets/sandhills_creek_sections.txt", tag=""),
    "ne": dict(tiles=["sandhills_river", "sandhills"], zone=14, box="sandhills_ne",
               kinds=["stream", "river"], asset="assets/sandhills_creek_sections_ne.txt", tag="_ne"),
}
REG = REGIONS["nc"]
ASSET = ROOT / REGIONS["nc"]["asset"]

def use_region(key):
    """Point the module at one region's tiles, UTM zone and asset."""
    global REG, ASSET
    REG = REGIONS[key]
    ASSET = ROOT / REG["asset"]
D = np.arange(-HALF_M, HALF_M + 1e-9, STEP_M)          # 81 samples
C0 = len(D) // 2

# --- inputs ---------------------------------------------------------------------
def stream_ways():
    """Every mapped waterway of the region's kinds, deduped, in UTM metres.
    Served from the Overpass disk cache; a miss fetches once and caches."""
    from corpus import config, net, geo
    box = [b for b in config.SEARCH_BOXES if b[0] == REG["box"]][0]
    _, la, lo, h = box
    quads = ((la-h/2, lo-h/2), (la-h/2, lo+h/2), (la+h/2, lo-h/2), (la+h/2, lo+h/2))
    ways = {}
    for kind in REG["kinds"]:
        for (qa, qo) in quads:
            b = f"{qa-h/2},{qo-h/2},{qa+h/2},{qo+h/2}"
            q = QUERY.format(kind=kind, b=b)
            p = CACHE / f"{hashlib.sha256(q.encode()).hexdigest()[:24]}.json"
            if not p.exists():
                print(f"  [fetch] {kind} {b}", flush=True)
                net.overpass(q)
            assert p.exists(), f"overpass cache missing for {kind} {b}: {p}"
            for el in json.load(open(p)).get("elements", []):
                g = el.get("geometry")
                if g and len(g) >= 4 and (kind, el["id"]) not in ways:
                    lat = np.array([q2["lat"] for q2 in g]); lon = np.array([q2["lon"] for q2 in g])
                    ne = geo.ll_to_m(lat, lon, REG["zone"])   # (N,2) [northing, easting]
                    ways[(kind, el["id"])] = (ne[:, 1], ne[:, 0])   # (E, N)
    return ways

def tile_frames(min_valid=0.98):
    from macro_campaign import cgrid
    out = []
    for sub in REG["tiles"]:
        for js in sorted((TILE_ROOT / sub).glob("*.json")):
            meta = json.load(open(js))
            if meta.get("valid_frac", 0) < min_valid: continue
            z, (_, _, c) = cgrid.read_f32(js.with_suffix(".cgrid"))
            out.append((f"{sub}/{js.stem}", z.astype(float), float(meta["easting0"]),
                        float(meta["northing0"]), c))
    return out

# --- one transect -----------------------------------------------------------------
def med3(v):
    return ndimage.median_filter(v, size=3, mode="nearest")

def sample_transect(z, cell, row, col, nrow, ncol, shift_m=0.0):
    dd = (D + shift_m) / cell
    zt = ndimage.map_coordinates(z, [row + nrow * dd, col + ncol * dd], order=1, mode="nearest")
    return zt

def process(z, cell, row, col, nrow, ncol):
    """-> (S, resolved, shift) or None if screened out."""
    zt = sample_transect(z, cell, row, col, nrow, ncol)
    if not np.all(np.isfinite(zt)): return None
    w = RECENTER_M
    # coarse position on the median-3 profile (robust to single-cell pits),
    # refined on the raw profile within +-2 m (the filter ties on a sharp V
    # and lands a metre off -- the selftest caught it)
    j0 = C0 - w + int(np.argmin(med3(zt)[C0 - w:C0 + w + 1]))
    lo, hi = max(j0 - 2, 0), min(j0 + 3, len(zt))
    j = lo + int(np.argmin(zt[lo:hi]))
    shift = (j - C0) * STEP_M
    if shift != 0.0:
        zt = sample_transect(z, cell, row, col, nrow, ncol, shift)
        if not np.all(np.isfinite(zt)): return None
    m = (np.abs(D) >= DETREND[0]) & (np.abs(D) <= DETREND[1])
    a, b = np.polyfit(D[m], zt[m], 1)
    S = zt - (a * D + b)
    S = S - S[C0]
    if np.max(np.abs(np.diff(S))) > STEP_MAX * STEP_M: return None
    rl, rr = rims(S)
    if max(rl, rr) > RIM_MAX or min(rl, rr) < -1.0: return None
    resolved = S[C0 - 10] >= RESOLVE_MIN and S[C0 + 10] >= RESOLVE_MIN
    return S, resolved, shift

def rims(S):
    return float(np.median(S[C0 - 24:C0 - 15])), float(np.median(S[C0 + 16:C0 + 25]))

def closure(S, rim, right):
    """first |d| >= 2 where the median-3 section reaches 0.8 * rim, 2..24 m."""
    if rim <= 0: return 24.0
    Sm = med3(S)
    for d in range(2, 25):
        v = Sm[C0 + d] if right else Sm[C0 - d]
        if v >= 0.8 * rim: return float(d)
    return 24.0

def descriptors(S):
    rl, rr = rims(S)
    wl, wr = closure(S, rl, False), closure(S, rr, True)
    return dict(depth=0.5 * (rl + rr), w_l=wl, w_r=wr, rim_l=rl, rim_r=rr, asym=(wl - wr) / max(wl + wr, 1e-9))

# --- a way over a tile ---------------------------------------------------------------
def way_transects(z, e0, n0, cell, E, N):
    P = np.stack([E - e0, N - n0], 1)   # metres in tile frame: (x=east, y=north)
    L = float(np.hypot(*np.diff(P, axis=0).T).sum())
    if L < 200: return []
    Q = cm.measured_line(P, STATION_M, 30.0)
    if len(Q) < 4: return []
    T = np.gradient(Q, axis=0); T /= np.maximum(np.hypot(*T.T), 1e-9)[:, None]
    N = np.stack([-T[:, 1], T[:, 0]], 1)
    out = []
    ext = z.shape[0] * cell
    for i in range(len(Q)):
        x, y = Q[i]
        if not (HALF_M + 5 < x < ext - HALF_M - 5 and HALF_M + 5 < y < ext - HALF_M - 5): continue
        r = process(z, cell, y / cell, x / cell, N[i, 1], N[i, 0])
        if r is None: continue
        S, resolved, shift = r
        out.append(dict(station=i, x=x, y=y, nx=N[i, 0], ny=N[i, 1], S=S, resolved=resolved, shift=shift, **descriptors(S)))
    return out

def along_corr(rows):
    """e-folding lag (m) of the depth series along one way, or None."""
    d = np.array([r["depth"] for r in rows]); d = d - d.mean()
    if len(d) < 30 or d.std() < 1e-6: return None
    ac = np.correlate(d, d, "full")[len(d) - 1:] / (d @ d)
    below = np.where(ac < np.exp(-1))[0]
    return float(below[0] * STATION_M) if len(below) else None

# --- measure ------------------------------------------------------------------------
def measure(verbose=True):
    ways = stream_ways()
    tiles = tile_frames()
    per_tile, all_rows, reach_rows, corrs = {}, [], [], []
    for name, z, e0, n0, cell in tiles:
        ext = z.shape[0] * cell
        n_scr = n_res = 0
        for wid, (E, N) in ways.items():
            if not (E.min() < e0 + ext and E.max() > e0 and N.min() < n0 + ext and N.max() > n0): continue
            rows = way_transects(z, e0, n0, cell, E, N)
            if not rows: continue
            n_scr += len(rows); n_res += sum(r["resolved"] for r in rows)
            for r in rows: r["tile"] = name; r["way"] = wid
            kept = [r for r in rows if r["resolved"]]
            all_rows += kept
            for k in range(0, len(kept) - REACH_N + 1, REACH_N):
                Sm = np.median(np.stack([r["S"] for r in kept[k:k + REACH_N]]), 0)
                reach_rows.append(dict(S=Sm, tile=name, way=wid, **descriptors(Sm)))
            c = along_corr(rows)
            if c is not None: corrs.append(c)
        if n_scr:
            per_tile[name] = (n_scr, n_res)
            if verbose: print(f"  {name}: {n_scr} transects, {n_res} resolved ({100*n_res/max(n_scr,1):.0f}%)", flush=True)
    n_scr = sum(v[0] for v in per_tile.values()); n_res = sum(v[1] for v in per_tile.values())
    stats = dict(n_screened=n_scr, n_resolved=n_res, resolved_frac=n_res / max(n_scr, 1),
                 along_corr_m=float(np.median(corrs)) if corrs else 45.0, n_tiles=len(per_tile),
                 n_reaches=len(reach_rows))
    return all_rows, reach_rows, per_tile, stats

def pct(v):
    return [float(np.percentile(v, p)) for p in (10, 25, 50, 75, 90)]

def export(all_rows, reach_rows, stats, path=None, n_s=256):
    # NOT a default argument: `ASSET` is rebound by use_region(), and a
    # default captured at def time wrote the Nebraska pack over the Carolina
    # one on the first regional run.
    path = path or ASSET
    rows = sorted(all_rows, key=lambda r: 0.5 * (r["w_l"] + r["w_r"]))
    idx = np.unique(np.linspace(0, len(rows) - 1, min(n_s, len(rows))).astype(int)) if rows else []
    s_rows = [rows[i] for i in idx]
    m_rows = sorted(reach_rows, key=lambda r: 0.5 * (r["w_l"] + r["w_r"]))
    sl = slice(C0 - EXPORT_HALF, C0 + EXPORT_HALF + 1)
    with open(path, "w") as f:
        f.write("CSEC1\n")
        f.write(f"# measured 2 m creek cross-sections: {stats['n_resolved']} resolved of {stats['n_screened']} screened transects, "
                f"{stats['n_reaches']} reaches, {stats['n_tiles']} {'+'.join(REG['tiles'])} tiles, "
                f"waterway={'/'.join(REG['kinds'])} (tools/aeolian/creek_sections.py)\n")
        f.write(f"# OSM waterway=stream, {STATION_M:.0f} m stations, +-{HALF_M} m at {STEP_M:.0f} m, recentred +-{RECENTER_M} m, detrended {DETREND[0]}..{DETREND[1]} m\n")
        f.write(f"half_m {EXPORT_HALF}\nstep_m {STEP_M:.0f}\nn_sections {len(s_rows)}\nn_medians {len(m_rows)}\n")
        f.write(f"resolved_frac {stats['resolved_frac']:.4f}\nalong_corr_m {stats['along_corr_m']:.1f}\n")
        for key, vals in (("depth_p", [r["depth"] for r in all_rows]), ("width_p", [0.5*(r["w_l"]+r["w_r"]) for r in all_rows]),
                          ("asym_p", [r["asym"] for r in all_rows])):
            f.write(f"{key} " + " ".join(f"{v:.3f}" for v in pct(vals)) + "\n")
        f.write("# s depth w_l w_r v(-24)..v(+24)   individual transects, sorted by mean closure width\n")
        for r in s_rows:
            f.write(f"s {r['depth']:.3f} {r['w_l']:.0f} {r['w_r']:.0f} " + " ".join(f"{v:.3f}" for v in r["S"][sl]) + "\n")
        f.write("# m ...   reach medians (20 stations)\n")
        for r in m_rows:
            f.write(f"m {r['depth']:.3f} {r['w_l']:.0f} {r['w_r']:.0f} " + " ".join(f"{v:.3f}" for v in r["S"][sl]) + "\n")
    print(f"wrote {path}: {len(s_rows)} s rows, {len(m_rows)} m rows; resolved_frac {stats['resolved_frac']:.3f}; along_corr {stats['along_corr_m']:.0f} m")

# --- sheet -------------------------------------------------------------------------
def our_transects(gen_dir, seeds):
    """Our current cut's sections, through the SAME detrend, for the overlay."""
    import creek_terrain_sheet as ts, creek_crossings as cc
    out = []
    for seed in seeds:
        try:
            z, wet, c = ts.load(gen_dir, seed, "tex_")
        except Exception:
            continue
        _, m, _ = cc.creek_mask(pathlib.Path(gen_dir) / f"tex_{seed}.water.cgrid")
        if m is None: continue
        for (cy, cx) in ts.creek_stations(m, c, fracs=(0.2, 0.35, 0.5, 0.65, 0.8)):
            for dd, zt, wt in ts.transects(z, m, c, cy, cx, n=3, half_m=HALF_M, step_m=40.0):
                zt = np.interp(D, dd, zt)
                msk = (np.abs(D) >= DETREND[0]) & (np.abs(D) <= DETREND[1])
                a, b = np.polyfit(D[msk], zt[msk], 1)
                S = zt - (a * D + b); S -= S[C0]
                out.append(S)
    return out

def sheet(all_rows, reach_rows, per_tile, stats, out_html, gen_dir=None, seeds=()):
    import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
    from siting_sheet import hillshade_rgb
    import creek_overlay_sheet as ov
    from PIL import Image
    from macro_campaign import cgrid
    parts = []
    # crops: 6 tiles with most resolved transects, each centred on a resolved station
    top = sorted(per_tile.items(), key=lambda kv: -kv[1][1])[:6]
    rng = np.random.default_rng(3)
    for name, (n_scr, n_res) in top:
        z, (_, _, c) = cgrid.read_f32(TILE_ROOT / f"{name}.cgrid"); z = z.astype(float)
        rows = [r for r in all_rows if r["tile"] == name]
        if not rows: continue
        r = rows[rng.integers(len(rows))]
        cx, cy = int(r["x"] / c), int(r["y"] / c); half = 150
        x0 = int(np.clip(cx - half, 0, z.shape[1] - 2*half)); y0 = int(np.clip(cy - half, 0, z.shape[0] - 2*half))
        zz = z[y0:y0+2*half, x0:x0+2*half]
        base = hillshade_rgb(zz, c)
        # every stream vertex of this tile's ways inside the crop, as line layers
        ways = stream_ways()
        meta = json.load(open(TILE_ROOT / f"{name}.json")); e0, n0 = meta["easting0"], meta["northing0"]
        h, w = base.shape[:2]; scale = 2
        im = Image.new("RGBA", (w*scale, h*scale), (0, 0, 0, 0))
        from PIL import ImageDraw
        dr = ImageDraw.Draw(im)
        for wid, (E, N) in ways.items():
            px = (E - e0) / c - x0; py = (N - n0) / c - y0
            if not ((px > -50) & (px < w + 50) & (py > -50) & (py < h + 50)).any(): continue
            dr.line([(float(x*scale), float((h - 1 - y)*scale)) for x, y in zip(px, py)], fill=(214, 48, 36, 235), width=2)
        # resolved stations as dots
        for q in rows:
            qx, qy = q["x"]/c - x0, q["y"]/c - y0
            if 0 <= qx < w and 0 <= qy < h:
                dr.ellipse([(qx*scale-3, (h-1-qy)*scale-3), (qx*scale+3, (h-1-qy)*scale+3)], fill=(255, 236, 90, 230))
        b_img = Image.fromarray(np.flipud(base)).resize((w*scale, h*scale), Image.LANCZOS)
        parts.append(f'<div class="panel" style="width:{w*scale}px;height:{h*scale}px"><img class="base" src="{ov.uri(b_img, quality=86)}">'
                     f'<img class="creek" src="{ov.uri(im, "PNG")}"><div class="cap">{name} · {n_res}/{n_scr} resolved · 600 m at 2x</div></div>')
    crops_html = '<div class="row">' + "".join(parts) + "</div>"
    # stacked sections
    S = np.stack([r["S"] for r in all_rows]); M = np.stack([r["S"] for r in reach_rows]) if reach_rows else None
    fig, ax = plt.subplots(1, 1, figsize=(9, 3.6), dpi=100)
    for lo, hi, al in ((10, 90, 0.15), (25, 75, 0.3)):
        ax.fill_between(D, np.percentile(S, lo, 0), np.percentile(S, hi, 0), color="#1f4e79", alpha=al, lw=0)
    ax.plot(D, np.median(S, 0), color="#1f4e79", lw=2, label=f"real resolved transects, median (n={len(S)})")
    if M is not None:
        for m in M[::max(1, len(M)//40)]: ax.plot(D, m, color="#1f4e79", lw=0.5, alpha=0.35)
    if gen_dir:
        ours = our_transects(gen_dir, seeds)
        if ours:
            O = np.stack(ours)
            ax.plot(D, np.median(O, 0), color="#b03a2e", lw=2, label=f"our current cut, median (n={len(O)})")
            ax.fill_between(D, np.percentile(O, 25, 0), np.percentile(O, 75, 0), color="#b03a2e", alpha=0.2, lw=0)
    ax.set_ylim(-0.5, 3.0); ax.set_xlim(-HALF_M, HALF_M); ax.grid(True, color="#eee"); ax.legend(fontsize=8)
    ax.set_xlabel("m across the creek (re-centred on the channel, detrended 25–40 m)"); ax.set_ylabel("m above the bed")
    fig.tight_layout(); b = io.BytesIO(); fig.savefig(b, format="png"); plt.close(fig)
    stack_uri = "data:image/png;base64," + base64.b64encode(b.getvalue()).decode()
    fig, axes = plt.subplots(1, 3, figsize=(9, 2.4), dpi=100)
    for ax_, key, lab in zip(axes, ("depth", "w", "asym"), ("incision depth (rim) m", "closure width m", "asymmetry (w_l-w_r)/(w_l+w_r)")):
        v = [r[key] for r in all_rows] if key != "w" else [0.5*(r["w_l"]+r["w_r"]) for r in all_rows]
        ax_.hist(v, bins=30, color="#1f4e79", alpha=0.8); ax_.set_xlabel(lab, fontsize=8); ax_.tick_params(labelsize=7)
    fig.tight_layout(); b = io.BytesIO(); fig.savefig(b, format="png"); plt.close(fig)
    hist_uri = "data:image/png;base64," + base64.b64encode(b.getvalue()).decode()
    dp, wp, ap = pct([r["depth"] for r in all_rows]), pct([0.5*(r["w_l"]+r["w_r"]) for r in all_rows]), pct([r["asym"] for r in all_rows])
    tbl = "".join(f"<tr><td>{n}</td><td>{s}</td><td>{r}</td><td>{100*r/max(s,1):.0f}%</td></tr>" for n, (s, r) in sorted(per_tile.items(), key=lambda kv: -kv[1][1]))
    html = f"""<title>Real Creek Sections</title>
<style>
:root{{--bg:#f6f4ee;--ink:#22231f;--muted:#5f5e57;--rule:#d9d5c9;--accent:#8a3b2a;--panel:#fffdf8}}
@media (prefers-color-scheme: dark){{:root:not([data-theme="light"]){{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}}}
:root[data-theme="dark"]{{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:20px 24px;line-height:1.45}}
h2{{font-size:1.45rem;margin:0 0 6px}}h3{{font-size:1.05rem;margin:26px 0 6px;color:var(--accent)}}p{{max-width:72ch;color:var(--muted);margin:4px 0 10px}}
.controls{{position:sticky;top:0;z-index:5;background:var(--bg);padding:8px 0 10px;border-bottom:1px solid var(--rule);font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.row{{display:flex;gap:10px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{position:relative;background:var(--panel);flex:none}}.panel img{{position:absolute;left:0;top:0;width:100%;height:100%}}
body.hide-creek .panel .creek{{display:none}}
.cap{{position:absolute;left:6px;bottom:4px;font-size:11px;color:#fff;text-shadow:0 0 3px #000,0 0 3px #000;font-family:"IBM Plex Mono",ui-monospace,monospace}}
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums}}td,th{{border:1px solid var(--rule);padding:3px 8px;text-align:right}}td:first-child{{text-align:left}}
.k{{font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:12.5px}}
</style>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<h2>What a real sandhills creek does to the ground at 2 m</h2>
<p>OSM streams joined to the corpus DTM tiles. Red = mapped stream; yellow dots = stations where the DTM actually shows a channel (≥ 0.15 m rise by ±10 m). Toggle the lines off to see the ground as the render sees it.</p>
<div class="controls"><label><input type="checkbox" id="t-creek" checked> stream lines and resolved stations</label></div>
<h3>Real crops, 600 m at 2x, tinted hillshade</h3>{crops_html}
<h3>Cross-sections</h3>
<p class=k>screened transects {stats['n_screened']} · resolved {stats['n_resolved']} ({100*stats['resolved_frac']:.0f} %) · reaches {stats['n_reaches']} · along-stream depth correlation {stats['along_corr_m']:.0f} m<br>
depth p10/25/50/75/90 = {' / '.join(f'{v:.2f}' for v in dp)} m · closure width = {' / '.join(f'{v:.0f}' for v in wp)} m · asym = {' / '.join(f'{v:.2f}' for v in ap)}</p>
<img src="{stack_uri}"><br><img src="{hist_uri}">
<h3>Per tile</h3><table><tr><th>tile</th><th>screened</th><th>resolved</th><th>resolved %</th></tr>{tbl}</table>
<script>document.getElementById('t-creek').onchange=e=>document.body.classList.toggle('hide-creek',!e.target.checked);</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html}")

# --- selftest -----------------------------------------------------------------------
def selftest():
    cell = 2.0; n = 300
    yy, xx = np.indices((n, n)); X = xx * cell; Y = yy * cell
    plane = 0.02 * X + 0.01 * Y
    def run(channel):
        z = plane + channel
        # a straight "way" along y at x = 300 m
        lat = np.linspace(0, 1, 40)
        rows = []
        for y in np.arange(60, 540, STATION_M):
            r = process(z, cell, y / cell, 300.0 / cell, 0.0, 1.0)   # normal = +x: (nrow, ncol) = (0, 1)
            if r is not None: rows.append(descriptors(r[0]) | dict(resolved=r[1]))
        return rows
    # V channel: depth 0.6, half-width 6 m
    dx = np.abs(X - 300.0)
    V = -0.6 * np.clip(1 - dx / 6.0, 0, 1)
    U = -0.6 * np.clip(1 - (dx / 6.0) ** 2, 0, 1)
    for name, ch, w_expect in (("V", V, 4.8), ("U", U, 0.8**0.5 * 6.0 if False else 6.0 * np.sqrt(1 - 0.2))):
        rows = run(ch)
        assert rows and all(r["resolved"] for r in rows), f"{name}: not resolved"
        d = np.median([r["depth"] for r in rows]); w = np.median([0.5*(r["w_l"]+r["w_r"]) for r in rows])
        assert abs(d - 0.6) / 0.6 < 0.05, f"{name}: depth {d:.3f}"
        assert abs(w - w_expect) <= 1.0, f"{name}: width {w:.2f} vs {w_expect:.2f}"
        print(f"  {name}: depth {d:.3f} (0.600) width {w:.2f} ({w_expect:.2f}) OK")
    flat = run(np.zeros_like(plane))
    assert flat and not any(r["resolved"] for r in flat), "flat plane must be unresolved"
    print("  flat: unresolved OK\nselftest OK")

if __name__ == "__main__":
    a = sys.argv[1:]
    for _k in list(REGIONS):
        if f"--region={_k}" in a:
            use_region(_k); a = [x for x in a if x != f"--region={_k}"]
    if "--selftest" in a:
        selftest()
    elif a and a[0] == "measure":
        all_rows, reach_rows, per_tile, stats = measure()
        export(all_rows, reach_rows, stats)
        (ROOT / "out/creek").mkdir(parents=True, exist_ok=True)
        gen = a[1] if len(a) > 1 else None; seeds = a[2:]
        sheet(all_rows, reach_rows, per_tile, stats, ROOT / f"out/creek/step0_real_sections{REG['tag']}.html", gen, seeds)
    elif a and a[0] == "sheet":
        all_rows, reach_rows, per_tile, stats = measure(verbose=False)
        sheet(all_rows, reach_rows, per_tile, stats, ROOT / f"out/creek/step0_real_sections{REG['tag']}.html", a[1], a[2:])
