import base64, json, pathlib, html
SRC = pathlib.Path("out/m2_viewer")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/m2_viewer.html")
stats = json.loads((SRC/"stats.json").read_text())
VIEWS = [("net","Network","reaches coloured and widened by Strahler order; grey = growth that never reached the channel threshold"),
         ("terr","Territory","distance to the nearest channel — banded, so drainage spacing is legible")]
ARCHES = ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]
LABEL = {"piedmont":"Piedmont","great_plains":"Great Plains","river_valley":"River Valley",
         "hill_country":"Hill Country","heathland":"Heathland","sandhills":"Sandhills"}
seeds = sorted({r["seed"] for r in stats})
def uri(p):
    f = SRC/p
    return "data:image/jpeg;base64," + base64.b64encode(f.read_bytes()).decode() if f.exists() else ""
by = {(r["arch"], r["seed"]): r for r in stats}

rows=[]
for a in ARCHES:
    tiles=[]
    for s in seeds:
        r = by[(a,s)]
        imgs = " ".join(f'data-{v}="{uri(f"{a}_{s}_{v}.jpg")}"' for v,_,_ in VIEWS)
        state = "ok" if r["filled"] else "bad"
        tiles.append(f'''<button class="tile {state}" {imgs} data-stats='{html.escape(json.dumps(r))}'>
<img alt="{LABEL[a]} seed {s}" loading="lazy"><span class="tile-tag"><b>{s}</b><i>{r['density']:.2f} km/km² · Ω{r['omega']}</i></span></button>''')
    filled = sum(1 for s in seeds if by[(a,s)]["filled"])
    rows.append(f'''<section class="row" data-arch="{a}">
<header class="row-head"><h3>{LABEL[a]}</h3>
<p class="tally"><span class="{'g' if filled==len(seeds) else ('w' if filled else 'b')}">{filled} of {len(seeds)} reached usable density</span></p></header>
<div class="row-tiles">{"".join(tiles)}</div></section>''')

viewbtns = "".join(f'<button class="seg{" on" if i==0 else ""}" data-view="{v}">{n}</button>' for i,(v,n,d) in enumerate(VIEWS))
viewdesc = "".join(f'<span class="vd{" on" if i==0 else ""}" data-view="{v}">{d}</span>' for i,(v,n,d) in enumerate(VIEWS))

