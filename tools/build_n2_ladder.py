import base64, json, pathlib
SRC = pathlib.Path("out/n2_ladder")
OUT = pathlib.Path("/private/tmp/claude-501/-Users-davisolmsted-Documents-GitHub-GolfProceduralGenerator/7b548523-556f-42a3-ab16-a073d3d23991/scratchpad/n2_ladder.html")
rungs = json.loads((SRC/"rungs.json").read_text())
TILES = [("piedmont",1,"Piedmont · 1"),("piedmont",2,"Piedmont · 2"),
         ("hill_country",2,"Hill Country · 2 (convergent pair)"),("river_valley",1,"River Valley · 1")]
def uri(p):
    f = SRC/p
    return "data:image/jpeg;base64," + base64.b64encode(f.read_bytes()).decode() if f.exists() else ""

rows=[]
for r in rungs:
    ri = r["rung"]
    cells = "".join(
        f'<figure><img src="{uri(f"r{ri}_{a}_{s}.jpg")}" alt="{lab} at {r["label"]}" loading="lazy"><figcaption>{lab}</figcaption></figure>'
        for a,s,lab in TILES)
    sel = ' class="pick"' if ri==0 else ""
    rows.append(f'''<section class="rung"{sel}>
<header><h2>{r["label"]}</h2>
<p class="mono">mouth/head ratio {r["ratio"]:.0f}{" · ≈ SHIPPING TODAY" if ri==0 else ""}</p></header>
<div class="strip">{cells}</div></section>''')

DOC = f'''<title>N2 · Down-valley gravity ladder</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{{--ground:#E7E9EC;--surface:#FFF;--ink:#171A1F;--muted:#5A616C;--line:#CDD3DA;--accent:#6E37AA;--r:3px}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{
 --ground:#0F1115;--surface:#181B21;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#B98CE8}}}}
:root[data-theme="dark"]{{--ground:#0F1115;--surface:#181B21;--ink:#E9ECF1;--muted:#939BA7;--line:#2A2F38;--accent:#B98CE8}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--ground);color:var(--ink);
 font:15px/1.6 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;-webkit-font-smoothing:antialiased}}
.wrap{{max-width:1420px;margin:0 auto;padding:0 24px 72px}}
header.top{{border-bottom:1px solid var(--line);margin-bottom:24px;padding:38px 0 20px}}
.eyebrow{{font-family:ui-monospace,Menlo,monospace;font-size:11px;letter-spacing:.16em;text-transform:uppercase;color:var(--muted);margin:0 0 10px}}
h1{{font-size:clamp(24px,3vw,36px);margin:0 0 10px;letter-spacing:-.02em;line-height:1.1;text-wrap:balance}}
.lede{{margin:0;max-width:66ch;color:var(--muted);font-size:15.5px}}
.mono{{font-family:ui-monospace,Menlo,monospace;font-size:12px;color:var(--muted);margin:0}}
.rung{{margin-bottom:26px;border:1px solid var(--line);border-radius:var(--r);background:var(--surface);padding:14px 16px}}
.rung.pick{{border-color:var(--accent)}}
.rung header{{display:flex;gap:16px;align-items:baseline;flex-wrap:wrap;margin-bottom:10px}}
.rung h2{{margin:0;font-size:17px}}
.strip{{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:10px}}
figure{{margin:0}}
figure img{{width:100%;height:auto;display:block;border-radius:2px}}
figcaption{{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--muted);padding-top:4px}}
.notes{{margin-top:34px;border-top:1px solid var(--line);padding-top:22px;max-width:70ch}}
.notes p{{color:var(--muted);font-size:14px}}
</style>
<div class="wrap">
<header class="top">
  <p class="eyebrow">Attempt 4 · N2 review · pick a rung</p>
  <h1>Down-valley gravity, round 2: the mass ratio, mouth over head</h1>
  <p class="lede">Reach stays at the shipped 750 m; the axis is now the <strong>mouth/head pull ratio</strong> —
  "the mouth may be 50 or even 100 times more pull than the head." mass(frac) = ratio^(1−frac), exponential from ×1 at
  the head to ×ratio at the mouth, capped at 2.6× the downhill force so the top rungs bend courses rather than
  teleporting them. Same seeds on every row — only the ratio moves. <strong>Reply with a rung and it ships.</strong></p>
</header>
{"".join(rows)}
<section class="notes">
  <p><strong>The loops are gone at the source.</strong> The corkscrews on the previous ladder (and seed 2's squiggle)
  were not a tuning problem: the old formulation shifted the field's <em>reference point</em> downstream, and at high
  gain that reference jumps discontinuously as the nearest channel point changes — carving eddies into the field that
  walkers spiralled through. Trimming loops treated the symptom; walkers re-entered the same wells. Gravity is now an
  explicit <em>force on the walker</em> (the downstream tangent of the nearest channel, weighted by fade × mass),
  which has no wells to fall into at any gain. Verified at ratio 100 on the tiles that looped worst: clean.</p>
</section>
</div>
'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
