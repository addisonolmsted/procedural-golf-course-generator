"""QA gallery — THE Stage-0 assessment artifact.

Per course: raw/naturalized hillshade PNGs, a class-colored mask overlay, and a
signed raw-minus-naturalized diff (must be gray outside the inpaint mask), on a
toggle-chip HTML page with the meta table. Index: sortable table of every
course's provenance/coverage/inpaint stats + the margin-coverage CSV.
Hillshade matches tools/parkland_atlas/preview.py (az 315, alt 45).
"""

from __future__ import annotations

import csv
import html
import json
import os

import numpy as np
from PIL import Image

from . import config, grids, meta

# class overlay colors (RGBA)
CLASS_COLORS = {
    "green": (46, 204, 64, 170),
    "tee": (0, 116, 217, 170),
    "bunker": (255, 220, 0, 170),
    "fairway": (140, 220, 140, 90),
    "building": (170, 60, 200, 170),
    "road": (120, 120, 120, 170),
    "water": (40, 90, 255, 170),
    "earthwork": (150, 100, 40, 170),
    "disturbed": (200, 170, 60, 170),
    "artifact_flat": (0, 220, 220, 170),
    "artifact_curvature": (255, 65, 54, 170),
    "artifact_seam": (255, 133, 27, 200),
    "artifact_bridge": (255, 0, 255, 200),
    "consolidated": (255, 255, 255, 90),
}


def hillshade_rgb(z: np.ndarray, valid: np.ndarray, res_m: float) -> np.ndarray:
    """(h, w, 3) uint8 hillshade x hypsometric ramp; nodata = dark gray."""
    zf = np.where(valid, z, np.nan)
    zmin = np.nanpercentile(zf, 1.0)
    zmax = np.nanpercentile(zf, 99.0)
    t = np.clip((zf - zmin) / max(zmax - zmin, 1e-6), 0, 1)
    # simple terrain ramp (mirrors preview.py's feel)
    stops = np.array([
        [70, 110, 75], [110, 150, 90], [160, 180, 105],
        [200, 190, 130], [220, 205, 170], [235, 230, 215]], np.float64)
    idx = t * (len(stops) - 1)
    lo = np.floor(idx).astype(int)
    hi = np.minimum(lo + 1, len(stops) - 1)
    f = (idx - lo)[..., None]
    base = stops[lo] * (1 - f) + stops[hi] * f
    zp = np.where(valid, z, np.nanmedian(zf))
    gy, gx = np.gradient(zp, res_m)
    az = np.radians(config.HILLSHADE_AZ_DEG)
    alt = np.radians(config.HILLSHADE_ALT_DEG)
    slope = np.arctan(2.0 * np.hypot(gx, gy))
    aspect = np.arctan2(-gx, gy)
    hs = (np.sin(alt) * np.cos(slope)
          + np.cos(alt) * np.sin(slope) * np.cos(az - aspect))
    hs = np.clip(hs, 0.15, 1.0)[..., None]
    img = np.clip(base * hs, 0, 255).astype(np.uint8)
    img[~valid] = (40, 40, 40)
    return img


def _save_png(path: str, arr: np.ndarray) -> None:
    im = Image.fromarray(arr)
    if max(im.size) > config.QA_MAX_PX:
        s = config.QA_MAX_PX / max(im.size)
        im = im.resize((round(im.width * s), round(im.height * s)),
                       Image.Resampling.LANCZOS)
    im.save(path, optimize=True)


