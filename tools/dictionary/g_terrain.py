"""G-TERRAIN measurement battery (the non-network half).

1. ENERGY DISTANCE: 66-scalar feature vectors (tools/metrics definitions)
   for N generated watered tiles per biome vs the dictionary's HELD-OUT
   real tiles, z-scored in the pooled space. Reported per biome and
   pooled, next to the real-vs-real split-half baseline and the v1
   baseline of 2.35 (gate: beat it).
2. PSD SEAM: radial PSD of every generated tile; log-log slope fitted
   separately over 32-60 m and 70-130 m; a kink at the 64 m band handoff
   is the S2/S3 seam showing. Gate: |slope step| small and no power gap.

    python tools/dictionary/g_terrain.py <gen_dump_dir> <out_dir>
"""
import json
import pathlib
import struct
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "tile_scout"))
import yaml  # noqa: E402
from macro_campaign import cgrid, extract_v2  # noqa: E402
from metrics import features  # noqa: E402
from tile_scout import scoring  # noqa: E402

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]
V1_BASELINE = 2.35


def holdout_tiles():
    b = open(ROOT / "assets" / "dictionary_v2.bin", "rb").read()
    hlen = struct.unpack("<I", b[8:12])[0]
    return json.loads(b[12:12 + hlen])["holdout_tiles"]


def core_crop(z, cell):
    c0, c1 = int(750 / cell), int(2250 / cell)
    return z[c0:c1, c0:c1]


def vector(z, cell, cfg, axes):
    S, _ = features.compute(z, cell, np.isfinite(z), cfg, axes)
    return S


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    cfg = yaml.safe_load(open(ROOT / "tools" / "metrics" / "metrics" / "config.yaml"))
    axes = features.build_axes(cfg)
    ho = holdout_tiles()

    rows_gen, rows_real = {}, {}
    psd_rows = []
    for biome in BIOMES:
        rows_gen[biome] = []
        for f in sorted(dump.glob(f"{biome}_*.f32")):
            z = np.frombuffer(f.read_bytes(), dtype="<f4")
            n = int(len(z) ** 0.5)
            z = z.reshape(n, n).astype(np.float64)
            zc = core_crop(z, 2.0)
            rows_gen[biome].append(vector(zc, 2.0, cfg, axes))
            # PSD seam on the full tile (detrended inside radial_psd)
            from metrics import core as mcore
            wl, psd, kc = mcore.radial_psd(z, 2.0, np.isfinite(z),
                                           cfg["detrend"]["method"],
                                           cfg["spectrum"]["psd_n_axis"])
            lam = 1.0 / np.maximum(kc, 1e-12)
            def slope(lo, hi):
                m = (lam >= lo) & (lam <= hi) & (psd > 0)
                if m.sum() < 4:
                    return np.nan
                return np.polyfit(np.log(kc[m]), np.log(psd[m]), 1)[0]
            psd_rows.append((biome, f.stem, slope(32, 60), slope(70, 130)))
        rows_real[biome] = []
        for tid in ho[biome]:
            z, (_, _, cell) = cgrid.read_f32(extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            z = np.where(m, z, np.nanmean(z[m]))
            rows_real[biome].append(vector(core_crop(z, cell), cell, cfg, axes))

    # shared feature list = scalar keys finite in EVERY row of both sets
    every_row = [r for b in BIOMES for r in rows_gen[b] + rows_real[b]]
    keys = sorted(
        k for k in every_row[0]
        if all(np.isfinite(r.get(k, np.nan)) for r in every_row)
    )
    def mat(rows):
        return np.array([[r[k] for k in keys] for r in rows])
    all_rows = mat([r for b in BIOMES for r in rows_gen[b] + rows_real[b]])
    mu, sd = all_rows.mean(0), all_rows.std(0) + 1e-12

    report = ["# G-TERRAIN energy distance + PSD seam", "",
              f"{len(keys)} shared scalar features; generated n = "
              f"{sum(len(rows_gen[b]) for b in BIOMES)}, held-out real n = "
              f"{sum(len(rows_real[b]) for b in BIOMES)}", "",
              "| biome | ED gen-vs-real | ED real split-half (floor) | verdict vs v1 2.35 |",
              "|---|---|---|---|"]
    peds = []
    for biome in BIOMES:
        g = (mat(rows_gen[biome]) - mu) / sd
        r = (mat(rows_real[biome]) - mu) / sd
        ed = scoring.energy_distance(g, r)
        half = max(1, len(r) // 2)
        ed_floor = scoring.energy_distance(r[:half], r[half:]) if len(r) >= 2 else np.nan
        verdict = "PASS" if ed < V1_BASELINE else "fail"
        peds.append(ed)
        report.append(f"| {biome} | {ed:.2f} | {ed_floor:.2f} | {verdict} |")
    g_all = (mat([r for b in BIOMES for r in rows_gen[b]]) - mu) / sd
    r_all = (mat([r for b in BIOMES for r in rows_real[b]]) - mu) / sd
    ed_pool = scoring.energy_distance(g_all, r_all)
    report += ["", f"POOLED energy distance: {ed_pool:.2f}  (v1 baseline {V1_BASELINE}; "
               f"{'PASS' if ed_pool < V1_BASELINE else 'FAIL'})", "", "## PSD seam (64 m handoff)",
               "", "| biome | slope 32-60 m (mean) | slope 70-130 m (mean) | step |", "|---|---|---|---|"]
    for biome in BIOMES:
        a = np.array([r[2] for r in psd_rows if r[0] == biome])
        b = np.array([r[3] for r in psd_rows if r[0] == biome])
        report.append(f"| {biome} | {np.nanmean(a):.2f} | {np.nanmean(b):.2f} | "
                      f"{abs(np.nanmean(a) - np.nanmean(b)):.2f} |")
    (out / "G_TERRAIN_battery.md").write_text("\n".join(report) + "\n")
    print("\n".join(report))


if __name__ == "__main__":
    main()
