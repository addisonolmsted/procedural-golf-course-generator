"""Step-1 creek planform sheet: OURS beside REAL, same scale, same pipeline.

  python3 tools/aeolian/creek_sheet.py <sheet.txt> <out.html> [--refetch]
  python3 tools/aeolian/creek_sheet.py --selftest

<sheet.txt> is the labelled-polyline file `planform_sheet` writes. Real
reaches come from tools/aeolian/data/real_creek_reaches.json (committed);
--refetch rebuilds it with one Overpass query per quadrant of the
sandhills_nc box (the same query creek_corpus.py runs).

Every panel is an 800 m window at 1 px/m, the reach rotated so its window
chord is horizontal -- on both sides, so the eye compares planform and not
orientation. The numbers under each panel are computed on the FULL polyline
by creek_measure.py (10 m resample, 30 m smoothing, identical both sides).
"""
import sys, os, json, base64, io, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import creek_measure as cm

REAL_JSON = HERE / "data" / "real_creek_reaches.json"
REAL_TARGETS = ("REAL corpus (55,281 reaches): bend len 40/50/60 m · CV 0.38/0.46/0.55 · "
                "sinuosity 1.063/1.117/1.225 · skew 0.40/0.60/0.71 · straight%R150 59.7 (runs ~118 m)")
WIN_M = 800.0
HALF_H = 130.0

# --- input ---------------------------------------------------------------------
def read_sheet(path):
    polys, meta = {}, {}
    name, pts = None, []
    for line in open(path):
        line = line.rstrip("\n")
        if not line: continue
        if line.startswith("META "):
            parts = line.split()
            meta[parts[1]] = dict(kv.split("=") for kv in parts[2:])
            continue
        try:
            x, y = line.split()
            pts.append((float(x), float(y)))
        except ValueError:
            if name is not None: polys[name] = np.array(pts)
            name, pts = line.strip(), []
    if name is not None: polys[name] = np.array(pts)
    return polys, meta

def fetch_real(n_pick=6):
    sys.path.insert(0, str(HERE.parent / "golf"))
    from corpus import config, net, geo
    QUERY = ('[out:json][timeout:180];(way["waterway"="stream"]({b}););out geom;')
    box = [b for b in config.SEARCH_BOXES if b[0] == "sandhills_nc"][0]
    tag, la, lo, h = box
    reaches = []
    for (qa, qo) in ((la-h/2, lo-h/2), (la-h/2, lo+h/2), (la+h/2, lo-h/2), (la+h/2, lo+h/2)):
        b = f"{qa-h/2},{qo-h/2},{qa+h/2},{qo+h/2}"
        d = net.overpass(QUERY.format(b=b))
        for el in d.get("elements", []):
            g = el.get("geometry") or []
            if len(g) < 8: continue
            lat = np.array([q["lat"] for q in g]); lon = np.array([q["lon"] for q in g])
            P = geo.ll_to_m(lat, lon, geo.utm_zone(float(lon.mean())))
            L = float(np.hypot(*np.diff(P, axis=0).T).sum())
            if L < 1400: continue
            st = cm.bends(P)
            if not st or st["n_bends"] < 8: continue
            reaches.append(dict(id=el.get("id"), tag=tag, length=L, sinuosity=st["sinuosity"],
                                pts=np.round(P - P.min(0), 1).tolist()))
    print(f"  {len(reaches)} candidate reaches >= 1400 m with >= 8 bends")
    # 2 nearest each of the real sinuosity p25/50/75
    pick = []
    for target in (1.063, 1.117, 1.225):
        order = sorted(reaches, key=lambda r: abs(r["sinuosity"] - target))
        for r in order:
            if r not in pick:
                pick.append(r)
                if sum(1 for p in pick if abs(p["sinuosity"]-target) <= abs(r["sinuosity"]-target)+1e-9) >= 2: break
            if len([p for p in pick]) >= n_pick: break
    pick = pick[:n_pick]
    REAL_JSON.parent.mkdir(parents=True, exist_ok=True)
    json.dump(dict(box=box, query=QUERY, note="OSM waterway=stream, UTM metres, origin-shifted",
                   reaches=pick), open(REAL_JSON, "w"))
    print(f"  wrote {REAL_JSON} ({len(pick)} reaches)")

