"""Groove diagnostic: detect the elongated incision GROOVES a human reads
as gullies (the earlier slot detector marked disconnected 8-40 m pockets
and missed them — round-6 user report).

Detector (identical for generated and real tiles):
  black-hat = grey_closing(z, 8x8 cells /16 m) - z, threshold >= 0.25 m,
  connected components with NO length cap, kept if elongated
  (PCA major-extent / (area/major-extent) >= 3) and major-extent >= 24 m.
  The near-channel band is INCLUDED; each kept component is classified
  `connected` (its 10 m dilation touches the channel-corridor mask,
  transitively through other kept components) vs `isolated`.

Stats per tile (ours printed side-by-side with real holdouts):
  groove count & total skeleton-proxy length per km^2, connectivity
  fraction (connected length / total), black-hat depth percentiles
  inside grooves, and orientation alignment |cos(theta)| between each
  component's PCA axis and the local downslope aspect.

Usage:
  python tools/dictionary/groove_stats.py <stage_dump_dir> <out_dir>
      stage_dump_dir: <biome>_<seed>_{s2,s3,s4}.f32 + _chan8.u8 files
      (crates/course-transforms/examples/stage_dump.rs)
  Real holdouts are pulled from the dictionary's holdout list via the
  extract_v2 staging (full 2 m cgrid + cond8 dist_channel).
"""
import json
import pathlib
import struct
import sys

import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import cgrid, extract_v2  # noqa: E402

# ---- detector constants (frozen for reproducible acceptance re-runs) ----
BH_SIZE = 8            # closing structuring element, cells (16 m at 2 m)
BH_DEPTH_M = 0.25      # black-hat threshold
MIN_LEN_M = 24.0       # keep components at least this long
MIN_ELONG = 3.0        # major-extent / mean-width
MIN_AREA_M2 = 24.0
CONNECT_DILATE_CELLS = 5   # 10 m bridge for transitive connectivity
CORE_M = (750, 2250)   # analysis crop, matches the battery core crop
CELL = 2.0


def blackhat(z):
    return ndimage.grey_closing(z, size=(BH_SIZE, BH_SIZE)) - z


def components(z):
    """Kept groove components. Returns (labels, kept_ids, info dict)."""
    bh = blackhat(z)
    mask = bh >= BH_DEPTH_M
    lab, nl = ndimage.label(mask, structure=np.ones((3, 3)))
    kept, info = [], {}
    for i, sl in enumerate(ndimage.find_objects(lab), start=1):
        if sl is None:
            continue
        sub = lab[sl] == i
        area = sub.sum() * CELL * CELL
        if area < MIN_AREA_M2:
            continue
        ys, xs = np.nonzero(sub)
        pts = np.stack([ys, xs], 1).astype(np.float64)
        pts -= pts.mean(0)
        # PCA major axis: extent along it is the length proxy
        cov = pts.T @ pts / max(len(pts), 1)
        evals, evecs = np.linalg.eigh(cov)
        major = evecs[:, -1]
        proj = pts @ major
        length = (proj.max() - proj.min() + 1) * CELL
        width = area / max(length, CELL)
        if length < MIN_LEN_M or length / max(width, 1e-9) < MIN_ELONG:
            continue
        kept.append(i)
        info[i] = dict(
            length_m=length,
            width_m=width,
            depth_max=float(bh[sl][sub].max()),
            axis=np.arctan2(major[0], major[1]),  # dy, dx -> angle
            sl=sl,
        )
    return lab, kept, info, bh


def classify_connected(lab, kept, corridor):
    """Transitive connectivity: label the union of (kept components
    dilated by CONNECT_DILATE_CELLS) with the corridor; a component is
    `connected` iff it shares a union-label with any corridor cell."""
    kept_mask = np.isin(lab, kept)
    grown = ndimage.binary_dilation(
        kept_mask, iterations=CONNECT_DILATE_CELLS, structure=np.ones((3, 3))
    )
    union = grown | corridor
    ulab, _ = ndimage.label(union, structure=np.ones((3, 3)))
    corridor_labels = set(np.unique(ulab[corridor])) - {0}
    connected = set()
    for i in kept:
        ys, xs = np.nonzero(lab == i)
        if set(np.unique(ulab[ys, xs])) & corridor_labels:
            connected.add(i)
    return connected


def downslope_aspect(z, sigma_cells=16):
    zs = ndimage.gaussian_filter(z, sigma_cells)
    gy, gx = np.gradient(zs, CELL)
    return np.arctan2(-gy, -gx)  # direction of steepest DESCENT


