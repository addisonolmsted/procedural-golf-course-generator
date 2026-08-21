import base64, json, pathlib, html
SRC = pathlib.Path("out/n2_viewer")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/n2_viewer.html")
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
        img = uri(f"{a}_{s}_net.jpg")
        tiles.append(f'''<button class="tile" data-img="{img}" data-stats='{html.escape(json.dumps(r))}'>
<img alt="{LABEL[a]} seed {s}" loading="lazy" src="{img}"><span class="tile-tag"><b>{s}</b><i>{r['trunks']}T · {'Y' if r.get('joins',0)>0 else str(r['trunks'])+'T'}</i></span></button>''')
    rows.append(f'''<section class="row"><header class="row-head"><h3>{LABEL[a]}</h3></header>
<div class="row-tiles">{"".join(tiles)}</div></section>''')

DOC = f'''<title>N2 · Tributaries</title>
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
  <p class="eyebrow">Attempt 4 · network restart · phase N2 — tier-2 tributaries</p>
  <h1>Tributaries: down from the high ground, onto the trunks</h1>
  <p class="lede">Each tributary starts on a territory peak — the high ground farthest from any channel — and walks
  DOWNHILL on a proto-elevation field whose valley floors are the channels already placed, with a 300–700 m meander.
  Convergence is structural, not steered: a descent ends on a channel because there is nowhere else downhill to go.
  Each placed tributary joins the field before the next is routed, so later ones descend into earlier ones — the
  stem-with-side-branches structure that gives real networks their bifurcation ratio. The final ~150 m of every
  approach is an analytic curve that enters at the drawn junction angle by construction.</p>
  <div class="legend">
    <span><i class="grad" style="background:linear-gradient(90deg,#F2F2EE,#6E37AA)"></i> elevation along the line: pale = base level → violet = upstream</span>
    <span>▸ chevrons point DOWNSTREAM — a Y is two rivers merging, not one splitting</span>
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
    <thead><tr><th>check</th><th>result · corpus band</th></tr></thead>
    <tbody>
      <tr><td>junction angle p50</td><td>44.4–45.2° · band 37–45° ✓</td></tr>
      <tr><td>orthogonal junctions (&gt;80°)</td><td>12.9–13.0% · band 8.5–13.4% ✓</td></tr>
      <tr><td>approach geometry (round 2, user review)</td><td>the ENTRY was acute but the APPROACH was orthogonal — walkers descended a radial field and got hooked by the analytic tail, which the 50 m metric cannot see and the eye integrates. Fixed in the field, per the review suggestion: the proto reference point shifts up to 420 m DOWNSTREAM of the nearest channel point (fading over 750 m), so the valley floor slopes down-valley and descent curves into the trunk over its whole lower course.</td></tr>
      <tr><td>near-parallel (reach-based, declared policy)</td><td>0.0–0.04% · band 1.3–3.1% — BELOW band, expected at N2: one tier only; binds at N3</td></tr>
      <tr><td>walker survival</td><td>227 of 228 typical; deaths counted by cause (off-tile / exhausted / stub), never hidden</td></tr>
      <tr><td>monotone profiles</td><td>every tributary climbs from its junction elevation; asserted</td></tr>
      <tr><td>acyclic by construction</td><td>a tributary attaches only to channels placed before it, so every chain terminates on a trunk</td></tr>
    </tbody>
</table></div>
  <p class="sub"><strong>What to look for:</strong> tributaries should read as smooth curves off the high ground
  (orange) converging into the trunks at acute angles, with later ones joining earlier ones into small trees. The
  meander should be visible but gentler than the trunks'. Density is deliberately sparse — this is the MAJOR
  tributary tier only; the minor tiers that fill the ground to corpus density are N3. If any junction reads as a
  T-bone, or a tributary as nervous or straight, say so now.</p>
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
