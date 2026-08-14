"""F2 — the patch dictionary builder: harvest → bucket → curate → bake.

One command builds the whole asset from the E5 staging:

    python tools/dictionary/build.py            # -> assets/dictionary_v2.bin
    python tools/dictionary/build.py --report   # rebuild report only

Design (docs/calibration/f2-dictionary-plan.md):
- Two levels per biome: mid band (64–400 m residual at 8 m, 256 m patches)
  and fine band (< 64 m at 2 m, 64 m patches); PATCH=32, 50% overlap —
  the spike's proven geometry.
- REAL PATCHES, not a basis (the spike falsified drawn-coefficient PCA).
- Conditioning buckets on the corpus's own quantiles: slope × TPI ×
  relief-pos × log-dist(channel) = 4×3×3×3 per biome per level.
- Curation: per-(bucket,tile) limit, tile-diversity subselect to the cap,
  NCC dedup, rectilinearity rejector (field-boundary berms survive the
  leveled-ground screen; axis-aligned line energy is the tell).
- Held-out tiles (every 7th per biome, ≥3) never harvested — F3's QA set.
- Bake: i16 heights + per-patch scale (gradients are recomputed at load;
  storing heights halves the size vs gradient pairs), JSON header with
  bucket edges/stats/equalizers, blake3 fingerprint sidecar.
"""
import argparse
import hashlib
import json
import pathlib
import struct
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import extract_v2  # noqa: E402

try:
    import blake3  # type: ignore
    def b3(data): return blake3.blake3(data).hexdigest()
except ImportError:  # sidecar recomputed by the Rust loader; sha256 tracked too
    def b3(data): return "sha256:" + hashlib.sha256(data).hexdigest()

OUT_ASSET = ROOT / "assets" / "dictionary_v2.bin"
OUT_REPORT = ROOT / "tools" / "dictionary" / "report.md"

PATCH = 32
STRIDE = 16
CAP = 16              # patches per bucket (size budget: ~2 KB each)
PER_TILE_CAP = 3      # no single tile floods a bucket
NCC_MAX = 0.92
CLEAN_MIN = 0.85
RECT_MAX = 0.22       # axis-aligned gradient-mass fraction above this = berm
DIVERSITY_MIN = 5     # distinct source tiles per bucket, else borrow
EDGE_BINS = (4, 3, 3, 3)   # slope, tpi, relief_pos, logdist
BIOMES = ["piedmont", "sandhills", "great_plains", "river_valley",
          "hill_country", "heathland"]
FMT_VERSION = 1


def kept_by_biome():
    out = {b: [] for b in BIOMES}
    for a, t in extract_v2.kept_tiles():
        out[a].append(t)
    for b in out:
        out[b].sort()
    return out


def split_holdout(tiles):
    hold = [t for i, t in enumerate(tiles) if i % 7 == 3]
    while len(hold) < 3 and len(hold) < len(tiles):
        for t in tiles:
            if t not in hold:
                hold.append(t)
                break
    train = [t for t in tiles if t not in hold]
    return train, hold


def load_tile(biome, tid):
    z = np.load(extract_v2.OUT / "extract_v2" / biome / f"{tid}.npz")
    return z


def patch_grid(n):
    xs = list(range(0, n - PATCH + 1, STRIDE))
    if xs[-1] != n - PATCH:
        xs.append(n - PATCH)
    return xs


def cond_vec(cond8, y8, x8, w8):
    c = cond8[y8:y8 + w8, x8:x8 + w8].reshape(-1, 5).astype(np.float64)
    m = np.nanmean(c, axis=0)
    return np.array([m[0], m[1], m[2], np.log1p(max(m[4], 0.0))])


