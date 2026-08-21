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
    sel = ' class="pick"' if ri==1 else ""
    rows.append(f'''<section class="rung"{sel}>
<header><h2>{r["label"]}</h2>
<p class="mono">shift {r["shift"]:.0f} m · reach {r["reach"]:.0f} m{" · SHIPPING TODAY" if ri==1 else ""}</p></header>
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
  <h1>Down-valley gravity: the same four tiles at five pull strengths</h1>
  <p class="lede">"Almost like the trunks have a gravity pulling tributaries towards them at all times and the
  downstream sections have greater mass." Implemented exactly so: the field's reference point shifts downstream of the
  nearest channel point (the pull), scaled ×1.3 at a channel's mouth down to ×0.7 at its head (the mass), fading with
  distance (the reach). Same seeds on every row — only the dial moves. <strong>Reply with a rung (or between two) and
  it becomes the operating point.</strong></p>
</header>
{"".join(rows)}
<section class="notes">
  <p><strong>Also fixed in this round:</strong> the tight squiggle on Piedmont seed 2's left tributary. Near a
  confluence the nearest-channel reference flips between the two channels, the gradient rotates every step, and the
  walker orbited in place, engraving a knot. Two changes: heading is now integrated momentum rather than a per-step
  weighted sum, and an anti-orbit guard detects a circling walker, <em>trims the loop it walked</em>, and takes the
  analytic join from clean geometry.</p>
  <p><strong>A caveat the ladder makes visible:</strong> at L3–L4 tributaries run long, glancing courses beside the
  trunk before joining. That is the look of strong gravity — and it will raise the near-parallel fraction at N3, when
  density arrives. Not an argument against L3–L4; just the trade named before it is bought.</p>
</section>
</div>
'''
OUT.write_text(DOC)
print("wrote", OUT, f"{OUT.stat().st_size/1e6:.1f} MB")
