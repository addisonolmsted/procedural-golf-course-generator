import base64, json, pathlib, html
SRC = pathlib.Path("out/m1_viewer")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/m1_viewer.html")
stats = json.loads((SRC/"stats.json").read_text())
VIEWS = [("all","Composite","relief + strata + grain + trunks"),
         ("relief","Relief field","where the tile tends high or low — steers trunk placement and network growth"),
         ("strata","Strata","the rock column evaluated on a proxy elevation; contacts follow contours"),
         ("grain","Grain","the structural axis the network will be biased along")]
ARCHES = ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]
LABEL = {"piedmont":"Piedmont","great_plains":"Great Plains","river_valley":"River Valley",
         "hill_country":"Hill Country","heathland":"Heathland","sandhills":"Sandhills"}
NOTE = {
 "piedmont":"Reference case. No scarps — a thin veneer draws risers below the 1.5 m bench threshold.",
 "great_plains":"The caprock. Strong contacts, shallow risers; sparsest network of the six.",
 "river_valley":"One trunk, 40–160 km² of external inflow. Dominance is discharge, not catchment.",
 "hill_country":"Thick stack, tall risers. Every slope a staircase. Floor-widen 2.0–3.2× for routability.",
 "heathland":"Deranged: integration 0.02–0.12. Few or no trunks, no strata.",
 "sandhills":"Aeolian — no network at all. The one archetype with real grain anisotropy."}
seeds = sorted({r["seed"] for r in stats})
def uri(p):
    return "data:image/jpeg;base64," + base64.b64encode((SRC/p).read_bytes()).decode()
by = {(r["arch"], r["seed"]): r for r in stats}

cells=[]
for a in ARCHES:
    for s in seeds:
        r = by[(a,s)]
        imgs = " ".join(f'data-{v}="{uri(f"{a}_{s}_{v}.jpg")}"' for v,_,_ in VIEWS)
        cells.append(f'''<button class="tile" data-arch="{a}" data-seed="{s}" {imgs} data-stats='{html.escape(json.dumps(r))}'>
<img alt="{LABEL[a]} seed {s}" loading="lazy"><span class="tile-tag"><b>{s}</b><i>{r['trunks']}T · {r['scarps']}S</i></span></button>''')

rows=[]
for i,a in enumerate(ARCHES):
    tiles = "".join(c for c in cells if f'data-arch="{a}"' in c)
    rows.append(f'''<section class="row" data-arch="{a}">
<header class="row-head"><h3>{LABEL[a]}</h3><p>{NOTE[a]}</p></header>
<div class="row-tiles">{tiles}</div></section>''')

viewbtns = "".join(f'<button class="seg{" on" if i==0 else ""}" data-view="{v}" title="{d}">{n}</button>' for i,(v,n,d) in enumerate(VIEWS))
viewdesc = "".join(f'<span class="vd{" on" if i==0 else ""}" data-view="{v}">{d}</span>' for i,(v,n,d) in enumerate(VIEWS))

