"""D6-W: the siting-window sweep — does a seed CONTAIN a golfable 600 m
window, and where does that window sit in the terrain?

Mirrors S5's search geometry (600 m axis-aligned window, anywhere in the
1.5 km core). Per tile/seed: the best window's fraction of cells with
slope <= 8% (the fairway grade target), its internal relief, and its
terrain position (mean elevation as a percentile of the tile's p5-p95
range: 0 = valley floor, 1 = hilltop). Run identically over real kept
tiles and generated S2 dumps (variety_dump.rs output dir as argv[1]).

2026-08-11 baseline (post variety fixes): real hill-country land almost
never contains a fully-calm window (>=85% calm: 0/27 tiles; median best
0.25) yet real hc courses exist - architects take the valley/bench land
(best-window terrain position p50 0.36) and shape the rest. Generated
hc offers MORE findable pockets (13/20 seeds >=70% calm) in the same
terrain position band (p50 0.49). Re-run after S3 lands texture: these
pre-texture surfaces are candle-wax smooth, so generated calm fractions
will drop toward the real numbers.
"""
import sys
import pathlib
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
from macro_campaign import cgrid, extract_v2  # noqa: E402

W, STRIDE, C0, C1 = 75, 12, 94, 281  # 600 m window, ~100 m stride, 1.5 km core
BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]


def windows(z8):
    gy, gx = np.gradient(z8, 8.0)
    ok8 = (np.hypot(gx, gy) <= 0.08).astype(np.float64)
    s = np.zeros((ok8.shape[0] + 1, ok8.shape[1] + 1))
    s[1:, 1:] = np.cumsum(np.cumsum(ok8, 0), 1)
    zlo, zhi = np.percentile(z8, 5), np.percentile(z8, 95)
    out = []
    for y in range(C0, C1 - W + 1, STRIDE):
        for x in range(C0, C1 - W + 1, STRIDE):
            f8 = (s[y + W, x + W] - s[y, x + W] - s[y + W, x] + s[y, x]) / (W * W)
            win = z8[y:y + W, x:x + W]
            rel = float(np.percentile(win, 95) - np.percentile(win, 5))
            pos = float((win.mean() - zlo) / max(zhi - zlo, 1e-9))
            out.append((f8, rel, pos))
    return out


def summarize(tag, tiles):
    if not tiles:
        return
    best = [max(windows(z), key=lambda r: r[0]) for z in tiles]
    f = [b[0] for b in best]
    rel = [b[1] for b in best]
    pos = [b[2] for b in best]
    n = len(tiles)
    q = lambda v, p: float(np.percentile(v, p))
    print(f"{tag:22s} n={n:3d} | best frac(slope<=8%) p10/p50/p90 "
          f"{q(f,10):.2f}/{q(f,50):.2f}/{q(f,90):.2f} | >=70%: "
          f"{sum(x >= 0.70 for x in f)}/{n} >=85%: {sum(x >= 0.85 for x in f)}/{n} | "
          f"relief p50 {q(rel,50):.0f} m | position p10/p50/p90 "
          f"{q(pos,10):.2f}/{q(pos,50):.2f}/{q(pos,90):.2f}")


def main():
    gen_dir = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else None
    for biome in BIOMES:
        tiles = []
        for tid in [t for a, t in extract_v2.kept_tiles() if a == biome]:
            z, _ = cgrid.read_f32(extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            if m.mean() < 0.95:
                continue
            tiles.append(np.where(m, z, np.nanmean(z[m]))[::4, ::4])
        summarize(f"real {biome}", tiles)
        if gen_dir:
            gen = [np.frombuffer(f.read_bytes(), dtype=np.float32)
                   .reshape(375, 375).astype(np.float64)
                   for f in sorted(gen_dir.glob(f"{biome}_*.f32"))]
            summarize(f"gen  {biome}", gen)
        print()


if __name__ == "__main__":
    main()