DOC = f'''<title>M2 · Network growth</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{{--ground:#E7E9EC;--surface:#FFF;--surface2:#F1F4F7;--ink:#171A1F;--muted:#5A616C;
 --line:#CDD3DA;--line-soft:#E0E4E9;--trunk:#2F7FC4;--bad:#B8402C;--ok:#2F7D52;--warn:#9C6B16;--r:3px}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{
 --ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;
 --line:#2A2F38;--line-soft:#22262D;--trunk:#56B4FF;--bad:#E8654A;--ok:#5FBF86;--warn:#E8B93E}}}}
:root[data-theme="dark"]{{--ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;
 --line:#2A2F38;--line-soft:#22262D;--trunk:#56B4FF;--bad:#E8654A;--ok:#5FBF86;--warn:#E8B93E}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--ground);color:var(--ink);
 font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;-webkit-font-smoothing:antialiased}}
.wrap{{max-width:1280px;margin:0 auto;padding:0 24px 72px}}
header.top{{border-bottom:1px solid var(--line);margin-bottom:26px;padding:40px 0 22px}}
.eyebrow{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 10px}}
h1{{font-size:clamp(26px,3.4vw,40px);line-height:1.08;margin:0 0 10px;letter-spacing:-.02em;text-wrap:balance}}
.lede{{margin:0;max-width:64ch;color:var(--muted);font-size:16px}}
.verdict{{margin:18px 0 0;padding:13px 15px;border-left:3px solid var(--bad);background:var(--surface);border-radius:0 var(--r) var(--r) 0}}
.verdict b{{color:var(--bad)}}
.bar{{position:sticky;top:0;z-index:20;background:color-mix(in srgb,var(--ground) 88%,transparent);
 backdrop-filter:blur(9px);border-bottom:1px solid var(--line);margin-bottom:22px;display:flex;flex-wrap:wrap;gap:14px;align-items:center;padding:11px 0}}
.segs{{display:inline-flex;border:1px solid var(--line);border-radius:var(--r);overflow:hidden;background:var(--surface)}}
.seg{{appearance:none;border:0;background:transparent;color:var(--muted);cursor:pointer;padding:7px 13px;font:inherit;font-size:13px;border-right:1px solid var(--line-soft)}}
.seg:last-child{{border-right:0}} .seg.on{{background:var(--ink);color:var(--surface)}}
.seg:focus-visible,.tile:focus-visible{{outline:2px solid var(--trunk);outline-offset:2px}}
.vdesc{{color:var(--muted);font-size:13px;flex:1;min-width:200px}} .vd{{display:none}} .vd.on{{display:inline}}
.row{{margin-bottom:32px}}
.row-head{{display:flex;gap:14px;align-items:baseline;flex-wrap:wrap;margin-bottom:11px;padding-bottom:8px;border-bottom:1px solid var(--line-soft)}}
.row-head h3{{margin:0;font-size:16px}}
.tally{{margin:0;font-family:ui-monospace,Menlo,monospace;font-size:12px}}
.tally .g{{color:var(--ok)}} .tally .w{{color:var(--warn)}} .tally .b{{color:var(--bad)}}
.row-tiles{{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:12px}}
.tile{{position:relative;padding:0;border:1px solid var(--line);border-radius:var(--r);background:var(--surface2);cursor:pointer;overflow:hidden;line-height:0}}
.tile.bad{{border-color:var(--bad)}} .tile.ok{{border-color:var(--ok)}}
.tile img{{width:100%;height:auto;display:block}}
.tile-tag{{position:absolute;left:0;bottom:0;display:flex;gap:7px;align-items:baseline;padding:4px 8px;
 background:color-mix(in srgb,#0F1115 74%,transparent);color:#E9ECF1;font-family:ui-monospace,Menlo,monospace;font-size:11px;line-height:1.5;border-top-right-radius:var(--r)}}
.tile-tag i{{font-style:normal;opacity:.72}}
.notes{{margin-top:46px;border-top:1px solid var(--line);padding-top:28px}}
.notes h2{{font-size:19px;margin:0 0 6px;letter-spacing:-.01em}}
.notes h3{{font-size:15px;margin:26px 0 4px}}
.sub{{color:var(--muted);margin:0 0 16px;font-size:14px;max-width:68ch}}
.finds{{display:grid;gap:12px;grid-template-columns:repeat(auto-fit,minmax(270px,1fr))}}
.find{{background:var(--surface);border:1px solid var(--line);border-radius:var(--r);padding:15px 16px}}
.find h4{{margin:0 0 7px;font-size:14px}} .find p{{margin:0;color:var(--muted);font-size:13.5px}}
.find .num{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.14em;text-transform:uppercase;color:var(--muted);display:block;margin-bottom:7px}}
code{{font-family:ui-monospace,Menlo,monospace;font-size:12.5px;background:var(--surface2);padding:1px 4px;border-radius:2px}}
.tblwrap{{overflow-x:auto;margin:0 0 14px;border:1px solid var(--line);border-radius:var(--r);background:var(--surface)}}
table{{border-collapse:collapse;width:100%;min-width:520px;font-size:13px;font-family:ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums}}
th{{text-align:left;font-weight:600;color:var(--muted);font-size:11px;letter-spacing:.1em;text-transform:uppercase;padding:9px 12px;border-bottom:1px solid var(--line)}}
td{{padding:8px 12px;border-bottom:1px solid var(--line-soft)}} tr:last-child td{{border-bottom:0}}
td.b{{color:var(--bad)}} td.g{{color:var(--ok)}}
dialog{{border:1px solid var(--line);border-radius:var(--r);background:var(--surface);color:var(--ink);padding:0;max-width:min(900px,94vw);width:100%}}
dialog::backdrop{{background:rgba(8,9,12,.66)}}
.dhead{{display:flex;justify-content:space-between;align-items:baseline;gap:16px;padding:15px 18px;border-bottom:1px solid var(--line)}}
.dhead h3{{margin:0;font-size:16px}}
.dhead button{{appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--muted);border-radius:var(--r);cursor:pointer;font:inherit;font-size:13px;padding:4px 10px}}
.dbody{{display:grid;grid-template-columns:minmax(0,1.2fr) minmax(210px,.8fr)}}
.dbody img{{width:100%;height:auto;display:block;border-right:1px solid var(--line)}}
.dstats{{padding:14px 18px;display:grid;grid-template-columns:1fr auto;gap:5px 14px;align-content:start;font-family:ui-monospace,Menlo,monospace;font-size:12.5px;font-variant-numeric:tabular-nums}}
.dstats dt{{color:var(--muted)}} .dstats dd{{margin:0;text-align:right}}
@media (max-width:720px){{.dbody{{grid-template-columns:1fr}} .dbody img{{border-right:0;border-bottom:1px solid var(--line)}}}}
@media (prefers-reduced-motion:reduce){{*{{transition:none!important;animation:none!important}}}}
</style>
<div class="wrap">
<header class="top">
  <p class="eyebrow">Attempt 4 · branch network-first · M2 · partial</p>
  <h1>Step 4: the network grows, but it is not yet a drainage network</h1>
  <p class="lede">Headward growth from the placed trunk mouths. The engine works and the metrics are measurable on the
  graph before any surface exists — which is what the whole reorder was for. But two of six metrics are out of band,
  growth is unreliable, and the renders show something the numbers did not.</p>
  <div class="verdict"><b>What the pictures say that the metrics did not:</b> this is a
  <b>space-filling maze, not a drainage network</b>. The channels snake, double back, and run in near-parallel pairs;
  there is no trunk-and-tributary hierarchy, and almost nothing converges on the mouth. A drainage network is organised
  by where water can go. This one is organised by where there was room.</div>
</header>
<div class="bar"><div class="segs">{viewbtns}</div><div class="vdesc">{viewdesc}</div></div>
{"".join(rows)}
<section class="notes">
  <h2>Where the numbers stand</h2>
  <div class="tblwrap"><table>
    <thead><tr><th>metric</th><th>corpus band</th><th>ours</th><th></th></tr></thead>
    <tbody>
      <tr><td>drainage density</td><td>2.21–2.60 km/km²</td><td>0.37–2.98</td><td class="b">unreliable</td></tr>
      <tr><td>d2c median</td><td>96–116 m</td><td>95 (rv) → 1300 (most)</td><td class="b">unreliable</td></tr>
      <tr><td>Horton Rb</td><td>3.92–4.74</td><td>~2.0</td><td class="b">out of band</td></tr>
      <tr><td>Horton Rl</td><td>1.64–2.25</td><td>0.9–1.3</td><td class="b">out of band</td></tr>
      <tr><td>junction p50</td><td>37–45°</td><td>54–85°</td><td class="b">out of band</td></tr>
      <tr><td>loops / crossings</td><td>none</td><td>none</td><td class="g">by construction</td></tr>
    </tbody>
  </table></div>

  <h3>Four defects already found and fixed</h3>
  <p class="sub">Each by instrument rather than argument — <code>GrowStats</code> records why every head stopped.</p>
  <div class="finds">
    <div class="find"><span class="num">Fixed</span><h4>Branching killed itself</h4>
      <p>Two children leave the same node ~30 m apart, inside the spacing floor, so one died immediately: 37 nodes and
      density 0.10.</p></div>
    <div class="find"><span class="num">Fixed</span><h4>Heads hit their own path</h4>
      <p>The lineage exemption reset at every branch, so with a 468 m trail against a 420 m claim, any curvature was
      fatal.</p></div>
    <div class="find"><span class="num">Fixed</span><h4>Spacing calibrated from the wrong model</h4>
      <p>A parallel-channel estimate said spacing ≈ 4·d2c. A dendritic tree packs more length into the same spacing;
      the measured factor is nearer 1.4.</p></div>
    <div class="find"><span class="num">Fixed</span><h4>Uniform spacing forbids junctions</h4>
      <p>A junction is exactly where two channels are close together. Required spacing now ramps with distance
      <em>through the tree</em> — zero at a confluence, full at 2× the claim distance.</p></div>
  </div>

  <h3>The structural finding</h3>
  <p class="sub"><strong>A strictly binary tree has a bifurcation ratio of exactly 2</strong>, against a corpus band of
  3.92–4.74 — and no dial fixes that. A sweep over branch angle and rate moved Rb only between 1.5 and 2.4. Real
  networks are stems with many short tributaries entering along their length, not bifurcating trees. Branching is now
  asymmetric, which is also what gives an acute junction angle: a tributary needs a <em>continuing</em> stem to join at
  an angle to. It helped the angles. Rb is still ~2.0.</p>

  <h3>The decision this viewer is for</h3>
  <p class="sub">The spacing rule and the repulsion are doing three jobs at once — setting density, setting junction
  geometry, and deciding whether a head survives — and they are over-coupled, so every fix to one moved the others.
  That is the tuning story. The renders add a stronger one: growth has almost no elevation constraint, so it fills
  space rather than draining it.</p>
  <p class="sub"><strong>The proposed change:</strong> give the network a long profile <em>while it grows</em> rather
  than after, and make a head die when the catchment upslope can no longer supply the channel-initiation threshold —
  which is the actual physics — instead of dying when it bumps into a neighbour. Spacing then sets density alone, the
  branch angle sets junction geometry alone, and convergence toward the mouth comes from the elevations rather than
  from a steering weight.</p>
</section>
</div>
<dialog id="dlg"><div class="dhead"><h3 id="dtitle">—</h3><button id="dclose">Close</button></div>
<div class="dbody"><img id="dimg" alt=""><dl class="dstats" id="dstats"></dl></div></dialog>
<script>
let view="net";
const tiles=[...document.querySelectorAll(".tile")];
function paint(){{for(const t of tiles) t.querySelector("img").src=t.dataset[view];}}
paint();
document.querySelectorAll(".seg").forEach(b=>b.addEventListener("click",()=>{{
  view=b.dataset.view;
  document.querySelectorAll(".seg").forEach(x=>x.classList.toggle("on",x===b));
  document.querySelectorAll(".vd").forEach(x=>x.classList.toggle("on",x.dataset.view===view));
  paint(); if(dlg.open) fill(cur);
}}));
const LABEL={json.dumps(LABEL)};
const F=[["density","density km/km²",v=>v.toFixed(2)],["omega","Strahler max",v=>v],
 ["nodes","nodes grown",v=>v],["chan","channel nodes",v=>v],["reaches","reaches",v=>v],
 ["trunks","trunks seeded",v=>v],["steps","steps taken",v=>v],
 ["offtile","heads lost off-tile",v=>v],["claimed","heads stopped by neighbours",v=>v]];
const dlg=document.getElementById("dlg"); let cur=null;
function fill(t){{cur=t;const s=JSON.parse(t.dataset.stats);
 document.getElementById("dtitle").textContent=LABEL[s.arch]+" · seed "+s.seed;
 const i=document.getElementById("dimg"); i.src=t.dataset[view]; i.alt=LABEL[s.arch]+" seed "+s.seed;
 const dl=document.getElementById("dstats"); dl.innerHTML="";
 for(const f of F){{const dt=document.createElement("dt");dt.textContent=f[1];
  const dd=document.createElement("dd");dd.textContent=f[2](s[f[0]]);dl.append(dt,dd);}}}}
tiles.forEach(t=>t.addEventListener("click",()=>{{fill(t);dlg.showModal();}}));
document.getElementById("dclose").addEventListener("click",()=>dlg.close());
dlg.addEventListener("click",e=>{{if(e.target===dlg)dlg.close();}});
</script>
'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