DOC = f'''<title>M1 · Network-first terrain</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{{
  --ground:#E7E9EC; --surface:#FFF; --surface2:#F1F4F7; --ink:#171A1F; --muted:#5A616C;
  --line:#CDD3DA; --line-soft:#E0E4E9;
  --trunk:#2F7FC4; --scarp:#C1503A; --grain:#3B8F63; --mouth:#9C7A16;
  --ok:#2F7D52; --warn:#9C6B16;
  --r:3px;
}}
@media (prefers-color-scheme:dark){{ :root:not([data-theme="light"]){{
  --ground:#0F1115; --surface:#181B21; --surface2:#20242B; --ink:#E9ECF1; --muted:#939BA7;
  --line:#2A2F38; --line-soft:#22262D;
  --trunk:#56B4FF; --scarp:#E86A4E; --grain:#5FBF86; --mouth:#E8B93E;
  --ok:#5FBF86; --warn:#E8B93E;
}}}}
:root[data-theme="dark"]{{
  --ground:#0F1115; --surface:#181B21; --surface2:#20242B; --ink:#E9ECF1; --muted:#939BA7;
  --line:#2A2F38; --line-soft:#22262D;
  --trunk:#56B4FF; --scarp:#E86A4E; --grain:#5FBF86; --mouth:#E8B93E;
  --ok:#5FBF86; --warn:#E8B93E;
}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--ground);color:var(--ink);
  font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;
  -webkit-font-smoothing:antialiased}}
.mono{{font-family:ui-monospace,"SF Mono",SFMono-Regular,Menlo,Consolas,monospace}}
.wrap{{max-width:1280px;margin:0 auto;padding:0 24px 72px}}
a{{color:var(--trunk)}}

header.top{{border-bottom:1px solid var(--line);margin-bottom:28px;padding:40px 0 22px}}
.eyebrow{{font-family:ui-monospace,"SF Mono",Menlo,monospace;font-size:11px;letter-spacing:.16em;
  text-transform:uppercase;color:var(--muted);margin:0 0 10px}}
h1{{font-size:clamp(26px,3.4vw,40px);line-height:1.08;margin:0 0 10px;letter-spacing:-.02em;text-wrap:balance}}
.lede{{margin:0;max-width:62ch;color:var(--muted);font-size:16px}}
.facts{{display:flex;flex-wrap:wrap;gap:10px;margin-top:20px;padding:0;list-style:none}}
.facts li{{background:var(--surface);border:1px solid var(--line);border-radius:var(--r);
  padding:7px 11px;font-family:ui-monospace,Menlo,monospace;font-size:12px;font-variant-numeric:tabular-nums}}
.facts b{{font-weight:600}} .facts span{{color:var(--muted)}}

.bar{{position:sticky;top:0;z-index:20;background:color-mix(in srgb,var(--ground) 88%,transparent);
  backdrop-filter:blur(9px);border-bottom:1px solid var(--line);margin-bottom:26px;
  display:flex;flex-wrap:wrap;gap:14px;align-items:center;padding:11px 0}}
.segs{{display:inline-flex;border:1px solid var(--line);border-radius:var(--r);overflow:hidden;background:var(--surface)}}
.seg{{appearance:none;border:0;background:transparent;color:var(--muted);cursor:pointer;
  padding:7px 13px;font:inherit;font-size:13px;border-right:1px solid var(--line-soft)}}
.seg:last-child{{border-right:0}}
.seg:hover{{color:var(--ink);background:var(--surface2)}}
.seg.on{{background:var(--ink);color:var(--surface)}}
.seg:focus-visible,.tile:focus-visible,.chip:focus-visible{{outline:2px solid var(--trunk);outline-offset:2px}}
.vdesc{{color:var(--muted);font-size:13px;flex:1;min-width:200px}}
.vd{{display:none}} .vd.on{{display:inline}}
.chips{{display:flex;gap:6px;flex-wrap:wrap}}
.chip{{appearance:none;cursor:pointer;font:inherit;font-size:12px;padding:5px 10px;border-radius:99px;
  border:1px solid var(--line);background:var(--surface);color:var(--muted)}}
.chip.on{{color:var(--ink);border-color:var(--ink)}}

.row{{margin-bottom:34px}}
.row-head{{display:flex;gap:14px;align-items:baseline;flex-wrap:wrap;margin-bottom:11px;
  padding-bottom:8px;border-bottom:1px solid var(--line-soft)}}
.row-head h3{{margin:0;font-size:16px;letter-spacing:-.01em}}
.row-head p{{margin:0;color:var(--muted);font-size:13px;max-width:70ch}}
.row-tiles{{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:12px}}
.tile{{position:relative;padding:0;border:1px solid var(--line);border-radius:var(--r);
  background:var(--surface2);cursor:pointer;overflow:hidden;line-height:0;transition:border-color .12s}}
.tile:hover{{border-color:var(--trunk)}}
.tile img{{width:100%;height:auto;display:block}}
.tile-tag{{position:absolute;left:0;bottom:0;display:flex;gap:7px;align-items:baseline;
  padding:4px 8px;background:color-mix(in srgb,#0F1115 72%,transparent);color:#E9ECF1;
  font-family:ui-monospace,Menlo,monospace;font-size:11px;line-height:1.5;border-top-right-radius:var(--r)}}
.tile-tag i{{font-style:normal;opacity:.72}}

.legend{{display:flex;flex-wrap:wrap;gap:16px;margin:0 0 26px;padding:12px 14px;
  background:var(--surface);border:1px solid var(--line);border-radius:var(--r);
  font-size:12px;color:var(--muted);font-family:ui-monospace,Menlo,monospace}}
.legend span{{display:inline-flex;align-items:center;gap:7px}}
.sw{{width:16px;height:3px;border-radius:2px;display:inline-block}}
.sw.d{{width:9px;height:9px;border-radius:50%}}

.notes{{margin-top:52px;border-top:1px solid var(--line);padding-top:30px}}
.notes h2{{font-size:19px;margin:0 0 6px;letter-spacing:-.01em}}
.notes .sub{{color:var(--muted);margin:0 0 20px;font-size:14px;max-width:66ch}}
.finds{{display:grid;gap:12px;grid-template-columns:repeat(auto-fit,minmax(280px,1fr))}}
.find{{background:var(--surface);border:1px solid var(--line);border-radius:var(--r);padding:15px 16px}}
.find h4{{margin:0 0 7px;font-size:14px}}
.find p{{margin:0;color:var(--muted);font-size:13.5px}}
.find .num{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.14em;
  text-transform:uppercase;color:var(--scarp);display:block;margin-bottom:7px}}
.find code{{font-family:ui-monospace,Menlo,monospace;font-size:12.5px;
  background:var(--surface2);padding:1px 4px;border-radius:2px;color:var(--ink)}}

dialog{{border:1px solid var(--line);border-radius:var(--r);background:var(--surface);color:var(--ink);
  padding:0;max-width:min(940px,94vw);width:100%}}
dialog::backdrop{{background:rgba(8,9,12,.66)}}
.dhead{{display:flex;justify-content:space-between;align-items:baseline;gap:16px;
  padding:15px 18px;border-bottom:1px solid var(--line)}}
.dhead h3{{margin:0;font-size:16px}}
.dhead button{{appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--muted);
  border-radius:var(--r);cursor:pointer;font:inherit;font-size:13px;padding:4px 10px}}
.dbody{{display:grid;grid-template-columns:minmax(0,1.15fr) minmax(230px,.85fr);gap:0}}
.dbody img{{width:100%;height:auto;display:block;border-right:1px solid var(--line)}}
.dstats{{padding:14px 18px;display:grid;grid-template-columns:1fr auto;gap:5px 14px;align-content:start;
  font-family:ui-monospace,Menlo,monospace;font-size:12.5px;font-variant-numeric:tabular-nums}}
.dstats dt{{color:var(--muted)}} .dstats dd{{margin:0;text-align:right}}
.dstats .sep{{grid-column:1/-1;border-top:1px solid var(--line-soft);margin:6px 0 2px}}
@media (max-width:720px){{ .dbody{{grid-template-columns:1fr}} .dbody img{{border-right:0;border-bottom:1px solid var(--line)}} }}
@media (prefers-reduced-motion:reduce){{ *{{transition:none!important;animation:none!important}} }}
</style>

<div class="wrap">
<header class="top">
  <p class="eyebrow">Attempt 4 · branch network-first · M1</p>
  <h1>Steps 0–3: the template, before any ground exists</h1>
  <p class="lede">The network is the primary object and the surface is a consequence of it. These four steps draw an
  archetype, choose a structure kind, place trunk mouths, and lay down the fields that will steer network growth.
  <strong>There is no heightfield here</strong> — that is the point, and it is why this viewer draws vector geometry and
  scalar fields rather than a hillshade.</p>
  <ul class="facts">
    <li><b>16</b> <span>tests</span></li>
    <li><b>3–4 ms</b> <span>per tile · budget 900</span></li>
    <li><b>6</b> <span>archetypes</span></li>
    <li><b>203</b> <span>corpus tiles behind the bands</span></li>
    <li><b>3</b> <span>defects this viewer caught</span></li>
  </ul>
</header>

<div class="bar">
  <div class="segs">{viewbtns}</div>
  <div class="vdesc">{viewdesc}</div>
  <div class="chips" id="chips"></div>
</div>

<div class="legend">
  <span><i class="sw" style="background:var(--trunk)"></i> trunk — width ∝ external inflow</span>
  <span><i class="sw d" style="background:var(--mouth)"></i> mouth</span>
  <span><i class="sw" style="background:var(--scarp)"></i> bed contact (provisional)</span>
  <span><i class="sw" style="background:var(--grain)"></i> structural grain</span>
  <span>blue → orange = tends low → tends high</span>
  <span>inner square = routable core</span>
</div>

{"".join(rows)}

<section class="notes">
  <h2>What the viewer caught that the tests did not</h2>
  <p class="sub">Each of these passed every assertion that existed at the time. They are the argument for rendering
  early rather than at the end of a milestone.</p>
  <div class="finds">
    <div class="find"><span class="num">Corrected</span><h4>Resistance was a barcode</h4>
      <p>Generating resistance as an <code>(x,y)</code> field gives uniform stripes across the map. Real beds outcrop
      where the land surface crosses them, so a contact's trace follows a <strong>contour</strong> — which is exactly why
      a dissected plateau reads as a staircase. Resistance is now a property of the rock <em>column</em>. The consequence
      is structural: the true map pattern cannot exist until step 6, so step 3 emits the column plus a hint on a proxy
      elevation.</p></div>
    <div class="find"><span class="num">Corrected</span><h4>A field used a fifth of its range</h4>
      <p><code>relief_pred</code> was documented <code>[-1,1]</code> and measured rms 0.21, range −0.52…0.62 — a
      summed-octave field never reaches its bounds. Every downstream consumer would have been silently mis-scaled.
      Now normalised symmetrically, so “tends high” and “tends low” stay balanced.</p></div>
    <div class="find"><span class="num">Corrected</span><h4>190× slower on the least structure</h4>
      <p>Scarp chaining was O(n²): piedmont cost <strong>555 ms/tile</strong> against hill country's 14, because
      piedmont's thin beds make the most contacts. A stage that gets slower the less structure it has is a bug, not a
      budget problem. Bucketed by endpoint cell: 2.9 ms — and the test suite went 81 s to 1.2 s.</p></div>
  </div>

  <h2 style="margin-top:34px">Expected failures</h2>
  <p class="sub">Every terrain metric. There is no surface to measure, and judging this stage on texture is a category
  error. What <em>is</em> checkable — and is, in tests and above — is that each archetype shows the features its record
  claims: piedmont draws no scarps, great plains and hill country do, river valley gets exactly one trunk carrying
  40–160 km² of external inflow, heathland and sandhills get neither, and sandhills is the only archetype with real
  grain anisotropy. Two honest gaps: sandhills' dune trains are step 4's aeolian branch and do not exist yet, and the
  bed contacts drawn here are provisional — the real scarps emerge at step 6.</p>
  <p class="sub"><strong>The relief field is identical down each column.</strong> All six archetypes share a seed's
  site, which isolates the archetype's contribution for review. In production a seed draws exactly one archetype, so
  this is a diagnostic property, not a leak.</p>
</section>
</div>

<dialog id="dlg">
  <div class="dhead"><h3 id="dtitle">—</h3><button id="dclose">Close</button></div>
  <div class="dbody"><img id="dimg" alt=""><dl class="dstats" id="dstats"></dl></div>
</dialog>

<script>
const VIEWS={json.dumps([v for v,_,_ in VIEWS])};
let view="all";
const tiles=[...document.querySelectorAll(".tile")];
function paint(){{ for(const t of tiles) t.querySelector("img").src=t.dataset[view]; }}
paint();
document.querySelectorAll(".seg").forEach(b=>b.addEventListener("click",()=>{{
  view=b.dataset.view;
  document.querySelectorAll(".seg").forEach(x=>x.classList.toggle("on",x===b));
  document.querySelectorAll(".vd").forEach(x=>x.classList.toggle("on",x.dataset.view===view));
  paint(); if(dlg.open) fill(cur);
}}));

const ARCHES={json.dumps(ARCHES)}, LABEL={json.dumps(LABEL)};
const chips=document.getElementById("chips");
const shown=new Set(ARCHES);
ARCHES.forEach(a=>{{
  const c=document.createElement("button");
  c.className="chip on"; c.textContent=LABEL[a]; c.type="button";
  c.addEventListener("click",()=>{{
    shown.has(a)?shown.delete(a):shown.add(a);
    if(!shown.size){{shown.add(a);}}
    c.classList.toggle("on",shown.has(a));
    document.querySelectorAll(".row").forEach(r=>r.style.display=shown.has(r.dataset.arch)?"":"none");
  }});
  chips.appendChild(c);
}});

const dlg=document.getElementById("dlg");
const F=[["structure","structure",v=>v],["relief","relief budget",v=>v.toFixed(1)+" m"],
 ["d2c","d2c target",v=>v.toFixed(0)+" m"],["SEP"],
 ["trunks","trunks",v=>v],["inflow","external inflow",v=>v.toFixed(1)+" km²"],
 ["edge","base edge",v=>v],["integration","integration",v=>v.toFixed(2)],["SEP"],
 ["scarps","bed contacts",v=>v],["beds","beds in stack",v=>v],
 ["stack_m","stack thickness",v=>v.toFixed(1)+" m"],["riser","riser",v=>v.toFixed(2)+" m"],
 ["dip_pct","dip",v=>v.toFixed(2)+" %"],["SEP"],
 ["aniso","grain anisotropy",v=>v.toFixed(3)],["widen","floor widen",v=>v.toFixed(2)+"×"],
 ["terraces","terrace treads",v=>v],["asym","terrace asymmetry",v=>v.toFixed(2)]];
let cur=null;
function fill(t){{
  cur=t; const s=JSON.parse(t.dataset.stats);
  document.getElementById("dtitle").textContent=LABEL[s.arch]+" · seed "+s.seed;
  const img=document.getElementById("dimg"); img.src=t.dataset[view]; img.alt=LABEL[s.arch]+" seed "+s.seed;
  const dl=document.getElementById("dstats"); dl.innerHTML="";
  for(const f of F){{
    if(f[0]==="SEP"){{ const d=document.createElement("div"); d.className="sep"; dl.appendChild(d); continue; }}
    const dt=document.createElement("dt"); dt.textContent=f[1];
    const dd=document.createElement("dd"); dd.textContent=f[2](s[f[0]]);
    dl.append(dt,dd);
  }}
}}
tiles.forEach(t=>t.addEventListener("click",()=>{{fill(t);dlg.showModal();}}));
document.getElementById("dclose").addEventListener("click",()=>dlg.close());
dlg.addEventListener("click",e=>{{if(e.target===dlg)dlg.close();}});
</script>
'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
