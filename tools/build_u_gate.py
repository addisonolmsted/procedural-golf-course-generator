import base64, json, pathlib
SRC = pathlib.Path("out/u_gate")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/u_gate.html")
stats = json.loads((SRC/"stats.json").read_text())
ARCHES=["piedmont","great_plains","river_valley","hill_country","heathland"]
LABEL={"piedmont":"Piedmont","great_plains":"Great Plains","river_valley":"River Valley","hill_country":"Hill Country","heathland":"Heathland"}
NOTE={"piedmont":"rolling: one long gentle slope zone; interfluves from the relief field",
"great_plains":"flat with edges: low caprock scarp, wide flat crown",
"river_valley":"wide varying floor; per-side terrace program, arc-gated risers and treads",
"hill_country":"bimodal: hardness-gated bluffs over widened floors; benches between",
"heathland":"broad and subdued; kettles arrive at T4"}
seeds=sorted({r["seed"] for r in stats})
by={(r["arch"],r["seed"]):r for r in stats}
def uri(p):
    f=SRC/p
    return "data:image/jpeg;base64,"+base64.b64encode(f.read_bytes()).decode() if f.exists() else ""
rows=[]
for a in ARCHES:
    tiles="".join(f'''<figure><img src="{uri(f"{a}_{s}_terrain.jpg")}" loading="lazy" alt="{LABEL[a]} {s}">
<figcaption>seed {s} · relief {by[(a,s)]['relief']:.0f} m · under-8% {100*by[(a,s)]['cap']:.0f}%</figcaption></figure>''' for s in seeds if (a,s) in by)
    rows.append(f'''<section class="row"><header><h3>{LABEL[a]}</h3><p>{NOTE[a]}</p></header><div class="strip">{tiles}</div></section>''')
DOC=f'''<title>U gate · Macro terrain</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{{--ground:#E7E9EC;--surface:#FFF;--ink:#171A1F;--muted:#5A616C;--line:#CDD3DA;--r:3px}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--ground:#0F1115;--surface:#181B21;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38}}}}
:root[data-theme="dark"]{{--ground:#0F1115;--surface:#181B21;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--ground);color:var(--ink);font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}}
.wrap{{max-width:1380px;margin:0 auto;padding:0 22px 64px}}
header.top{{padding:32px 0 18px;border-bottom:1px solid var(--line);margin-bottom:20px}}
.eyebrow{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 8px}}
h1{{font-size:clamp(23px,2.8vw,34px);margin:0 0 8px;letter-spacing:-.02em}}
.lede{{margin:0;color:var(--muted);max-width:72ch}}
.row{{margin-bottom:26px}}
.row header{{display:flex;gap:14px;align-items:baseline;flex-wrap:wrap;margin-bottom:9px;border-bottom:1px solid var(--line);padding-bottom:6px}}
.row h3{{margin:0;font-size:16px}}
.row header p{{margin:0;color:var(--muted);font-size:13px}}
.strip{{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:10px}}
figure{{margin:0}}
figure img{{width:100%;height:auto;display:block;border:1px solid var(--line);border-radius:var(--r)}}
figcaption{{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--muted);padding-top:4px}}
.note{{margin-top:30px;border-top:1px solid var(--line);padding-top:18px;color:var(--muted);font-size:14px;max-width:74ch}}
</style>
<div class="wrap">
<header class="top">
<p class="eyebrow">Attempt 4 · U-series gate · round 2</p>
<h1>Macro terrain — tinted hillshade, soft-min frame, core-crossing trunks</h1>
<p class="lede">All height views are now tinted hillshade (shading carries form, a muted tint carries elevation).
The projection frame is C1 at the source — a soft-min over segments smooths distance, arc AND side in one
mechanism, closing the whole seam family including the inside-of-bend creases. Every trunk is required to spend
≥500 m inside the central 2×2 km playable square (asserted over 160 draws). Blue line = trunk bed.</p>
</header>
{"".join(rows)}
<p class="note"><strong>Open:</strong> rv treads still read soft at render scale (explorer-tunable); tint/shade
balance iterates with your explorer verdicts. Sandhills absent by design until T4.</p>
</div>'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