def tile_stats(z, corridor, name):
    c0, c1 = int(CORE_M[0] / CELL), int(CORE_M[1] / CELL)
    zc = z[c0:c1, c0:c1]
    cc = corridor[c0:c1, c0:c1]
    lab, kept, info, bh = components(zc)
    connected = classify_connected(lab, kept, cc)
    area_km2 = ((c1 - c0) * CELL / 1000.0) ** 2
    tot_len = sum(info[i]["length_m"] for i in kept)
    con_len = sum(info[i]["length_m"] for i in kept if i in connected)
    asp = downslope_aspect(zc)
    aligns = []
    for i in kept:
        sl = info[i]["sl"]
        cy = (sl[0].start + sl[0].stop) // 2
        cx = (sl[1].start + sl[1].stop) // 2
        # axis is undirected: alignment = |cos| of the doubled-angle diff
        aligns.append(abs(np.cos(info[i]["axis"] - asp[cy, cx])))
    depths = np.array([info[i]["depth_max"] for i in kept]) if kept else np.array([0.0])
    return dict(
        name=name,
        grooves_per_km2=len(kept) / area_km2,
        length_m_per_km2=tot_len / area_km2,
        connectivity_frac=(con_len / tot_len) if tot_len > 0 else 0.0,
        depth_p50=float(np.percentile(depths, 50)),
        depth_p90=float(np.percentile(depths, 90)),
        align_mean=float(np.mean(aligns)) if aligns else 0.0,
        n=len(kept),
        n_connected=len(connected),
    ), (lab, kept, connected, c0)


def load_f32(p):
    z = np.frombuffer(pathlib.Path(p).read_bytes(), dtype="<f4").astype(np.float64)
    n = int(round(len(z) ** 0.5))
    return z.reshape(n, n)


def gen_corridor(dump, biome, seed, n):
    ch8 = np.frombuffer(
        (dump / f"{biome}_{seed}_chan8.u8").read_bytes(), dtype=np.uint8
    ).reshape(376, 376)
    ch2 = np.kron(ch8, np.ones((4, 4), dtype=np.uint8))[:n, :n] > 0
    return ndimage.binary_dilation(ch2, iterations=6)  # 12 m corridor skirt


def real_tiles(biome, k=3):
    b = open(ROOT / "assets" / "dictionary_v2.bin", "rb").read()
    hlen = struct.unpack("<I", b[8:12])[0]
    ho = json.loads(b[12:12 + hlen])["holdout_tiles"][biome]
    out = []
    for tid in ho[:k]:
        z, (_, _, cell) = cgrid.read_f32(
            extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid"
        )
        z = z.astype(np.float64)
        m = np.isfinite(z)
        z = np.where(m, z, np.nanmean(z[m]))
        npz = np.load(extract_v2.OUT / "extract_v2" / biome / f"{tid}.npz")
        dist = np.kron(npz["cond8"][..., 4].astype(np.float64), np.ones((4, 4)))
        dist = dist[: z.shape[0], : z.shape[1]]
        corridor = ndimage.binary_dilation(dist <= 8.0, iterations=2)
        out.append((tid, z, corridor))
    return out


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    overlays = {}
    for f in sorted(dump.glob("*_s4.f32")):
        stem = f.stem[: -len("_s4")]
        biome, seed = stem.rsplit("_", 1)
        z = load_f32(f)
        corridor = gen_corridor(dump, biome, seed, z.shape[0])
        # stage attribution: depth >= 0.15 m at that stage over >=50% of cells
        z2, z3 = load_f32(dump / f"{stem}_s2.f32"), load_f32(dump / f"{stem}_s3.f32")
        st, geom = tile_stats(z, corridor, f"GEN {stem}")
        lab, kept, connected, c0 = geom
        born = {"S2": 0, "S3": 0, "S4": 0}
        c1 = c0 + lab.shape[0]
        bh2, bh3 = blackhat(z2[c0:c1, c0:c1]), blackhat(z3[c0:c1, c0:c1])
        for i in kept:
            sub = lab == i
            if (bh2[sub] >= 0.15).mean() >= 0.5:
                born["S2"] += 1
            elif (bh3[sub] >= 0.15).mean() >= 0.5:
                born["S3"] += 1
            else:
                born["S4"] += 1
        st["born"] = born
        rows.append(st)
        overlays[stem] = (z, lab, kept, connected, c0)
    for biome in sorted({r["name"].split()[1].rsplit("_", 1)[0] for r in rows}):
        for tid, z, corridor in real_tiles(biome):
            st, geom = tile_stats(z, corridor, f"REAL {biome} {tid}")
            rows.append(st)
            overlays[f"real_{biome}_{tid}"] = (z, *geom)
    hdr = (
        f"{'tile':34s} {'n':>5s} {'/km2':>6s} {'len/km2':>8s} {'conn%':>6s} "
        f"{'d_p50':>6s} {'d_p90':>6s} {'align':>6s}  born S2/S3/S4"
    )
    lines = [hdr]
    for r in rows:
        b = r.get("born")
        btxt = f"{b['S2']}/{b['S3']}/{b['S4']}" if b else "-"
        lines.append(
            f"{r['name']:34s} {r['n']:5d} {r['grooves_per_km2']:6.1f} "
            f"{r['length_m_per_km2']:8.0f} {100*r['connectivity_frac']:6.1f} "
            f"{r['depth_p50']:6.2f} {r['depth_p90']:6.2f} {r['align_mean']:6.2f}  {btxt}"
        )
    (out / "groove_stats.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    np.save(out / "overlay_geoms.npy", np.array([0]))  # marker; overlays drawn by caller
    import pickle

    (out / "overlays.pkl").write_bytes(pickle.dumps(overlays))


if __name__ == "__main__":
    main()
