import base64, json, pathlib, html
SRC = pathlib.Path("out/n1_viewer")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/n1_viewer.html")
stats = json.loads((SRC/"stats.json").read_text())
ARCHES = ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]
LABEL = {"piedmont":"Piedmont","great_plains":"Great Plains","river_valley":"River Valley",
         "hill_country":"Hill Country","heathland":"Heathland","sandhills":"Sandhills"}
seeds = sorted({r["seed"] for r in stats})
by = {(r["arch"], r["seed"]): r for r in stats}
def uri(p):
    f = SRC/p
    return "data:image/jpeg;base64," + base64.b64encode(f.read_bytes()).decode() if f.exists() else ""

rows=[]
for a in ARCHES:
    tiles=[]
    for s in seeds:
        r = by[(a,s)]
        img = uri(f"{a}_{s}_trunk.jpg")
        tiles.append(f'''<button class="tile" data-img="{img}" data-stats='{html.escape(json.dumps(r))}'>
<img alt="{LABEL[a]} seed {s}" loading="lazy" src="{img}"><span class="tile-tag"><b>{s}</b><i>{r['trunks']}T · {'Y' if r.get('joins',0)>0 else str(r['trunks'])+'T'}</i></span></button>''')
    rows.append(f'''<section class="row"><header class="row-head"><h3>{LABEL[a]}</h3></header>
<div class="row-tiles">{"".join(tiles)}</div></section>''')

