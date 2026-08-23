#!/usr/bin/env python3
"""Build the Sandhills texture patch pack.

Phase 5 (`docs/sandhills/README.md`). The measured gap is `fine/band` — the
sub-64 m detail riding on the dune band — where real sandhills carries 0.34-0.36
and the untextured generator carries 0.21-0.26. That is the "crumpled paper"
character, and no dune-tier dial reaches it because it sits below the dune band
entirely.

WHY A NEW PACK RATHER THAN `dictionary_v2.bin`. Two reasons, both about
conditioning rather than data quality. The v2 dictionary keys partly on
`dist_channel_m`, which is meaningless on ground with no drainage; and it keys
on ABSOLUTE aspect, where an aeolian surface needs aspect RELATIVE TO WIND —
a windward ramp and a lee face are different fabrics, and which compass
direction they happen to face is not the point. It also predates the 30
Carolina tiles. The raw material for a better-conditioned pack is already on
disk in `extract_v2`, so this reads that directly.

    python3 tools/aeolian/build_patchpack.py [out.npz]
"""
from __future__ import annotations

import pathlib
import sys

import numpy as np
from scipy import ndimage

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "metrics", "aeolian"):
    sys.path.insert(0, str(_TOOLS / _p))

from macro_campaign import extract_v2                     # noqa: E402
from macro_campaign.cgrid import read_f32                 # noqa: E402
import dune_stats as ds                                   # noqa: E402

OUT = _TOOLS / "macro_campaign" / "out"
# ONE PACK PER MODE. The plan said so and the first build ignored it: pooling
# both put Carolina blackwater-creek gullies into the aeolian pack, and they
# pasted onto Nebraska dune ground as elongated dark gouges that no real dune
# tile carries. A dune flank and a creek bank are different fabrics even at the
# same slope, TPI and aspect -- the conditioning cannot tell them apart because
# the thing that separates them is the process, not the local geometry.
MODES = {"aeolian": ("sandhills",), "fluvial": ("sandhills_nc",)}

# --- patch geometry -----------------------------------------------------
# 96 m at 2 m. Comfortably larger than the 64 m band edge, so a patch carries
# whole features rather than fragments, and small enough that the conditioning
# is locally true across it.
PATCH = 48
STRIDE = PATCH // 2
CLEAN_MIN = 0.98
# Per-bucket cap, so one landform cannot dominate and the pack stays small.
CAP = 250          # a patch with any developed/levelled ground is out

# --- conditioning bins --------------------------------------------------
# Deliberately coarse. Every extra axis divides the sample, and 74 tiles is
# what there is. These four are the ones an aeolian surface can actually
# supply at generation time.
SLOPE_EDGES = (0.03, 0.09)                     # gentle / moderate / steep
TPI_EDGES = (-0.35, 0.35)                      # hollow / flank / crest
ASPECT_BINS = 4                                # windward, two cross, lee
N_SLOPE, N_TPI = len(SLOPE_EDGES) + 1, len(TPI_EDGES) + 1
N_BUCKET = N_SLOPE * N_TPI * ASPECT_BINS


def bucket(slope, tpi, aspect_rel):
    s = int(np.searchsorted(SLOPE_EDGES, slope))
    t = int(np.searchsorted(TPI_EDGES, tpi))
    a = int((aspect_rel % (2 * np.pi)) / (2 * np.pi) * ASPECT_BINS) % ASPECT_BINS
    return (s * N_TPI + t) * ASPECT_BINS + a


def tile_wind(tile_path):
    """Wind azimuth for a corpus tile, from its own dune crests.

    Crests run perpendicular to the wind, so the wind axis is the crest axis
    rotated by 90 deg. The corpus does not record a wind direction; this is
    the only way to make aspect-relative-to-wind meaningful on real ground.
    """
    z, (_, _, cell) = read_f32(str(tile_path))
    return np.radians(ds.spectral_axis_deg(z, cell) + 90.0)


def main():
    for mode, biomes in MODES.items():
        dst = _TOOLS.parent / "assets" / f"sandhills_patches_{mode}.npz"
        dst.parent.mkdir(parents=True, exist_ok=True)
        print(f"\n=== {mode} ({', '.join(biomes)}) ===")
        build_one(dst, biomes)