def rect_frac(p):
    """Stripe detector v2: max orientation-bin share of gradient mass, ANY
    direction. v1 checked only the 0/90 axes and the S3 first-light render
    was full of 30-60 degree plow-row patches that had sailed through."""
    gy, gx = np.gradient(p)
    mag = np.hypot(gx, gy)
    tot = float(mag.sum())
    if tot < 1e-9:
        return 0.0
    ang = np.mod(np.degrees(np.arctan2(gy, gx)), 180.0)
    bins = (np.floor(ang / 15.0).astype(int) % 12).ravel()
    conc = np.bincount(bins, weights=mag.ravel(), minlength=12) / tot
    # stripes put mass in one bin AND its opposite-gradient twin: a single
    # 15-degree bin holding >30% of all gradient mass is not natural ground
    return float(conc.max())


def radial_power(p, nbins=16):
    f = np.fft.fftshift(np.abs(np.fft.fft2(p)) ** 2)
    n = p.shape[0]
    yy, xx = np.mgrid[0:n, 0:n]
    r = np.hypot(yy - n / 2, xx - n / 2)
    bins = np.linspace(0, n / 2, nbins + 1)
    out = np.zeros(nbins)
    for i in range(nbins):
        m = (r >= bins[i]) & (r < bins[i + 1])
        out[i] = float(f[m].mean()) if m.any() else 0.0
    return out


def level_arrays(npz, level):
    """-> (band, clean, cond_stride) at the level's resolution."""
    clean = npz["clean"]
    if level == "mid":
        band = npz["mid8"].astype(np.float32)
        cl = clean[::4, ::4]
        return band, cl, 1, 32  # cond8 window = 32 cells
    band = npz["fine"].astype(np.float32)
    return band, clean, 4, 8   # fine: y/4 into cond8, 8-cell window


def bucket_id(cv, edges):
    idx = 0
    for k, e in enumerate(edges):
        b = int(np.searchsorted(e, cv[k]))
        idx = idx * (len(e) + 1) + b
    return idx