DOC = f'''<title>N1 · Trunks</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{{--ground:#E7E9EC;--surface:#FFF;--surface2:#F1F4F7;--ink:#171A1F;--muted:#5A616C;
 --line:#CDD3DA;--line-soft:#E0E4E9;--trunk:#2F7FC4;--ok:#2F7D52;--r:3px}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{
 --ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;
 --line:#2A2F38;--line-soft:#22262D;--trunk:#56B4FF;--ok:#5FBF86}}}}
:root[data-theme="dark"]{{--ground:#0F1115;--surface:#181B21;--surface2:#20242B;--ink:#E9ECF1;--muted:#939BA7;
 --line:#2A2F38;--line-soft:#22262D;--trunk:#56B4FF;--ok:#5FBF86}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--ground);color:var(--ink);
 font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;-webkit-font-smoothing:antialiased}}
.wrap{{max-width:1280px;margin:0 auto;padding:0 24px 72px}}
header.top{{border-bottom:1px solid var(--line);margin-bottom:26px;padding:40px 0 22px}}
.eyebrow{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 10px}}
h1{{font-size:clamp(26px,3.4vw,40px);line-height:1.08;margin:0 0 10px;letter-spacing:-.02em;text-wrap:balance}}
.lede{{margin:0;max-width:64ch;color:var(--muted);font-size:16px}}
.legend{{display:flex;flex-wrap:wrap;gap:16px;margin:20px 0 0;padding:12px 14px;background:var(--surface);
 border:1px solid var(--line);border-radius:var(--r);font-size:12px;color:var(--muted);font-family:ui-monospace,Menlo,monospace}}
.legend span{{display:inline-flex;align-items:center;gap:7px}}
.sw{{width:9px;height:9px;border-radius:50%;display:inline-block}}
.grad{{width:44px;height:8px;border-radius:2px;background:linear-gradient(90deg,#EBF0FF,#4696FF);display:inline-block}}
.row{{margin-bottom:30px}}
.row-head{{margin-bottom:10px;padding-bottom:7px;border-bottom:1px solid var(--line-soft)}}
.row-head h3{{margin:0;font-size:16px}}
.row-tiles{{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:12px}}
.tile{{position:relative;padding:0;border:1px solid var(--line);border-radius:var(--r);background:var(--surface2);cursor:pointer;overflow:hidden;line-height:0}}
.tile:hover{{border-color:var(--trunk)}}
.tile:focus-visible{{outline:2px solid var(--trunk);outline-offset:2px}}
.tile img{{width:100%;height:auto;display:block}}
.tile-tag{{position:absolute;left:0;bottom:0;display:flex;gap:7px;padding:4px 8px;
 background:color-mix(in srgb,#0F1115 74%,transparent);color:#E9ECF1;font-family:ui-monospace,Menlo,monospace;font-size:11px;line-height:1.5;border-top-right-radius:var(--r)}}
.tile-tag i{{font-style:normal;opacity:.72}}
.notes{{margin-top:44px;border-top:1px solid var(--line);padding-top:26px}}
.notes h2{{font-size:19px;margin:0 0 6px}}
.sub{{color:var(--muted);margin:0 0 14px;font-size:14px;max-width:68ch}}
.tblwrap{{overflow-x:auto;margin:0 0 14px;border:1px solid var(--line);border-radius:var(--r);background:var(--surface)}}
table{{border-collapse:collapse;width:100%;min-width:520px;font-size:13px;font-family:ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums}}
th{{text-align:left;font-weight:600;color:var(--muted);font-size:11px;letter-spacing:.1em;text-transform:uppercase;padding:9px 12px;border-bottom:1px solid var(--line)}}
td{{padding:8px 12px;border-bottom:1px solid var(--line-soft)}} tr:last-child td{{border-bottom:0}}
dialog{{border:1px solid var(--line);border-radius:var(--r);background:var(--surface);color:var(--ink);padding:0;max-width:min(860px,94vw);width:100%}}
dialog::backdrop{{background:rgba(8,9,12,.66)}}
.dhead{{display:flex;justify-content:space-between;padding:15px 18px;border-bottom:1px solid var(--line)}}
.dhead h3{{margin:0;font-size:16px}}
.dhead button{{appearance:none;border:1px solid var(--line);background:var(--surface2);color:var(--muted);border-radius:var(--r);cursor:pointer;font:inherit;font-size:13px;padding:4px 10px}}
.dbody{{display:grid;grid-template-columns:minmax(0,1.2fr) minmax(200px,.8fr)}}
.dbody img{{width:100%;height:auto;display:block;border-right:1px solid var(--line)}}
.dstats{{padding:14px 18px;display:grid;grid-template-columns:1fr auto;gap:5px 14px;align-content:start;font-family:ui-monospace,Menlo,monospace;font-size:12.5px}}
.dstats dt{{color:var(--muted)}} .dstats dd{{margin:0;text-align:right}}
@media (max-width:720px){{.dbody{{grid-template-columns:1fr}} .dbody img{{border-right:0;border-bottom:1px solid var(--line)}}}}
@media (prefers-reduced-motion:reduce){{*{{transition:none!important}}}}
</style>
<div class="wrap">
<header class="top">
  <p class="eyebrow">Attempt 4 · network restart · phase N1 — trunks only</p>
  <h1>Trunks: end-to-end, long-wave, and downhill all the way</h1>
  <p class="lede">Round 2, after review. <strong>Every trunk is now a through-river</strong> — a trunk-scale valley
  is carved by a river whose catchment far exceeds the window (real borders carry only 0.29 trunk-class outlets per
  edge), so headwater starts are reserved for the tributaries. Counts dialled down: mostly one, sometimes two — which
  may <strong>converge at an in-tile confluence</strong> — very rarely three. A 400 m corridor floor keeps separate
  trunks apart along their whole length, not just at the mouths.</p>
  <div class="legend">
    <span><i class="grad"></i> elevation along the line: pale = base level → blue = head</span>
    <span><i class="sw" style="background:#E8B93E"></i> mouth</span>
    <span><i class="sw" style="background:#78EBAA"></i> entry on a far edge</span>
    <span><i class="sw" style="background:#4B5AC8;border:2px solid #FFF"></i> confluence junction</span>
    <span>background: relief field, blue low → orange high</span>
  </div>
</header>
{"".join(rows)}
<section class="notes">
  <h2>Measured at the gate</h2>
  <div class="tblwrap"><table>
    <thead><tr><th>check</th><th>result</th></tr></thead>
    <tbody>
      <tr><td>min curvature radius (asserted in tests)</td><td>150 m — enforced by iterated smoothing, not hoped for</td></tr>
      <tr><td>long profile</td><td>strictly monotone, zero at the mouth; all trunks drop gently (4–12 % of relief budget — mature rivers, not headwater streams); a joining trunk's profile starts at the junction elevation</td></tr>
      <tr><td>trunk–trunk crossings</td><td>0 — disjoint far-end sectors + a 400 m whole-path corridor floor + rebuild ladder; an unplaceable trunk is dropped, never overlapped</td></tr>
      <tr><td>convergence</td><td>a secondary joins the primary at 45 % probability — junction chosen where the primary's local tangent faces the entry (mid-bend junctions hairpinned; measured 2 m radius before the fix), approach at 35–50° off the downstream tangent</td></tr>
      <tr><td>600 m window sinuosity</td><td>1.03–1.04 (corpus pooled band 1.06–1.10 — trunks are deliberately the straightest members; the pooled band becomes binding at N3 when tributaries dominate)</td></tr>
      <tr><td>1500 m window sinuosity</td><td>1.07–1.14 — the long-wave meander lives here</td></tr>
      <tr><td>river valley</td><td>always exactly 1 trunk, always edge-to-edge</td></tr>
    </tbody>
  </table></div>
  <p class="sub"><strong>What to look for:</strong> trunks should read as one smooth river each — no doubling back, no
  wiggle at the 100 m scale, bends on the kilometre scale. Through-rivers (green head) should cross the whole tile;
  headwater trunks (red head) should start on or near orange ground and reach the mouth through blue. If any trunk
  reads as straight-with-a-scallop or as nervous, say so — amplitude and wavelength are one draw each and cheap to
  retune now, expensive after tributaries hang off them.</p>
</section>
</div>
<dialog id="dlg"><div class="dhead"><h3 id="dtitle">—</h3><button id="dclose">Close</button></div>
<div class="dbody"><img id="dimg" alt=""><dl class="dstats" id="dstats"></dl></div></dialog>
<script>
const LABEL={json.dumps(LABEL)};
const F=[["trunks","trunks",v=>v],["joins","converging",v=>v],["sin600","sinuosity 600 m",v=>v.toFixed(3)],
 ["min_rad","min radius m",v=>v],["len_km","total length km",v=>v.toFixed(2)],["head_z","head elevation m",v=>v.toFixed(1)]];
const dlg=document.getElementById("dlg");
document.querySelectorAll(".tile").forEach(t=>t.addEventListener("click",()=>{{
 const s=JSON.parse(t.dataset.stats);
 document.getElementById("dtitle").textContent=LABEL[s.arch]+" · seed "+s.seed;
 const i=document.getElementById("dimg"); i.src=t.dataset.img; i.alt=LABEL[s.arch]+" seed "+s.seed;
 const dl=document.getElementById("dstats"); dl.innerHTML="";
 for(const f of F){{const dt=document.createElement("dt");dt.textContent=f[1];
  const dd=document.createElement("dd");dd.textContent=f[2](s[f[0]]);dl.append(dt,dd);}}
 dlg.showModal();
}}));
document.getElementById("dclose").addEventListener("click",()=>dlg.close());
dlg.addEventListener("click",e=>{{if(e.target===dlg)dlg.close();}});
</script>
'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