def load_real():
    if not REAL_JSON.exists(): return []
    d = json.load(open(REAL_JSON))
    return [(f"REAL_{r['id']}", np.array(r["pts"])) for r in d["reaches"]]

# --- windows -------------------------------------------------------------------
def window(P, s_center=None, L=WIN_M):
    """Sub-polyline within +-L/2 arc of s_center, rotated chord-horizontal,
    returned with the transform so companion lines can follow."""
    seg = np.hypot(*np.diff(P, axis=0).T); cum = np.r_[0.0, np.cumsum(seg)]
    if s_center is None: s_center = cum[-1] / 2
    lo, hi = s_center - L/2, s_center + L/2
    m = (cum >= lo) & (cum <= hi)
    if m.sum() < 2: m[:] = True
    W = P[m]
    c = W[0]; v = W[-1] - W[0]; th = -np.arctan2(v[1], v[0])
    R = np.array([[np.cos(th), -np.sin(th)], [np.sin(th), np.cos(th)]])
    return (lambda Q: (Q - c) @ R.T), (lo, hi, s_center)

def fmt(st, extra=None):
    if st is None or st.get("bends") is None:
        return "too short to measure"
    f = lambda v, p=1: "–" if v is None else f"{v:.{p}f}"
    s = (f"bends {st['bends']} · len p50 {f(st['len_p50'],0)} m · CV {f(st['cv'],2)} · "
         f"sinu {f(st['sinuosity'],3)} · skew {f(st['skew_p50'],2)} · "
         f"straight%R150 {f(st['straight150'])} / run p90 {f(st['run_p90'],0)} m\n"
         f"xcross35 {f(st['xcross35'])}% · amp p95 {f(st['amp_p95'])} m")
    if extra: s += " · " + extra
    return s

