"""S3 visual confirmation: triptychs + blind A/B pairs.

Two deliverables, both rendered with IDENTICAL hillshading so the eye
compares terrain, not tonemapping:

1. `TRIPTYCH_<biome>.png` — three 1.5 km crops side by side:
   S2 base | S3 amplified | a REAL corpus tile of the same biome.
   The direct "did amplification produce the desired effect" check.

2. `ab_pairs/pair_NN.png` + `ab_key.json` — the P2 blind format: each
   image is two 750 m crops, one real, one generated, left/right
   randomized. Judge which is real; the G-TERRAIN target is that the
   reviewer scores <= 65% (near-chance).

    python tools/dictionary/s3_visual.py <dump_dir> <out_dir>
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


def seeds_in(dump):
    """Discover dumped seeds instead of hardcoding them, so each review
    round can use fresh generated tiles."""
    out = set()
    for f in dump.glob("*_*.f32"):
        out.add(int(f.stem.rsplit("_", 1)[1]))
    return sorted(out)


def hillshade(z, cell, ex=1.6):
    gy, gx = np.gradient(z * ex, cell)
    az, alt = np.radians(315), np.radians(45)
    sl = np.arctan(np.hypot(gx, gy))
    asp = np.arctan2(-gx, gy)
    hs = np.sin(alt) * np.cos(sl) + np.cos(alt) * np.sin(sl) * np.cos(az - asp)
    return np.clip((hs + 0.15) / 1.15 * 255, 0, 255).astype(np.uint8)


def real_tile(biome, idx=0):
    tids = [t for a, t in extract_v2.kept_tiles() if a == biome]
    tid = tids[idx % len(tids)]
    z, (_, _, cell) = cgrid.read_f32(extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
    z = z.astype(np.float64)
    m = np.isfinite(z)
    return np.where(m, z, np.nanmean(z[m])), cell, tid


def core(z, cell):
    c0, c1 = int(750 / cell), int(2250 / cell)
    return z[c0:c1, c0:c1]


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    (out / "ab_pairs").mkdir(parents=True, exist_ok=True)

    seeds = seeds_in(dump)
    t_seed = seeds[0]

    # ---- triptychs ------------------------------------------------------
    for biome in BIOMES:
        gen = np.frombuffer((dump / f"{biome}_{t_seed}.f32").read_bytes(), dtype="<f4")
        n = int(len(gen) ** 0.5)
        gen = gen.reshape(n, n).astype(np.float64)
        base = np.array(Image.open(dump / f"{biome}_{t_seed}_base.png").convert("L"))
        rz, rcell, rtid = real_tile(biome)
        panels = [
            ("S2 base", np.flipud(base[int(750 / 2):int(2250 / 2), int(750 / 2):int(2250 / 2)])),
            ("S3 amplified", hillshade(core(gen, 2.0), 2.0)),
            (f"REAL {rtid}", hillshade(core(rz, rcell), rcell)),
        ]
        w = 420
        sheet = Image.new("RGB", (3 * (w + 6) + 6, w + 30), (255, 255, 255))
        for i, (label, img) in enumerate(panels):
            im = Image.fromarray(img).convert("RGB").resize((w, w))
            sheet.paste(im, (6 + i * (w + 6), 24))
            d = ImageDraw.Draw(sheet)
            d.text((10 + i * (w + 6), 6), label, fill=(0, 0, 0))
        sheet.save(out / f"TRIPTYCH_{biome}.png")
        print(f"triptych {biome}")

    # ---- blind A/B pairs ------------------------------------------------
    rng = random.Random(20260815)  # bumped per review round: fresh real tiles + crops
    key = {}
    pair_i = 0
    for biome in BIOMES:
        for seed in seeds:
            f = dump / f"{biome}_{seed}.f32"
            if not f.exists():
                continue
            gen = np.frombuffer(f.read_bytes(), dtype="<f4")
            n = int(len(gen) ** 0.5)
            gen = gen.reshape(n, n).astype(np.float64)
            rz, rcell, rtid = real_tile(biome, idx=rng.randrange(6))
            # random 750 m crops from each core
            def crop(z, cell):
                w = int(750 / cell)
                c0, c1 = int(750 / cell), int(2250 / cell) - w
                y = rng.randrange(c0, max(c0 + 1, c1))
                x = rng.randrange(c0, max(c0 + 1, c1))
                return hillshade(z[y:y + w, x:x + w], cell)
            g_img = Image.fromarray(crop(gen, 2.0)).convert("RGB").resize((360, 360))
            r_img = Image.fromarray(crop(rz, rcell)).convert("RGB").resize((360, 360))
            gen_left = rng.random() < 0.5
            left, right = (g_img, r_img) if gen_left else (r_img, g_img)
            sheet = Image.new("RGB", (2 * 366 + 6, 390), (255, 255, 255))
            sheet.paste(left, (6, 24))
            sheet.paste(right, (372, 24))
            d = ImageDraw.Draw(sheet)
            name = f"pair_{pair_i:02d}"
            d.text((6, 4), f"{name}  ({biome})   which is REAL?", fill=(0, 0, 0))
            sheet.save(out / "ab_pairs" / f"{name}.png")
            key[name] = {"real": "left" if not gen_left else "right",
                         "biome": biome, "gen_seed": seed, "real_tile": rtid}
            pair_i += 1
    (out / "ab_pairs" / "ab_key.json").write_text(json.dumps(key, indent=1))
    print(f"{pair_i} A/B pairs -> {out/'ab_pairs'} (key in ab_key.json — don't peek)")


if __name__ == "__main__":
    main()
