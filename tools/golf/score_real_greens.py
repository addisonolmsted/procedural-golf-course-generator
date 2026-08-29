"""Score REAL green locations with our green instrument, vs random controls
on the same property. Writes real_green_scores.json."""
import sys, os, json, pathlib
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
for p in ("macro_campaign", "golf", "dtm_primitives"):
    sys.path.insert(0, str(ROOT / "tools" / p))
from macro_campaign import cgrid
import siting, greens as G

SC = pathlib.Path(os.environ["SCRATCH"])
data = json.load(open(SC / "real_greens.json"))
rng = np.random.default_rng(7)
CLS = {1:"flat",2:"peak",3:"ridge",4:"shoulder",5:"spur",6:"slope",
       7:"hollow",8:"footslope",9:"valley",10:"pit"}

out = {}
for region, courses in data.items():
    rows = {"real": [], "ctrl": []}
    recall_hit = recall_tot = 0
    for c in courses:
        if len(c["greens"]) < 8:
            continue
        z, (_, _, cell) = cgrid.read_f32(c["tile"])
        z = np.where(np.isfinite(z), z, np.nanmean(z)).astype(np.float64)
        wet = np.zeros(z.shape, bool)          # real DEMs carry no water mask
        f = siting.build_fields(z, cell, wet)
        m = siting.build_morphology(f)
        p = siting.build_persistence(f)

        gs = np.array(c["greens"])
        # controls: random points in the greens' bounding box (on-property)
        y0, y1 = gs[:, 0].min(), gs[:, 0].max()
        x0, x1 = gs[:, 1].min(), gs[:, 1].max()
        ctrl = np.stack([rng.uniform(y0, y1, 3 * len(gs)),
                         rng.uniform(x0, x1, 3 * len(gs))], 1)

        def measure(pts, key):
            for (ym, xm) in pts:
                yi, xi = int(ym / f.cell), int(xm / f.cell)
                if not (0 <= yi < f.z8.shape[0] and 0 <= xi < f.z8.shape[1]):
                    continue
                ok, grad, resid = G.confirm_2m(z, cell, wet, ym, xm)
                tab = G.approach_table(f, yi, xi, grad)
                rows[key].append(dict(
                    course=c["name"],
                    pad=bool(f.pad_green[yi, xi]),
                    confirm=bool(ok),
                    cls240=CLS.get(int(m.cls240[yi, xi]), "edge"),
                    cls80=CLS.get(int(m.cls80[yi, xi]), "edge"),
                    relief_pos=float(f.relief_pos[yi, xi]),
                    pit=float(p.pit[yi, xi]),
                    peak=float(p.peak[yi, xi]),
                    vis_best=float(tab[:, 0].max()),
                    vis_mean=float(tab[:, 0].mean()),
                    recept_best=float(tab[:, 1].max()),
                    backdrop_best=float(np.clip(tab[:, 2], 0, None).max()),
                    room_best=float(tab[:, 3].max()),
                    rough=float(f.subgrid_rough[yi, xi]),
                ))
        measure(gs, "real")
        measure(ctrl, "ctrl")

        # recall: candidates from our generator over the greens' area
        class FakeSit:  # window covering the greens bbox, clipped to grid
            pass
        side = max(y1 - y0, x1 - x0) + 200.0
        side = min(side, (f.z8.shape[0] - 2 * m.collar) * f.cell - 16)
        wy = np.clip((y0 + y1) / 2 - side / 2, m.collar * f.cell,
                     f.z8.shape[0] * f.cell - side - m.collar * f.cell)
        wx = np.clip((x0 + x1) / 2 - side / 2, m.collar * f.cell,
                     f.z8.shape[1] * f.cell - side - m.collar * f.cell)
        sit = FakeSit()
        sit.window_ij = (int(wy / f.cell), int(wx / f.cell))
        sit.window_m = (wy, wx, side)
        class FakeCH:
            yx = (wy + side / 2, wx + side / 2)
            reserved_green = yx
            reserved_tee = yx
            score = 1.0
        sit.clubhouse = FakeCH()
        try:
            pool = G.generate(z, cell, wet, sit, f, m, p, n_target=125)
            cand = np.array([q.yx for q in pool]) if pool else np.zeros((0, 2))
            for (ym, xm) in gs:
                recall_tot += 1
                if len(cand) and np.hypot(cand[:, 0] - ym,
                                          cand[:, 1] - xm).min() <= 40.0:
                    recall_hit += 1
        except Exception as e:
            print(f"    recall skipped for {c['name']}: {e}")
        print(f"  {region} {c['name'][:30]:32s} greens {len(gs):3d}  "
              f"recall so far {recall_hit}/{recall_tot}")
    out[region] = dict(rows=rows, recall=[recall_hit, recall_tot])
json.dump(out, open(SC / "real_green_scores.json", "w"))
print("done")