# --- the sheet -----------------------------------------------------------------
def main():
    if "--selftest" in sys.argv:
        sys.path.insert(0, str(HERE))
        import importlib.util
        spec = importlib.util.spec_from_file_location("creek_skew", HERE / "creek_skew.py")
        # creek_skew has a __main__ guard, so importing it is safe
        mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
        s = np.linspace(0, 3000, 3001)
        P = np.stack([s, 12*np.sin(s/60.0) + 5*np.sin(s/23.0)], 1)
        a = mod.bend_skew(P); b = cm.bend_skew(P)
        assert a == b, "bend_skew drifted from creek_skew.py"
        print(f"selftest OK: {len(a)} bends, skew p50 {np.median(a):.3f}")
        return
    src, out_html = sys.argv[1], sys.argv[2]
    if "--refetch" in sys.argv: fetch_real()
    import matplotlib; matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    polys, meta = read_sheet(src)
    real = load_real()
    ours_syn = sorted(k for k in polys if k.startswith("OURS_"))
    dials = sorted(k for k in polys if k.startswith("DIAL_"))
    seeds = sorted({k.split("_")[1] for k in polys if k.startswith("SEED_")})
    sines = [k for k in ("SINE_straight", "SINE_pinch") if k in polys]

    # panel list: (title, main line, companions[(name, pts, style)], stats-extra)
    panels = []
    for name, P in real:
        panels.append((name + "   [real OSM stream, NC sandhills]", P, [], None, "real"))
    for k in ours_syn:
        corr = k.split("_")[1]
        comp = [("axis", polys.get(f"CORR_{corr}_AXIS"), "axis"),
                ("edge", polys.get(f"CORR_{corr}_EDGE_L"), "edge"),
                ("edge", polys.get(f"CORR_{corr}_EDGE_R"), "edge")]
        m = meta.get(k, {})
        extra = f"capped {m.get('capped','?')}/{m.get('bends','?')} · r_min {m.get('r_min','?')} · wobble {m.get('wobble','?')}"
        panels.append((k, polys[k], comp, extra, "ours"))
    for k in dials:
        comp = [("axis", polys.get("CORR_straight_AXIS"), "axis"),
                ("edge", polys.get("CORR_straight_EDGE_L"), "edge"),
                ("edge", polys.get("CORR_straight_EDGE_R"), "edge")]
        m = meta.get(k, {})
        extra = " · ".join(f"{a} {b}" for a, b in m.items())
        panels.append((k, polys[k], comp, extra, "dial"))
    for s in seeds:
        ax = polys.get(f"SEED_{s}_AXIS")
        comp = [("axis", ax, "axis"), ("edge", polys.get(f"SEED_{s}_EDGE_L"), "edge"),
                ("edge", polys.get(f"SEED_{s}_EDGE_R"), "edge")]
        m = meta.get(f"SEED_{s}_OURS", {})
        axs = cm.bends(ax) if ax is not None else None
        extra = (f"axis sinu {axs['sinuosity']:.3f} · " if axs else "") + \
                f"capped {m.get('capped','?')}/{m.get('bends','?')} · lam {m.get('lam','?')} swing {m.get('swing','?')} hw {m.get('hw','?')}"
        panels.append((f"SEED_{s}_OURS   [bend train on the real trunk corridor]", polys[f"SEED_{s}_OURS"], comp, extra, "seed"))
        if f"SEED_{s}_THIN" in polys:
            mh = meta.get(f"SEED_{s}_THIN", {})
            panels.append((f"SEED_{s}_THIN   [bend train, thin-tail first pass, same corridor]", polys[f"SEED_{s}_THIN"], comp,
                           " · ".join(f"{a} {b}" for a, b in mh.items()), "seed"))
        panels.append((f"SEED_{s}_SINE   [SHIPPED two-sine offset, same corridor -- ablation]", polys[f"SEED_{s}_SINE"], comp, None, "sine"))
    for k in sines:
        corr = k.split("_")[1]
        comp = [("axis", polys.get(f"CORR_{corr}_AXIS"), "axis"),
                ("edge", polys.get(f"CORR_{corr}_EDGE_L"), "edge"),
                ("edge", polys.get(f"CORR_{corr}_EDGE_R"), "edge")]
        panels.append((k + "   [SHIPPED two-sine offset -- ablation]", polys[k], comp, None, "sine"))

    ncol = 2
    nrow = (len(panels) + ncol - 1) // ncol
    fig, axes = plt.subplots(nrow, ncol, figsize=(ncol * 9.0, nrow * 3.6), dpi=100)
    axes = np.atleast_2d(axes)
    fig.subplots_adjust(left=0.035, right=0.995, top=1 - 0.9 / (nrow * 3.6), bottom=0.25 / (nrow * 3.6),
                        hspace=0.75, wspace=0.07)
    colors = dict(real="#1f4e79", ours="#b03a2e", dial="#b03a2e", seed="#b03a2e", sine="#7d7d7d")
    rows = []
    for i, (title, P, comp, extra, kind) in enumerate(panels):
        ax = axes[i // ncol][i % ncol]
        st = cm.all_stats(P)
        rows.append((title, st, extra))
        # window: for real reaches and the straight synthetic, the middle; for seed corridors, the middle third
        tf, (lo, hi, sc) = window(P)
        for cname, C, style in comp:
            if C is None: continue
            Cw = tf(C)
            if style == "axis": ax.plot(Cw[:,0], Cw[:,1], ":", color="#999", lw=0.8)
            else: ax.plot(Cw[:,0], Cw[:,1], "-", color="#c8c8c8", lw=0.8)
        W = tf(P); ax.plot(W[:,0], W[:,1], "-", color=colors[kind], lw=1.6)
        # the line as MEASURED (10 m / 30 m), faint
        Q = tf(cm.measured_line(P)); ax.plot(Q[:,0], Q[:,1], "-", color="#f0b27a", lw=0.7, alpha=0.9)
        ax.set_xlim(-20, WIN_M + 20); ax.set_ylim(-HALF_H, HALF_H)
        ax.set_aspect("equal", adjustable="datalim")
        ax.set_xticks(range(0, int(WIN_M)+1, 200)); ax.set_yticks([-100, 0, 100])
        ax.tick_params(labelsize=7); ax.grid(True, color="#eee", lw=0.5)
        ax.set_title(title, fontsize=8.5, loc="left", fontweight="bold" if kind == "real" else "normal")
        ax.text(0.0, -0.16, fmt(st, extra), transform=ax.transAxes, fontsize=7.0, va="top",
                family="monospace")
    for j in range(len(panels), nrow * ncol):
        axes[j // ncol][j % ncol].axis("off")
    fig.suptitle("Creek planform, step 1 (no terrain): REAL (blue) vs BEND TRAIN (red) vs SHIPPED SINE (grey). "
                 "800 m windows at 1 px/m, chord-horizontal. Orange = the 10 m/30 m line the numbers are measured on.\n" + REAL_TARGETS,
                 fontsize=9, y=0.995)
    png = pathlib.Path(out_html).with_suffix(".png")
    fig.savefig(png, dpi=100)
    b = io.BytesIO(); fig.savefig(b, format="png", dpi=100)
    uri = "data:image/png;base64," + base64.b64encode(b.getvalue()).decode()
    # table
    tr = []
    for title, st, extra in rows:
        st = st or {}
        f = lambda v, p=1: "" if v is None else f"{v:.{p}f}"
        tr.append("<tr><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td></tr>" % (
            title, st.get("bends") or "", f(st.get("len_p50"),0), f(st.get("cv"),2), f(st.get("sinuosity"),3),
            f(st.get("skew_p50"),2), f(st.get("straight150")), f(st.get("run_p90"),0), f(st.get("xcross35")), f(st.get("amp_p95")), extra or ""))
    html = f"""<title>Creek Planform Sheet</title>
<style>
:root{{--bg:#f6f4ee;--ink:#22231f;--muted:#5f5e57;--rule:#d9d5c9;--accent:#8a3b2a;--panel:#fffdf8}}
@media (prefers-color-scheme: dark){{:root:not([data-theme="light"]){{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}}}
:root[data-theme="dark"]{{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:20px 24px;line-height:1.45}}
h2{{font-size:1.45rem;margin:0 0 6px;text-wrap:balance}}h3{{font-size:1.05rem;margin:26px 0 4px;color:var(--accent)}}
p{{max-width:70ch;color:var(--muted);margin:4px 0 12px}}.cap{{font-size:11.5px;color:var(--muted);margin:2px 0 8px;font-family:"IBM Plex Mono",ui-monospace,monospace}}
.row{{display:flex;gap:10px;align-items:flex-start;flex-wrap:wrap}}img{{max-width:100%;background:var(--panel)}}
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums}}td,th{{border:1px solid var(--rule);padding:3px 8px;text-align:right}}td:first-child,th:first-child{{text-align:left}}
.wrap{{overflow-x:auto}}
</style>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<h2>Creek planform, step 1: bend train vs real vs shipped sine</h2>
<p>{REAL_TARGETS}</p>
<div class=wrap><img src="{uri}"></div>
<h3>Numbers (full polyline, 10 m resample + 30 m smoothing, identical both sides)</h3>
<div class=wrap><table><tr><th>panel</th><th>bends</th><th>len p50 m</th><th>CV</th><th>sinuosity</th><th>skew p50</th><th>straight% R150</th><th>run p90 m</th><th>xcross35 %</th><th>amp p95 m</th><th>dials</th></tr>
{''.join(tr)}</table></div>
<p style="font-size:11px;color:#666">xcross35 = share of 350 m chords (angled ≥35° to the creek) that cross it twice; amp p95 = lateral amplitude about the reach's own axis, detrended at 8 % of length.</p>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html} and {png}")
    for title, st, extra in rows:
        print(f"  {title[:48]:48s} {fmt(st, extra)}")

if __name__ == "__main__":
    main()