def harvest_biome(biome, tiles):
    """-> per-level dict(edges, buckets={id: [(tile, y, x, patch f16, cond)]}, stats)."""
    result = {}
    for level in ("mid", "fine"):
        # ---- pass 1: conditioning quantiles over train tiles ----
        conds = []
        for tid in tiles:
            npz = load_tile(biome, tid)
            band, cl, cs, cw = level_arrays(npz, level)
            cond8 = npz["cond8"]
            ys, xs = patch_grid(band.shape[0]), patch_grid(band.shape[1])
            for y in ys[::2]:
                for x in xs[::2]:
                    if cl[y:y + PATCH, x:x + PATCH].mean() < CLEAN_MIN:
                        continue
                    conds.append(cond_vec(cond8, y // cs, x // cs, cw))
        conds = np.array(conds) if conds else np.zeros((1, 4))
        edges = []
        for k, nb in enumerate(EDGE_BINS):
            qs = np.quantile(conds[:, k], np.linspace(0, 1, nb + 1)[1:-1])
            edges.append([float(v) for v in qs])
        # ---- pass 2: streaming curation ----
        buckets = {}
        stats = {"cand": 0, "clean_rej": 0, "rect_rej": 0, "dup_rej": 0,
                 "tile_cap_rej": 0, "kept": 0}
        order = sorted(tiles, key=lambda t: hashlib.sha256(
            (biome + t).encode()).hexdigest())
        for tid in order:
            npz = load_tile(biome, tid)
            band, cl, cs, cw = level_arrays(npz, level)
            cond8 = npz["cond8"]
            per_tile_in_bucket = {}
            ys, xs = patch_grid(band.shape[0]), patch_grid(band.shape[1])
            for y in ys:
                for x in xs:
                    stats["cand"] += 1
                    if cl[y:y + PATCH, x:x + PATCH].mean() < CLEAN_MIN:
                        stats["clean_rej"] += 1
                        continue
                    p = band[y:y + PATCH, x:x + PATCH].astype(np.float32)
                    if not np.isfinite(p).all():
                        stats["clean_rej"] += 1
                        continue
                    p = p - p.mean()
                    rf = rect_frac(p)
                    if rf > RECT_MAX:
                        stats["rect_rej"] += 1
                        continue
                    cv = cond_vec(cond8, y // cs, x // cs, cw)
                    bid = bucket_id(cv, edges)
                    kept = buckets.setdefault(bid, [])
                    ptk = per_tile_in_bucket.setdefault(bid, 0)
                    if ptk >= PER_TILE_CAP:
                        stats["tile_cap_rej"] += 1
                        continue
                    if len(kept) >= CAP * 3:  # oversample; diversity-subselect later
                        continue
                    pn = p / max(float(np.linalg.norm(p)), 1e-9)
                    dup = False
                    for it in kept:
                        q = it[3]
                        qf = q.astype(np.float32)
                        qn = qf - qf.mean()
                        c = float(np.dot(pn.ravel(),
                                         (qn / max(float(np.linalg.norm(qn)), 1e-9)).ravel()))
                        if c > NCC_MAX:
                            dup = True
                            break
                    if dup:
                        stats["dup_rej"] += 1
                        continue
                    kept.append((tid, y, x, p.astype(np.float16), cv))
                    per_tile_in_bucket[bid] = ptk + 1
                    stats["kept"] += 1
        # ---- diversity subselect to CAP (round-robin by tile) ----
        for bid, kept in buckets.items():
            if len(kept) <= CAP:
                continue
            by_tile = {}
            for item in kept:
                by_tile.setdefault(item[0], []).append(item)
            sel = []
            while len(sel) < CAP and any(by_tile.values()):
                for tid in sorted(by_tile):
                    if by_tile[tid] and len(sel) < CAP:
                        sel.append(by_tile[tid].pop(0))
            buckets[bid] = sel
        result[level] = {"edges": edges, "buckets": buckets, "stats": stats}
    return result


def borrow_neighbours(level_data):
    """Buckets with < DIVERSITY_MIN source tiles borrow from conditioning-
    adjacent buckets; provenance recorded."""
    edges = level_data["edges"]
    dims = [len(e) + 1 for e in edges]
    buckets = level_data["buckets"]
    borrows = {}

    def unpack(bid):
        out = []
        for d in reversed(dims):
            out.append(bid % d)
            bid //= d
        return list(reversed(out))

    def pack(ix):
        bid = 0
        for k, d in enumerate(dims):
            bid = bid * d + ix[k]
        return bid

    for bid in list(buckets):
        tiles = {it[0] for it in buckets[bid]}
        if len(tiles) >= DIVERSITY_MIN:
            continue
        ix = unpack(bid)
        for dist in (1, 2):
            for k in range(4):
                for dd in (-dist, dist):
                    jx = list(ix)
                    jx[k] += dd
                    if not (0 <= jx[k] < dims[k]):
                        continue
                    nb = pack(jx)
                    for item in buckets.get(nb, []):
                        if len(buckets[bid]) >= CAP:
                            break
                        if item[0] in tiles and len(tiles) < DIVERSITY_MIN:
                            continue
                        buckets[bid].append(item + ("borrowed",))
                        tiles.add(item[0])
                        borrows[bid] = borrows.get(bid, 0) + 1
                if len(tiles) >= DIVERSITY_MIN and len(buckets[bid]) >= CAP:
                    break
            if len(tiles) >= DIVERSITY_MIN:
                break
    return borrows


def bake(all_data, holdouts):
    header = {"format_version": FMT_VERSION, "patch": PATCH, "cap": CAP,
              "edge_bins": list(EDGE_BINS),
              "cond_dims": ["lp_slope", "tpi", "relief_pos", "log1p_dist_channel"],
              "holdout_tiles": holdouts, "biomes": {}}
    blob = bytearray()
    for biome, levels in all_data.items():
        brec = {}
        for level, ld in levels.items():
            lrec = {"edges": ld["edges"], "cell_m": 8.0 if level == "mid" else 2.0,
                    "buckets": {}, "borrows": ld.get("borrows", {}),
                    "stats": ld["stats"]}
            for bid in sorted(ld["buckets"]):
                kept = ld["buckets"][bid]
                if not kept:
                    continue
                amps = [float(np.std(it[3].astype(np.float32))) for it in kept]
                eq = np.mean([radial_power(it[3].astype(np.float32)) for it in kept],
                             axis=0)
                entries = []
                for item in kept:
                    tid, y, x, p, cv = item[:5]
                    pf = p.astype(np.float32)
                    scale = float(np.abs(pf).max() / 32760.0) or 1e-9
                    q = np.round(pf / scale).astype(np.int16)
                    entries.append({"src": tid, "y": int(y), "x": int(x),
                                    "scale": scale, "offset": len(blob),
                                    "cond": [float(v) for v in cv],
                                    "borrowed": len(item) > 5})
                    blob.extend(q.tobytes())
                lrec["buckets"][str(bid)] = {
                    "amp_p25": float(np.percentile(amps, 25)),
                    "amp_p50": float(np.percentile(amps, 50)),
                    "amp_p75": float(np.percentile(amps, 75)),
                    "equalizer": [float(v) for v in eq],
                    "n_src_tiles": len({e["src"] for e in entries}),
                    "patches": entries,
                }
            brec[level] = lrec
        header["biomes"][biome] = brec
    hjson = json.dumps(header, sort_keys=True).encode()
    out = b"CDIC" + struct.pack("<II", FMT_VERSION, len(hjson)) + hjson + bytes(blob)
    OUT_ASSET.parent.mkdir(parents=True, exist_ok=True)
    OUT_ASSET.write_bytes(out)
    fp = b3(out)
    (OUT_ASSET.with_suffix(".fingerprint")).write_text(fp + "\n")
    return len(out), fp


def report(all_data, holdouts, size, fp):
    lines = ["# F2 dictionary build report\n"]
    lines.append(f"asset: `assets/dictionary_v2.bin` — {size/1e6:.1f} MB, "
                 f"fingerprint `{fp[:16]}…`\n")
    lines.append("| biome | level | candidates | clean-rej | rect-rej | dup-rej "
                 "| kept | buckets filled | <5-tile buckets (borrowed) |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    for biome, levels in all_data.items():
        for level, ld in levels.items():
            s = ld["stats"]
            nb = len([b for b in ld["buckets"].values() if b])
            weak = sum(1 for b in ld["buckets"].values()
                       if b and len({it[0] for it in b}) < DIVERSITY_MIN)
            total_kept = sum(len(b) for b in ld["buckets"].values())
            lines.append(
                f"| {biome} | {level} | {s['cand']} | {s['clean_rej']} | "
                f"{s['rect_rej']} | {s['dup_rej']} | {total_kept} | {nb} | "
                f"{weak} ({sum(ld.get('borrows', {}).values())} borrowed) |")
    lines.append("\nheld-out tiles (never harvested — F3's QA set):\n")
    for b, hs in holdouts.items():
        lines.append(f"- {b}: {', '.join(hs)}")
    OUT_REPORT.write_text("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--biome", default=None)
    args = ap.parse_args()
    tiles_by = kept_by_biome()
    holdouts = {}
    all_data = {}
    for biome in BIOMES:
        if args.biome and biome != args.biome:
            continue
        train, hold = split_holdout(tiles_by[biome])
        holdouts[biome] = hold
        print(f"[{biome}] train {len(train)} hold {len(hold)}", flush=True)
        data = harvest_biome(biome, train)
        for level in data:
            data[level]["borrows"] = borrow_neighbours(data[level])
            s = data[level]["stats"]
            print(f"  {level}: kept {s['kept']} of {s['cand']} "
                  f"(clean-rej {s['clean_rej']}, rect-rej {s['rect_rej']}, "
                  f"dup-rej {s['dup_rej']})", flush=True)
        all_data[biome] = data
    size, fp = bake(all_data, holdouts)
    report(all_data, holdouts, size, fp)
    print(f"baked {size/1e6:.1f} MB fingerprint {fp[:16]}…")


if __name__ == "__main__":
    main()