def _course_pngs(key: str) -> dict | None:
    sdir = os.path.join(config.STORE, key)
    if not os.path.exists(os.path.join(sdir, "naturalized.tif")):
        return None
    m = meta.read_meta(os.path.join(sdir, "meta.json"))
    raw, _t, _e = grids.read_gtiff(os.path.join(sdir, "raw.tif"))
    nat, _t2, _e2 = grids.read_gtiff(os.path.join(sdir, "naturalized.tif"))
    valid = raw != config.NODATA
    res = config.WORK_RES_M
    imgdir = os.path.join(config.QA, "img")
    os.makedirs(imgdir, exist_ok=True)

    _save_png(os.path.join(imgdir, f"{key}_raw.png"),
              hillshade_rgb(raw, valid, res))
    _save_png(os.path.join(imgdir, f"{key}_nat.png"),
              hillshade_rgb(nat, valid, res))

    # mask overlay on the raw hillshade; exterior desaturated
    over = hillshade_rgb(raw, valid, res).astype(np.float64)
    mdir = os.path.join(sdir, "masks")
    ext, _t3, _e3 = grids.read_gtiff(os.path.join(mdir, "exterior.tif"))
    ext = ext.astype(bool)
    over[ext] = over[ext] * 0.45 + 40.0
    for cls, rgba in CLASS_COLORS.items():
        path = os.path.join(mdir, f"{cls}.tif")
        if not os.path.exists(path):
            continue
        mk, _tt, _ee = grids.read_gtiff(path)
        mk = mk.astype(bool)
        if not mk.any():
            continue
        a = rgba[3] / 255.0
        over[mk] = over[mk] * (1 - a) + np.array(rgba[:3], np.float64) * a
    _save_png(os.path.join(imgdir, f"{key}_mask.png"),
              np.clip(over, 0, 255).astype(np.uint8))

    # signed diff: must be mid-gray everywhere outside the inpaint mask
    d = np.where(valid, raw - nat, 0.0)
    dmax = max(float(np.abs(d).max()), 0.5)
    dn = np.clip(d / dmax, -1, 1)
    diff = np.zeros((*d.shape, 3), np.uint8)
    diff[..., 0] = np.clip(128 + dn * 127, 0, 255).astype(np.uint8)   # raw higher = red
    diff[..., 2] = np.clip(128 - dn * 127, 0, 255).astype(np.uint8)   # nat higher = blue
    diff[..., 1] = 128
    diff[~valid] = (40, 40, 40)
    _save_png(os.path.join(imgdir, f"{key}_diff.png"), diff)
    return m


_PAGE = """<!doctype html><meta charset="utf-8"><title>{key} — dtm_atlas QA</title>
<style>body{{font:14px system-ui;margin:16px;background:#181818;color:#ddd}}
.chip{{display:inline-block;padding:4px 12px;margin:2px;border:1px solid #555;
border-radius:12px;cursor:pointer;user-select:none}}.chip.on{{background:#2a6;color:#fff}}
#stack{{position:relative;margin-top:10px}}#stack img{{position:absolute;top:0;left:0;
max-width:96vw;height:auto}}#stack img:first-child{{position:relative}}
table{{border-collapse:collapse;margin-top:14px}}td,th{{border:1px solid #444;
padding:3px 9px;text-align:left;font-size:13px}}</style>
<h2>{label} <small>({key})</small></h2>
<div>{chips}</div><div id="stack">{imgs}</div>
<table>{meta_rows}</table>
<script>
function tg(id){{const el=document.getElementById('im_'+id),c=document.getElementById('ch_'+id);
const on=el.style.display!=='none';el.style.display=on?'none':'';c.classList.toggle('on',!on);}}
</script>"""


def _course_page(key: str, m: dict) -> None:
    layers = [("raw", "raw hillshade", ""), ("nat", "naturalized", "display:none"),
              ("mask", "mask overlay", "display:none"), ("diff", "raw−nat diff", "display:none")]
    chips = "".join(
        f'<span class="chip{" on" if not st else ""}" id="ch_{lid}" '
        f'onclick="tg(\'{lid}\')">{name}</span>' for lid, name, st in layers)
    imgs = "".join(
        f'<img id="im_{lid}" src="img/{key}_{lid}.png" style="{st}">'
        for lid, name, st in layers)
    rows = []
    flat = {
        "boundary": f"{m['boundary_source']} (name score {m.get('boundary_name_score', 0)})",
        "effective source": f"{m['fetch']['effective_source']} "
                            f"(texture {m['fetch']['texture_frac']})",
        "usable_for_calibration": m["usable_for_calibration"],
        "grid": f"{m['grid']['w']}x{m['grid']['h']} @ {m['grid']['res_m']} m ({m['crs']})",
        "margin": json.dumps(m.get("margin", {})),
        "inpaint": json.dumps({k: v for k, v in m.get("inpaint", {}).items()}),
        "artifacts": json.dumps(m.get("artifacts", {})),
        "masks %": json.dumps({k: v["frac"] for k, v in m.get("masks", {}).items()}),
        "elev raw / naturalized": f"{m.get('elev')} / {m.get('elev_naturalized')}",
    }
    for k, v in flat.items():
        rows.append(f"<tr><th>{html.escape(k)}</th>"
                    f"<td>{html.escape(str(v))}</td></tr>")
    page = _PAGE.format(key=key, label=html.escape(m.get("label", key)),
                        chips=chips, imgs=imgs, meta_rows="".join(rows))
    with open(os.path.join(config.QA, f"{key}.html"), "w") as f:
        f.write(page)


