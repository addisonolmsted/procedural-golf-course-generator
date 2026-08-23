#!/usr/bin/env python3
"""Build the Carolina Sandhills review page (self-contained HTML)."""
import base64, json, pathlib, sys

SRC = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/sand_review")
DST = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else "/tmp/sand_review.html")
rows = json.loads((SRC / "rows.json").read_text())

BLOCKS = {"Sandhills Game Land, NC": (35.05, -79.55), "Fort Bragg, NC": (35.20, -79.26),
          "Carolina Sandhills NWR, SC": (34.54, -80.22), "Sand Hills State Forest, SC": (34.64, -80.10)}
for r in rows:
    r["block"] = min(BLOCKS, key=lambda k: (BLOCKS[k][0]-r["lat"])**2 + (BLOCKS[k][1]-r["lon"])**2)
    r["uri"] = "data:image/jpeg;base64," + base64.b64encode((SRC / r["img"]).read_bytes()).decode()
rows.sort(key=lambda r: (r["block"], r["tile"]))

CSS = """
:root{
  --ground:#e8eaec; --surface:#fbfcfc; --surface-2:#f0f2f3; --line:#cdd3d7;
  --ink:#14171a; --ink-2:#3c454c; --muted:#5c666e;
  --accent:#0f5d66; --accent-soft:#d5e6e7;
  --keep:#2c6e49; --keep-soft:#d8e9de; --cull:#9c3527; --cull-soft:#f2dcd8;
  --flag:#8a6114; --shadow:0 1px 2px rgba(20,23,26,.07),0 4px 14px rgba(20,23,26,.05);
  --serif:ui-serif,Georgia,"Iowan Old Style",Palatino,serif;
  --sans:ui-sans-serif,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
  --mono:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,monospace;
}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
  --ground:#131619; --surface:#1b2024; --surface-2:#222930; --line:#333c44;
  --ink:#e7ebee; --ink-2:#c0c8cf; --muted:#8d979f;
  --accent:#5fb6bf; --accent-soft:#17323a;
  --keep:#6cc08d; --keep-soft:#17301f; --cull:#e0796a; --cull-soft:#331a16;
  --flag:#d3a844; --shadow:0 1px 2px rgba(0,0,0,.4),0 4px 16px rgba(0,0,0,.3);
}}
:root[data-theme="dark"]{
  --ground:#131619; --surface:#1b2024; --surface-2:#222930; --line:#333c44;
  --ink:#e7ebee; --ink-2:#c0c8cf; --muted:#8d979f;
  --accent:#5fb6bf; --accent-soft:#17323a;
  --keep:#6cc08d; --keep-soft:#17301f; --cull:#e0796a; --cull-soft:#331a16;
  --flag:#d3a844; --shadow:0 1px 2px rgba(0,0,0,.4),0 4px 16px rgba(0,0,0,.3);
}
*{box-sizing:border-box}
body{margin:0;background:var(--ground);color:var(--ink);font-family:var(--sans);
  font-size:16px;line-height:1.6;-webkit-font-smoothing:antialiased}
.wrap{max-width:1220px;margin:0 auto;padding:44px 22px 100px}
header{display:flex;flex-direction:column;gap:10px;padding-bottom:26px;border-bottom:2px solid var(--ink)}
.eyebrow{font-family:var(--mono);font-size:11.5px;letter-spacing:.14em;text-transform:uppercase;color:var(--accent)}
h1{font-family:var(--serif);font-weight:600;font-size:clamp(29px,4.2vw,44px);line-height:1.1;
  margin:0;text-wrap:balance;letter-spacing:-.012em}
.lede{max-width:66ch;color:var(--ink-2);font-size:17px;margin:0}
h2{font-family:var(--serif);font-weight:600;font-size:24px;margin:0;letter-spacing:-.008em}
section{margin-top:44px;display:flex;flex-direction:column;gap:18px}
p{margin:0;max-width:70ch}
.note{color:var(--muted);font-size:14.5px}
code{font-family:var(--mono);font-size:.88em;background:var(--surface-2);
  padding:1px 5px;border-radius:3px;border:1px solid var(--line)}
.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(310px,1fr));gap:18px}
.card{background:var(--surface);border:1px solid var(--line);border-radius:5px;
  padding:18px 20px;box-shadow:var(--shadow);display:flex;flex-direction:column;gap:11px}
.card h3{margin:0;font-size:13px;font-family:var(--mono);letter-spacing:.1em;text-transform:uppercase}
.card.keep h3{color:var(--keep)} .card.cull h3{color:var(--cull)}
ul{margin:0;padding-left:0;list-style:none;display:flex;flex-direction:column;gap:11px}
li{display:grid;grid-template-columns:auto 1fr;gap:10px;font-size:15px;color:var(--ink-2);line-height:1.5}
li b{color:var(--ink);font-weight:600}
.tick{font-family:var(--mono);font-weight:700;line-height:1.55}
.card.keep .tick{color:var(--keep)} .card.cull .tick{color:var(--cull)}
.bar{position:sticky;top:0;z-index:20;background:var(--surface);border:1px solid var(--line);
  border-radius:5px;padding:12px 16px;display:flex;flex-wrap:wrap;gap:14px;align-items:center;
  box-shadow:var(--shadow);margin-top:44px}
.tally{font-family:var(--mono);font-size:13px;display:flex;gap:14px;align-items:center;
  font-variant-numeric:tabular-nums}
.tally b{font-weight:700} .t-keep{color:var(--keep)} .t-cull{color:var(--cull)}
.spacer{flex:1 1 auto}
button{font-family:var(--sans);font-size:13.5px;font-weight:500;padding:7px 13px;
  border-radius:4px;border:1px solid var(--line);background:var(--surface-2);
  color:var(--ink);cursor:pointer;transition:background .13s,border-color .13s}
button:hover{border-color:var(--accent)}
button:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
button.primary{background:var(--accent);border-color:var(--accent);color:var(--surface)}
:root[data-theme="dark"] button.primary,
:root:not([data-theme="light"]) button.primary{color:#0d1114}
select{font-family:var(--sans);font-size:13.5px;padding:7px 9px;border-radius:4px;
  border:1px solid var(--line);background:var(--surface-2);color:var(--ink)}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(330px,1fr));gap:20px;margin-top:22px}
.tile{background:var(--surface);border:1px solid var(--line);border-radius:5px;overflow:hidden;
  box-shadow:var(--shadow);display:flex;flex-direction:column;
  border-left:4px solid var(--line);transition:border-color .13s}
.tile.keep{border-left-color:var(--keep)} .tile.cull{border-left-color:var(--cull);opacity:.62}
.tile img{width:100%;display:block;aspect-ratio:1;object-fit:cover;background:var(--surface-2)}
.meta{padding:12px 14px;display:flex;flex-direction:column;gap:9px}
.id{display:flex;justify-content:space-between;align-items:baseline;gap:10px}
.id .t{font-family:var(--mono);font-size:13.5px;font-weight:600}
.id .g{font-family:var(--mono);font-size:11.5px;color:var(--muted)}
.blk{font-size:12px;color:var(--muted)}
.stats{display:grid;grid-template-columns:repeat(3,1fr);gap:7px 10px;
  font-family:var(--mono);font-size:11.5px;font-variant-numeric:tabular-nums}
.stat{display:flex;flex-direction:column;gap:1px}
.stat span{color:var(--muted);font-size:9.5px;letter-spacing:.07em;text-transform:uppercase}
.chips{display:flex;flex-wrap:wrap;gap:5px}
.chip{font-family:var(--mono);font-size:10px;letter-spacing:.05em;text-transform:uppercase;
  padding:2px 7px;border-radius:99px;border:1px solid var(--line);color:var(--muted)}
.chip.ok{color:var(--keep);border-color:var(--keep);background:var(--keep-soft)}
.chip.no{color:var(--cull);border-color:var(--cull);background:var(--cull-soft)}
.chip.fl{color:var(--flag);border-color:var(--flag)}
.acts{display:grid;grid-template-columns:1fr 1fr;gap:7px;padding:0 14px 13px}
.acts button{width:100%}
.tile.keep .bk{background:var(--keep);border-color:var(--keep);color:var(--surface)}
.tile.cull .bc{background:var(--cull);border-color:var(--cull);color:var(--surface)}
:root[data-theme="dark"] .tile.keep .bk,:root:not([data-theme="light"]) .tile.keep .bk,
:root[data-theme="dark"] .tile.cull .bc,:root:not([data-theme="light"]) .tile.cull .bc{color:#0d1114}
dialog{border:1px solid var(--line);border-radius:6px;background:var(--surface);color:var(--ink);
  max-width:660px;width:92%;padding:0;box-shadow:var(--shadow)}
dialog::backdrop{background:rgba(10,14,17,.55)}
.dlg{padding:20px 22px;display:flex;flex-direction:column;gap:13px}
textarea{width:100%;height:230px;font-family:var(--mono);font-size:12px;background:var(--surface-2);
  color:var(--ink);border:1px solid var(--line);border-radius:4px;padding:11px;resize:vertical}
.legend{display:flex;flex-wrap:wrap;gap:16px;font-family:var(--mono);font-size:11.5px;color:var(--muted)}
.legend b{color:var(--ink-2);font-weight:600}
@media (prefers-reduced-motion:reduce){*{transition:none!important;animation:none!important}}
"""

