"""Formal P2/P3 session materials, watered edition.

Differences from s3_visual.py (reviewer-driven):
- Generated side uses the S4 WATERED surface (g_dump.rs output): water
  bodies read as flat planes, like real lidar — the carved-bed tell is
  gone.
- Real side draws from tiles scored LOW on a road/rectilinearity probe
  (roads were identifying the real tile in most pairs). Score = p95
  over sliding 256 m windows of the max 15°-orientation-bin share of
  gradient mass (the dictionary rejector's statistic, applied locally):
  roads dominate a window even when the tile is mostly natural.

Outputs under <out_dir>:
  p2_pairs/pair_NN.png + p2_key.json     which-is-real (target <= 65%)
  p3_tiles/tile_NN.png + p3_key.json     name-the-biome (target >= 80%)
  real_pool_report.txt                   chosen real tiles + road scores

    python tools/dictionary/p2_session.py <gen_dump_dir> <out_dir>
"""
import json
import pathlib
import random
import sys

import numpy as np
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import cgrid, extract_v2  # noqa: E402

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]


def hillshade(z, cell, ex=1.6):
    gy, gx = np.gradient(z * ex, cell)
    az, alt = np.radians(315), np.radians(45)
    sl = np.arctan(np.hypot(gx, gy))
    asp = np.arctan2(-gx, gy)
    hs = np.sin(alt) * np.cos(sl) + np.cos(alt) * np.sin(sl) * np.cos(az - asp)
    return np.clip((hs + 0.15) / 1.15 * 255, 0, 255).astype(np.uint8)


def road_score(z, cell):
    """p95 over sliding windows of local straight-orientation dominance."""
    gy, gx = np.gradient(z, cell)
    mag = np.hypot(gx, gy)
    ang = np.mod(np.degrees(np.arctan2(gy, gx)), 180.0)
    bins = (ang / 15.0).astype(int) % 12
    w = max(1, int(256 / cell))
    scores = []
    for y0 in range(0, z.shape[0] - w, w // 2):
        for x0 in range(0, z.shape[1] - w, w // 2):
            m = mag[y0:y0 + w, x0:x0 + w].ravel()
            bb = bins[y0:y0 + w, x0:x0 + w].ravel()
            tot = m.sum()
            if tot <= 1e-9:
                continue
            conc = np.bincount(bb, weights=m, minlength=12) / tot
            scores.append(conc.max())
    return float(np.percentile(scores, 95)) if scores else 1.0


def real_pool(report_lines):
    """Per biome: kept tiles sorted by road score, lowest first."""
    pool = {}
    for biome in BIOMES:
        tids = [t for a, t in extract_v2.kept_tiles() if a == biome]
        scored = []
        for tid in tids:
            z, (_, _, cell) = cgrid.read_f32(
                extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            z = np.where(m, z, np.nanmean(z[m]))
            scored.append((road_score(z, cell), tid, z, cell))
        scored.sort(key=lambda t: t[0])
        pool[biome] = scored
        report_lines.append(
            f"{biome}: " + ", ".join(f"{tid}={s:.2f}" for s, tid, _, _ in scored[:6]))
    return pool


def crop(z, cell, rng, w_m=750, clean=False):
    """Random crop; with clean=True draw 8 candidates and keep the one
    with the lowest LOCAL road score — the tile filter alone still let
    random crops land on field boundaries."""
    w = int(w_m / cell)
    c0, c1 = int(750 / cell), int(2250 / cell) - w
    best = None
    for _ in range(16 if clean else 1):
        y = rng.randrange(c0, max(c0 + 1, c1))
        x = rng.randrange(c0, max(c0 + 1, c1))
        patch = z[y:y + w, x:x + w]
        score = road_score(patch, cell) if clean else 0.0
        if best is None or score < best[0]:
            best = (score, patch)
    return hillshade(best[1], cell)


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    (out / "p2_pairs").mkdir(parents=True, exist_ok=True)
    (out / "p3_tiles").mkdir(parents=True, exist_ok=True)
    rng = random.Random(20260818)
    report = ["road scores of the 6 lowest tiles per biome (lower = cleaner):"]
    pool = real_pool(report)
    (out / "real_pool_report.txt").write_text("\n".join(report) + "\n")

    gens = {b: sorted(dump.glob(f"{b}_*.f32")) for b in BIOMES}

    # ---- P2: which is real -------------------------------------------------
    key = {}
    pair_i = 0
    for biome in BIOMES:
        picks = rng.sample(gens[biome], min(2, len(gens[biome])))
        for gi, f in enumerate(picks):
            z = np.frombuffer(f.read_bytes(), dtype="<f4")
            n = int(len(z) ** 0.5)
            z = z.reshape(n, n).astype(np.float64)
            # use the top-3 cleanest reals, rotating
            rs, rtid, rz, rcell = pool[biome][(pair_i + gi) % 3]
            g_img = Image.fromarray(crop(z, 2.0, rng)).convert("RGB").resize((360, 360))
            r_img = Image.fromarray(crop(rz, rcell, rng, clean=True)).convert("RGB").resize((360, 360))
            gen_left = rng.random() < 0.5
            left, right = (g_img, r_img) if gen_left else (r_img, g_img)
            sheet = Image.new("RGB", (2 * 366 + 6, 390), (255, 255, 255))
            sheet.paste(left, (6, 24))
            sheet.paste(right, (372, 24))
            d = ImageDraw.Draw(sheet)
            name = f"pair_{pair_i:02d}"
            d.text((6, 4), f"{name}  ({biome})   which is REAL?", fill=(0, 0, 0))
            sheet.save(out / "p2_pairs" / f"{name}.png")
            key[name] = {"real": "left" if not gen_left else "right",
                         "biome": biome, "gen": f.stem, "real_tile": rtid,
                         "real_road_score": round(rs, 3)}
            pair_i += 1
    (out / "p2_key.json").write_text(json.dumps(key, indent=1))

    # ---- P3: name the biome (generated tiles only) -------------------------
    key3 = {}
    tiles = []
    for biome in BIOMES:
        for f in rng.sample(gens[biome], min(2, len(gens[biome]))):
            tiles.append((biome, f))
    rng.shuffle(tiles)
    for i, (biome, f) in enumerate(tiles):
        z = np.frombuffer(f.read_bytes(), dtype="<f4")
        n = int(len(z) ** 0.5)
        z = z.reshape(n, n).astype(np.float64)
        img = Image.fromarray(hillshade(z[375:1125, 375:1125], 2.0)).convert("RGB").resize((540, 540))
        sheet = Image.new("RGB", (552, 570), (255, 255, 255))
        sheet.paste(img, (6, 24))
        d = ImageDraw.Draw(sheet)
        name = f"tile_{i:02d}"
        d.text((6, 4), f"{name}   which archetype?", fill=(0, 0, 0))
        sheet.save(out / "p3_tiles" / f"{name}.png")
        key3[name] = {"biome": biome, "gen": f.stem}
    (out / "p3_key.json").write_text(json.dumps(key3, indent=1))
    print(f"{pair_i} P2 pairs + {len(tiles)} P3 tiles -> {out}")


if __name__ == "__main__":
    main()
