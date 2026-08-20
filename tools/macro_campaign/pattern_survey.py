"""M0g: the remaining declared policies, PLUS the drainage-pattern discriminators.

One pass over the corpus at the planform family's settled policy -- 8 m grid,
extract_v2.CHANNEL_AREA_M2 = 6e4 (the SAME threshold Policy A declares; the
coarser grid is because the matched generated-side twin routes at 8 m).

Emits, per biome:
  near_par_frac   fraction of channel length running near-parallel  (real_planform)
  d2c_p50         distance to nearest channel, m
  density         km of channel per km2
  junc_p50/gt80   confluence angles, 6-cell baseline                (junction_real)
and the PATTERN discriminators the archetype design needs:
  orient_aniso    axial (doubled-angle) resultant of reach tangents,
                  length-weighted. 0 = isotropic dendritic, 1 = parallel/trellis.
  main_share      share of channel length in the single largest system
  n_systems       systems carrying >= 5% of channel length
  omega           Strahler max
"""
import sys, pathlib, json
import numpy as np

ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
from macro_campaign import cgrid, extract_v2, flow, develop            # noqa
from metrics import core as mcore                                       # noqa
import importlib.util
def _load(name, path):
    sp = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(sp); sp.loader.exec_module(m); return m
rp = _load("rp", ROOT / "tools/macro_campaign/real_planform.py")
jr = _load("jr", ROOT / "tools/macro_campaign/junction_real.py")

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]
CELL = 8.0
THRESH = extract_v2.CHANNEL_AREA_M2      # 6e4 -- Policy A's threshold
NTILE = int(sys.argv[1]) if len(sys.argv) > 1 else 99


def systems(rec, is_ch, nx):
    """Label each channel cell by the terminal its flow path reaches.
    Returns list of system lengths (cells), descending."""
    n = rec.size
    label = np.full(n, -1, dtype=np.int64)
    for start in np.nonzero(is_ch)[0]:
        if label[start] >= 0:
            continue
        path, cur = [], int(start)
        while True:
            if label[cur] >= 0:
                term = label[cur]; break
            path.append(cur)
            r = rec[cur]
            if r < 0 or not is_ch[r]:
                term = cur; break
            cur = int(r)
        for p in path:
            label[p] = term
    lab = label[is_ch]
    _, cnt = np.unique(lab, return_counts=True)
    return np.sort(cnt)[::-1]


def orient_aniso(reaches):
    """Length-weighted axial resultant of segment directions in [0,1]."""
    sx = sy = w = 0.0
    for p in reaches:
        if len(p) < 2:
            continue
        d = np.diff(p, axis=0)
        L = np.hypot(d[:, 0], d[:, 1])
        good = L > 0
        if not good.any():
            continue
        th = 2.0 * np.arctan2(d[good, 1], d[good, 0])   # doubled: axial
        sx += float((np.cos(th) * L[good]).sum())
        sy += float((np.sin(th) * L[good]).sum())
        w += float(L[good].sum())
    return float(np.hypot(sx, sy) / w) if w > 0 else float("nan")


rows = {}
for b in BIOMES:
    tids = [t for a, t in extract_v2.kept_tiles() if a == b][:NTILE]
    near_len = near_total = 0.0
    acc_np, acc_d2c, acc_den, acc_or, acc_ms, acc_ns, acc_om = [], [], [], [], [], [], []
    a6 = []
    for tid in tids:
        src = extract_v2.OUT / "tiles" / b / f"{tid}.cgrid"
        if not src.exists():
            continue
        z, (_, _, cell) = cgrid.read_f32(src)
        z = z.astype(np.float64)
        m = np.isfinite(z)
        if m.mean() < 0.95:
            continue
        z = np.where(m, z, np.nanmean(z[m]))
        dev = develop.load_mask(b, tid)
        if dev is not None and dev.shape != z.shape:
            dev = None
        # --- reaches at 8 m (the matched policy)
        reaches = rp.trace_reaches(z, cell, dev, grid_m=CELL)
        if len(reaches) < 2:
            continue
        # NB: the traced-grid cell (8 m), NOT the native cell -- real_planform.py:249
        # does the same. Passing 2.0 makes smooth_resample's window 200 m and
        # discards every reach shorter than that, which reads as 0.00% parallel.
        near, tot = rp.near_parallel(reaches, CELL)
        # POOLED across tiles, matching real_planform.py:264
        # (near_par_frac = near_len / near_total), not a median of per-tile
        # ratios -- the two differ and only one is the declared number.
        near_len += near; near_total += tot
        if tot > 0:
            acc_np.append(100.0 * near / tot)
        acc_or.append(orient_aniso(reaches))
        # --- raster side at 8 m
        step = max(int(round(CELL / cell)), 1)
        z8 = z[::step, ::step]
        zf = mcore.fill_depressions(z8, CELL)
        rec2, _ = flow.receivers(zf, CELL)
        accg = flow.accumulate(rec2).astype(float) * CELL * CELL
        ny, nx = z8.shape
        recf, accf = rec2.ravel(), accg.ravel()
        is_ch = accf >= THRESH
        if is_ch.sum() < 8:
            continue
        # density + d2c
        chan2 = is_ch.reshape(ny, nx)
        acc_den.append(chan2.sum() * CELL / (ny * nx * CELL * CELL / 1e6) / 1000.0)
        from scipy import ndimage
        d = ndimage.distance_transform_edt(~chan2, sampling=CELL)
        acc_d2c.append(float(np.percentile(d, 50)))
        # junction angles, 6-cell baseline
        a6 += jr.angles_for(recf, is_ch, accf, nx, ny, 6)
        # systems
        sl = systems(recf, is_ch, nx)
        tot_c = float(sl.sum())
        acc_ms.append(100.0 * sl[0] / tot_c)
        acc_ns.append(int((sl / tot_c >= 0.05).sum()))
        # omega
        h = mcore.horton_ratios(z8, CELL, accum_area_threshold_m2=THRESH)
        if np.isfinite(h["strahler_max"]):
            acc_om.append(h["strahler_max"])
    med = lambda v: float(np.median(v)) if len(v) else float("nan")
    rows[b] = dict(n=len(acc_np),
                   near_par=(100.0*near_len/near_total if near_total else float('nan')), d2c=med(acc_d2c), density=med(acc_den),
                   junc_p50=med(a6), junc_gt80=100.0*float(np.mean(np.array(a6) > 80)) if a6 else float("nan"),
                   orient=med(acc_or), main_share=med(acc_ms),
                   n_sys=med(acc_ns), omega=med(acc_om))
    r = rows[b]
    print(f"{b:14s} n={r['n']:3d} | nearpar {r['near_par']:5.2f}% | d2c {r['d2c']:6.1f} | dens {r['density']:5.2f} "
          f"| junc {r['junc_p50']:5.1f} (>80 {r['junc_gt80']:4.1f}%) | aniso {r['orient']:5.3f} "
          f"| main {r['main_share']:5.1f}% | nsys {r['n_sys']:.0f} | W {r['omega']:.0f}", flush=True)

json.dump(rows, open(pathlib.Path(sys.argv[0]).parent / "pattern_survey.json", "w"), indent=1)
print("\nwrote pattern_survey.json")