GUIDE_KEEP = [
    ("Broad, flat-topped sand interfluves", "The pale high ground. This is what a Carolina course is routed on, and the mode's C1 stage builds it. Wide and gently domed, not narrow ridges."),
    ("A dendritic web of shallow creek bottoms", "The green low ground: low-gradient blackwater branches with soft, concave side slopes. Many fine tributaries is right — sand drains readily and the network is fine-textured."),
    ("Carolina bays", "Elliptical shallow depressions, long axis NW–SE, often with a raised sand rim on the southeast side. A signature landform, and wanted — they become the C6 stage."),
    ("Relict sand ridges on the interfluves", "Low, soft, wind-built swells riding on top of the high ground. This is the aeolian mantle that ties the Carolina mode to the Nebraska one."),
]
GUIDE_CULL = [
    ("Agriculture OSM did not tag", "Center-pivot arcs, rectilinear terracing, suspiciously flat rectangles with straight edges. Old fields often carry no landuse polygon, so the automatic screen cannot see them — this is the main thing your eye adds."),
    ("Grading and excavation", "Borrow pits, quarry benches, building pads, embankments with a constant-height face. Graded ground is real topography to every detector downstream, so a pit becomes a 'basin' and a cut edge becomes a 'scarp'."),
    ("Impoundments", "Farm ponds and reservoirs read as dead-flat panels, usually with one straight dam edge. A natural bay is elliptical and shallow; a pond is flat and has a hard rim."),
    ("Ground disturbance on the Fort Bragg blocks", "Tank trails, cleared ranges, cratering, straight cut corridors. Ordinary forest roads and firebreaks are normal for this landscape — cull only when the disturbance dominates the tile."),
    ("Soapy, detail-free texture", "3DEP mosaics best-available sources; a 10 m-source tile resampled to 2 m looks plausible at a glance but has no fine texture. Compare against its neighbours — if one tile is conspicuously smoother, it is the source and not the ground."),
    ("Wrong archetype", "Steep, V-shaped, deeply incised valleys are piedmont, not sandhills. These blocks sit close to the Fall Line and a tile can fall on the wrong side of it."),
]