def build_one(dst, BIOMES):

    buckets = [[] for _ in range(N_BUCKET)]
    seen = [0] * N_BUCKET
    rng0 = np.random.default_rng(20260823)
    src = []
    for biome in BIOMES:
        tiles = [t for a, t in extract_v2.kept_tiles() if a == biome]
        for ti, t in enumerate(tiles):
            npz = OUT / "extract_v2" / biome / f"{t}.npz"
            if not npz.exists():
                continue
            d = np.load(npz)
            clean = d["clean"]
            # The residual is recomputed HERE, with the same band definition the
            # `fine/band` metric uses (a plain 64 m lowpass residual), rather
            # than reusing extract_v2's `fine`, which is a half-amplitude
            # Gaussian split and reads 1.68x weaker on the same tile.
            #
            # That mismatch is why the first pack needed a gain of 2.8 to hit
            # the metric -- and a gain of 2.8 does not just scale a statistic,
            # it makes every real blowout 2.8x deeper. The renders came back
            # with elongated gouges no real tile carries. Cutting patches in the
            # metric's own band puts the gain back at 1.0, where pasted lidar
            # keeps the amplitude it was measured at.
            zt, (_, _, tcell) = read_f32(str(OUT / "tiles" / biome / f"{t}.cgrid"))
            zt = zt.astype(np.float32)
            fine = zt - ndimage.gaussian_filter(zt, (64.0 / np.pi) / tcell)
            cond = d["cond8"].astype(np.float32)     # slope, tpi, relief_pos, aspect, dist
            wind = tile_wind(OUT / "tiles" / biome / f"{t}.cgrid")
            n_before = sum(len(b) for b in buckets)
            for y in range(0, fine.shape[0] - PATCH, STRIDE):
                for x in range(0, fine.shape[1] - PATCH, STRIDE):
                    if clean[y:y + PATCH, x:x + PATCH].mean() < CLEAN_MIN:
                        continue
                    cy, cx = (y + PATCH // 2) // 4, (x + PATCH // 2) // 4
                    if cy >= cond.shape[0] or cx >= cond.shape[1]:
                        continue
                    sl, tp, _, asp, _ = cond[cy, cx]
                    if not np.isfinite([sl, tp, asp]).all():
                        continue
                    p = fine[y:y + PATCH, x:x + PATCH]
                    if not np.isfinite(p).all():
                        continue
                    # REJECT patches whose deep features touch the edge.
                    #
                    # Measured on train_19 (2026-08-23): the pasted surface
                    # carried 70 elongated cuts by the gouge instrument --
                    # about the REAL count of 71 -- but ours were 55-58 m long
                    # against real 99-195 m, and their centres sat 6.3 m from
                    # the patch-stride grid against a random-expectation 12 m.
                    # A 96 m patch cuts through a 100-200 m blowout and the
                    # fragment is pasted with its upwind ramp and downwind
                    # apron amputated: an isolated black gash. The material
                    # was always real; the truncation was the artifact.
                    lo_thresh = -2.0 * max(float(p.std()), 0.05)
                    deep = p < lo_thresh
                    if deep.any():
                        edge = np.zeros_like(deep)
                        edge[0, :] = edge[-1, :] = True
                        edge[:, 0] = edge[:, -1] = True
                        edge[1, :] |= True; edge[-2, :] |= True
                        edge[:, 1] |= True; edge[:, -2] |= True
                        if (deep & edge).any():
                            continue
                    bi = bucket(sl, tp, float(asp) - wind)
                    # Reservoir-cap during collection: holding every candidate
                    # would be ~0.5 GB before the cap is applied.
                    b = buckets[bi]
                    if len(b) < CAP:
                        b.append(p.astype(np.float16))
                    else:
                        seen[bi] += 1
                        j = rng0.integers(0, seen[bi])
                        if j < CAP:
                            b[j] = p.astype(np.float16)
            src.append((biome, t, sum(len(b) for b in buckets) - n_before))
            print(f"  [{ti + 1:3d}] {biome}/{t}  +{src[-1][2]} patches", flush=True)

    counts = np.array([len(b) for b in buckets])
    print(f"\n{counts.sum()} patches across {N_BUCKET} buckets")
    print(f"  per bucket: min {counts.min()}  p10 {np.percentile(counts,10):.0f}  "
          f"median {np.median(counts):.0f}  max {counts.max()}")
    empty = int((counts == 0).sum())
    thin = int((counts < 30).sum())
    print(f"  empty {empty}   under-30 {thin}")

    packed, index = [], []
    for b in buckets:
        index.append((len(packed), len(b)))
        packed.extend(b)
    arr = np.stack(packed) if packed else np.zeros((0, PATCH, PATCH), np.float16)
    np.savez_compressed(dst, patches=arr, index=np.array(index, np.int32),
                        patch=PATCH, n_slope=N_SLOPE, n_tpi=N_TPI,
                        n_aspect=ASPECT_BINS,
                        slope_edges=np.array(SLOPE_EDGES), tpi_edges=np.array(TPI_EDGES))
    print(f"wrote {dst.name}  ({dst.stat().st_size/1e6:.1f} MB, {len(packed)} patches)")
    write_bin(dst.with_suffix(".bin"), arr, index)


SCALE = 1000.0     # millimetres; residuals are order +/- 1 m


def write_bin(path, arr, index):
    """SPACK1 — the runtime format.

    i16 millimetres rather than f32: residuals are order +/- 1 m, so a 1 mm
    quantum is far below anything the surface expresses, and it halves a pack
    that is already tens of MB.
    """
    import struct
    a = np.clip(np.nan_to_num(arr.astype(np.float32)) * SCALE, -32767, 32767).astype("<i2")
    with open(path, "wb") as f:
        f.write(b"SPACK1\0")
        f.write(struct.pack("<BHBBBxII", 1, PATCH, N_SLOPE, N_TPI, ASPECT_BINS,
                            len(index), a.shape[0]))
        f.write(struct.pack("<f", SCALE))
        f.write(np.asarray(SLOPE_EDGES, "<f4").tobytes())
        f.write(np.asarray(TPI_EDGES, "<f4").tobytes())
        f.write(np.asarray(index, "<u4").tobytes())
        f.write(a.tobytes())
    print(f"wrote {path}  ({path.stat().st_size/1e6:.1f} MB)")


if __name__ == "__main__":
    main()