_INDEX = """<!doctype html><meta charset="utf-8"><title>dtm_atlas QA index</title>
<style>body{font:14px system-ui;margin:16px;background:#181818;color:#ddd}
a{color:#7cf}table{border-collapse:collapse}td,th{border:1px solid #444;
padding:3px 8px;font-size:13px;text-align:right}td:first-child,th:first-child,
td:nth-child(2){text-align:left}th{cursor:pointer;background:#242424}</style>
<h2>dtm_atlas — Stage 0 QA index</h2><p id="summary"></p>
<table id="t"><thead></thead><tbody></tbody></table>
<script>
const ROWS=__ROWS__;
const COLS=[["key","key"],["label","label"],["src","source"],["usable","usable"],
["km","window km"],["extdata","margin data %"],["extinp","margin inpainted %"],
["inp","inpaint %"],["curv","curv cells"],["seams","seams"],["bridges","bridges"],
["bnd","boundary"]];
let dir=1;
function render(rows){
 document.querySelector('#t thead').innerHTML='<tr>'+COLS.map(([k,l],i)=>
  `<th onclick="srt(${i})">${l}</th>`).join('')+'</tr>';
 document.querySelector('#t tbody').innerHTML=rows.map(r=>'<tr>'+COLS.map(([k],i)=>
  i==0?`<td><a href="${r.key}.html">${r.key}</a></td>`:`<td>${r[k]}</td>`
 ).join('')+'</tr>').join('');
}
function srt(i){const k=COLS[i][0];dir=-dir;
 ROWS.sort((a,b)=>(a[k]>b[k]?1:a[k]<b[k]?-1:0)*dir);render(ROWS);}
render(ROWS);
const n=ROWS.length,u=ROWS.filter(r=>r.usable=='yes').length,
f=ROWS.filter(r=>r.bnd.includes('fallback')).length;
document.getElementById('summary').textContent=
 `${n} courses | ${u} usable_for_calibration | ${f} boundary fallbacks`;
</script>"""


def build(keys: list[str]) -> int:
    os.makedirs(config.QA, exist_ok=True)
    os.makedirs(config.REPORTS, exist_ok=True)
    rows = []
    for key in keys:
        try:
            m = _course_pngs(key)
        except Exception as e:
            print(f"qa: {key}: FAIL {type(e).__name__}: {e}")
            continue
        if m is None:
            continue
        _course_page(key, m)
        mg = m.get("margin", {})
        rows.append({
            "key": key,
            "label": m.get("label", key),
            "source": m["fetch"]["effective_source"],
            "usable": "yes" if m["usable_for_calibration"] else "NO",
            "window km": f"{m['grid']['w'] * 2 / 1000:.1f}x{m['grid']['h'] * 2 / 1000:.1f}",
            "margin data %": round(100 * mg.get("exterior_data_frac", 0), 1),
            "margin inpainted %": round(100 * mg.get("exterior_inpaint_frac", 0), 1),
            "inpaint %": round(100 * m.get("inpaint", {}).get("frac", 0), 1),
            "curv cells": m.get("artifacts", {}).get("curvature_cells", 0),
            "seams": len(m.get("artifacts", {}).get("seam_rows", []))
            + len(m.get("artifacts", {}).get("seam_cols", [])),
            "bridges": m.get("artifacts", {}).get("bridge_components", 0),
            "boundary": m["boundary_source"],
        })
        print(f"qa: {key}: ok")
    with open(os.path.join(config.QA, "index.html"), "w") as f:
        f.write(_INDEX.replace("__ROWS__", json.dumps(rows, sort_keys=True)))
    with open(os.path.join(config.REPORTS, "margin_coverage.csv"), "w",
              newline="") as f:
        if rows:
            wtr = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
            wtr.writeheader()
            wtr.writerows(rows)
    print(f"qa: wrote {len(rows)} pages + index + margin_coverage.csv")
    return 0