def stat(label, val): return f'<div class="stat"><span>{label}</span>{val}</div>'

cards = []
for r in rows:
    chips = [f'<span class="chip {"ok" if r["proxy_pass"] else "no"}">proxy {"pass" if r["proxy_pass"] else "fail"}</span>']
    if r["road"] > 0.30: chips.append('<span class="chip fl">linear cuts</span>')
    if r["leveled"] > 0.10: chips.append('<span class="chip fl">flat patches</span>')
    if r["developed"] is not None and r["developed"] > 0.012:
        chips.append(f'<span class="chip fl">built {r["developed"]*100:.1f}%</span>')
    cards.append(f'''<article class="tile" data-tile="{r['tile']}" data-block="{r['block']}"
 data-a="{r['A']:.4f}" data-relief="{r['relief']:.2f}" data-road="{r['road']:.4f}" data-flag="{int(r['road']>0.30 or r['leveled']>0.10 or (r['developed'] or 0)>0.012)}">
<img src="{r['uri']}" alt="Tinted hillshade of tile {r['tile']}" loading="lazy">
<div class="meta">
<div class="id"><span class="t">{r['tile']}</span><span class="g">{r['lat']:.2f}&deg;N {abs(r['lon']):.2f}&deg;W</span></div>
<div class="blk">{r['block']}</div>
<div class="stats">
{stat('relief', f"{r['relief']:.0f} m")}{stat('calm', f"{r['cap']:.2f}")}{stat('contig', f"{r['contig']:.0f} ha")}
{stat('orient A', f"{r['A']:.2f}")}{stat('&lambda;', f"{r['lam']:.0f} m")}{stat('slope p95', f"{r['slope_p95']:.1f}&deg;")}
</div>
<div class="chips">{''.join(chips)}</div>
</div>
<div class="acts"><button class="bk" data-act="keep">Keep</button><button class="bc" data-act="cull">Cull</button></div>
</article>''')

html = f'''<title>Carolina Sandhills &mdash; corpus review</title>
<style>{CSS}</style>
<div class="wrap">
<header>
  <div class="eyebrow">Attempt 5 &middot; Phase 1 &middot; human review gate</div>
  <h1>34 Carolina Sandhills tiles, awaiting a keep-or-cull</h1>
  <p class="lede">These survived the automatic screen. <code>extract-v2</code> only reads tiles a
  person has marked kept, so the corpus cannot close until every one of these has a decision.
  Judge each tile twice: <em>is this ground undisturbed</em>, and <em>is this ground Carolina Sandhills</em>.</p>
</header>

<section>
  <h2>What the automatic screen already did &mdash; and what it cannot see</h2>
  <p>76 tiles were fetched across four protected blocks; 65 arrived. The OSM screen measured roads,
  buildings and disturbed land on each and culled 31 that exceeded 2&nbsp;% of tile area &mdash; about
  18&nbsp;ha. The 34 below are what is left, and they are clean <em>as far as OpenStreetMap knows</em>.</p>
  <p class="note">That is the gap your eye fills. OSM has excellent road and building coverage and
  almost no coverage of abandoned agriculture, old grading, borrow pits or historic land levelling.
  A tile can measure 0.0&nbsp;% developed and still be a former field.</p>
</section>

<section>
  <h2>What to look for</h2>
  <div class="cols">
    <div class="card keep"><h3>Keep &mdash; the character we want</h3><ul>
    {''.join(f'<li><span class="tick">+</span><span><b>{t}.</b> {d}</span></li>' for t, d in GUIDE_KEEP)}
    </ul></div>
    <div class="card cull"><h3>Cull &mdash; disqualifying</h3><ul>
    {''.join(f'<li><span class="tick">&minus;</span><span><b>{t}.</b> {d}</span></li>' for t, d in GUIDE_CULL)}
    </ul></div>
  </div>
  <p class="note"><b>When in doubt, cull.</b> The corpus is a measurement instrument: one graded tile
  quietly moves a published band, and there is no downstream check that would catch it. We have 34
  candidates against a target of about 30, so a marginal tile is cheap to lose and expensive to keep.</p>
</section>

<section>
  <h2>Reading the numbers</h2>
  <div class="legend">
    <span><b>relief</b> p95&minus;p5, metres</span>
    <span><b>calm</b> fraction under the 8&nbsp;% fairway cap</span>
    <span><b>contig</b> largest connected calm area</span>
    <span><b>orient A</b> spectral orientation order, 0&nbsp;isotropic &rarr; 1&nbsp;one axis</span>
    <span><b>&lambda;</b> dominant wavelength</span>
    <span><b>slope p95</b> steepness of the steepest twentieth</span>
  </div>
  <p class="note">These describe the tile; they do not decide it. A tile that fails the golf proxy is
  still a perfectly good <em>corpus</em> tile &mdash; the proxy measures whether ground is routable,
  not whether it is real. Chips marked in amber are advisory only: <em>linear cuts</em> flags
  orientation-dominant gradients, which firebreaks and forest roads both produce, and
  <em>flat patches</em> flags near-zero fine texture, which is how levelled fields read.</p>
</section>

<div class="bar">
  <div class="tally"><span>Reviewed <b id="n-done">0</b>/<b>{len(rows)}</b></span>
  <span class="t-keep">Keep <b id="n-keep">0</b></span><span class="t-cull">Cull <b id="n-cull">0</b></span></div>
  <div class="spacer"></div>
  <select id="sort" aria-label="Sort tiles">
    <option value="block">Sort: by block</option><option value="a">Sort: orientation A</option>
    <option value="relief">Sort: relief</option><option value="road">Sort: linear cuts</option>
  </select>
  <button id="flagged">Flagged only</button>
  <button id="export" class="primary">Copy decisions</button>
</div>

<div class="grid" id="grid">{''.join(cards)}</div>
</div>

<dialog id="dlg"><div class="dlg">
  <h2>Decisions</h2>
  <p class="note">Paste this back into the session. Kept tiles go to <code>review_v2.json</code>;
  culled tiles go to <code>exclude.json</code> with a <code>human:</code> reason.</p>
  <textarea id="out" readonly></textarea>
  <div style="display:flex;gap:8px;justify-content:flex-end">
    <button id="copy" class="primary">Copy to clipboard</button><button id="close">Close</button></div>
</div></dialog>

<script>
const S = {{}}, grid = document.getElementById('grid');
const upd = () => {{
  const v = Object.values(S);
  document.getElementById('n-done').textContent = v.length;
  document.getElementById('n-keep').textContent = v.filter(x => x === 'keep').length;
  document.getElementById('n-cull').textContent = v.filter(x => x === 'cull').length;
}};
grid.addEventListener('click', e => {{
  const b = e.target.closest('button[data-act]'); if (!b) return;
  const t = b.closest('.tile'), id = t.dataset.tile, a = b.dataset.act;
  if (S[id] === a) {{ delete S[id]; t.classList.remove('keep', 'cull'); }}
  else {{ S[id] = a; t.classList.toggle('keep', a === 'keep'); t.classList.toggle('cull', a === 'cull'); }}
  upd();
}});
document.getElementById('sort').addEventListener('change', e => {{
  const k = e.target.value;
  [...grid.children].sort((p, q) => k === 'block'
      ? (p.dataset.block + p.dataset.tile).localeCompare(q.dataset.block + q.dataset.tile)
      : (+q.dataset[k]) - (+p.dataset[k])).forEach(n => grid.appendChild(n));
}});
let only = false;
document.getElementById('flagged').addEventListener('click', e => {{
  only = !only; e.target.textContent = only ? 'Show all' : 'Flagged only';
  [...grid.children].forEach(n => n.style.display = (!only || n.dataset.flag === '1') ? '' : 'none');
}});
document.getElementById('export').addEventListener('click', () => {{
  const keep = [], cull = [];
  [...grid.children].forEach(n => {{
    const s = S[n.dataset.tile];
    if (s === 'keep') keep.push(n.dataset.tile); else if (s === 'cull') cull.push(n.dataset.tile);
  }});
  const undecided = [...grid.children].map(n => n.dataset.tile).filter(t => !S[t]);
  document.getElementById('out').value = JSON.stringify(
    {{archetype: 'sandhills_nc', keep, cull, undecided}}, null, 2);
  document.getElementById('dlg').showModal();
}});
document.getElementById('copy').addEventListener('click', async () => {{
  const t = document.getElementById('out');
  try {{ await navigator.clipboard.writeText(t.value); }} catch (_) {{ t.select(); document.execCommand('copy'); }}
  document.getElementById('copy').textContent = 'Copied';
  setTimeout(() => document.getElementById('copy').textContent = 'Copy to clipboard', 1400);
}});
document.getElementById('close').addEventListener('click', () => document.getElementById('dlg').close());
</script>'''

DST.write_text(html)
print(f"wrote {DST}  ({DST.stat().st_size/1e6:.2f} MB, {len(rows)} tiles)")
